use crate::error::{AppError, AppResult};
use crate::models::{Metadata, ReplayGain, SortTags};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::probe::Probe;
use lofty::tag::items::popularimeter::{Popularimeter, StarRating};
use lofty::tag::{Accessor, ItemKey, Tag, TagType};
use std::path::Path;

/// アプリが扱う年の有効範囲
const YEAR_RANGE: std::ops::RangeInclusive<i32> = 1000..=9999;

/// トラック番号・ディスク番号と、その総数の有効範囲
const NUMBER_RANGE: std::ops::RangeInclusive<i32> = 1..=999;

/// BPMの有効範囲
const BPM_RANGE: std::ops::RangeInclusive<i32> = 1..=999;

/// タグから年を取得する
///
/// lofty 0.24 の `Timestamp` は年が未設定・不正なタグでも `year = 0` を返すことがあるため、
/// アプリが扱う範囲外の年は `None` に正規化する。
fn extract_year(tag: &Tag) -> Option<i32> {
    tag.date()
        .map(|d| d.year as i32)
        .filter(|y| YEAR_RANGE.contains(y))
}

/// タグからアルバムアーティストを取得する（空の値は、タグがないものとして扱う）
fn extract_album_artist(tag: &Tag) -> Option<String> {
    tag.get_string(ItemKey::AlbumArtist)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 音楽ファイルを読み込む
///
/// 形式は中身から判定し、判定できない場合は拡張子から決める（拡張子と中身が違うファイル、
/// たとえば`.ogg`の中がOpusのファイルも読めるようにする）。
fn read_tagged_file(file_path: &Path, options: ParseOptions) -> AppResult<TaggedFile> {
    Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .options(options)
        .guess_file_type()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))
}

/// 列を追加する前に登録したトラックについて、後からファイルのタグを読む項目
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BackfillTags {
    /// アルバムアーティスト（タグにない場合はNone）
    pub album_artist: Option<String>,
    /// 並び順に使う値（ソート用のタグ）
    pub sort_tags: SortTags,
}

/// 音楽ファイルのタグから、アルバムアーティストとソート用のタグだけを読む
///
/// それらの列を追加する前に登録したトラックの読み込み（`tag_backfill`）に使う。
/// 長さ・ビットレートなどは読まない（全ファイルを読むため、1ファイルあたりの時間を抑える）。
pub fn read_backfill_tags(file_path: &Path) -> AppResult<BackfillTags> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new().read_properties(false))?;

    Ok(tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
        .map(|tag| {
            let metadata = library_metadata(tag);
            BackfillTags {
                sort_tags: metadata.sort_tags(),
                album_artist: metadata.album_artist,
            }
        })
        .unwrap_or_default())
}

/// ゲインとして受け付ける範囲（dB）。範囲外の値は壊れたタグとして無視する
const GAIN_RANGE_DB: std::ops::RangeInclusive<f64> = -60.0..=60.0;

/// EBU R128（-23 LUFS）とReplayGain（-18 LUFS）の基準の差（dB）
const R128_TO_REPLAY_GAIN_DB: f64 = 5.0;

/// ReplayGainのゲインの値（例: "-6.48 dB"、"+1.2 dB"）を解釈する
fn parse_gain_db(value: &str) -> Option<f64> {
    let number = value.trim();
    let number = number
        .strip_suffix("dB")
        .or_else(|| number.strip_suffix("db"))
        .or_else(|| number.strip_suffix("DB"))
        .unwrap_or(number)
        .trim();
    let gain: f64 = number.parse().ok()?;
    (gain.is_finite() && GAIN_RANGE_DB.contains(&gain)).then_some(gain)
}

/// ReplayGainのピークの値（例: "0.988769"）を解釈する
fn parse_peak(value: &str) -> Option<f64> {
    let peak: f64 = value.trim().parse().ok()?;
    (peak.is_finite() && peak > 0.0).then_some(peak)
}

/// EBU R128のゲインの値（Q7.8の整数。例: "-1536" = -6 dB）を、ReplayGainの基準のdBに変換する
fn parse_r128_gain(value: &str) -> Option<f64> {
    let q78: i32 = value.trim().parse().ok()?;
    let gain = f64::from(q78) / 256.0 + R128_TO_REPLAY_GAIN_DB;
    GAIN_RANGE_DB.contains(&gain).then_some(gain)
}

/// タグから音量の正規化に使うゲインとピークを読み取る
///
/// ReplayGainのタグを優先し、ない場合はEBU R128のタグ（Opus・Vorbisコメント）を使う。
/// R128にはピークがないため、ピークはReplayGainのタグからだけ読み取る。
fn extract_replay_gain(tag: &Tag) -> ReplayGain {
    let gain = |replay_gain_key: ItemKey, r128_key: ItemKey| {
        tag.get_string(replay_gain_key)
            .and_then(parse_gain_db)
            .or_else(|| tag.get_string(r128_key).and_then(parse_r128_gain))
    };
    ReplayGain {
        track_gain: gain(ItemKey::ReplayGainTrackGain, ItemKey::R128TrackGain),
        track_peak: tag
            .get_string(ItemKey::ReplayGainTrackPeak)
            .and_then(parse_peak),
        album_gain: gain(ItemKey::ReplayGainAlbumGain, ItemKey::R128AlbumGain),
        album_peak: tag
            .get_string(ItemKey::ReplayGainAlbumPeak)
            .and_then(parse_peak),
    }
}

/// 評価として受け付ける範囲（0は評価なし）
pub const RATING_RANGE: std::ops::RangeInclusive<i32> = 0..=5;

/// Vorbisコメントの`RATING`の値の付け方
#[derive(Debug, Clone, Copy, PartialEq)]
enum VorbisRatingScale {
    /// 星の数をそのまま書く（1〜5）
    Stars,
    /// 20刻みで書く（20〜100。MusicBee・MediaMonkeyなど。こちらが多い）
    Percent,
}

/// Vorbisコメントの`RATING`の値（数値だけの文字列）を、星の数と付け方に解釈する
///
/// - 1〜5: 星の数
/// - 6〜100: 20刻み（半分の星は切り上げる。例: 90 → 5）
/// - 0より大きく1未満の小数: 0〜1の割合（例: 0.8 → 4）
///
/// 0・範囲外・数値でない値はNone（評価なし）。
fn parse_vorbis_rating(value: &str) -> Option<(i32, VorbisRatingScale)> {
    let number: f64 = value.trim().parse().ok()?;
    if !number.is_finite() || number <= 0.0 || number > 100.0 {
        return None;
    }
    if number < 1.0 {
        let stars = (number * 5.0).round().clamp(1.0, 5.0);
        return Some((stars as i32, VorbisRatingScale::Percent));
    }
    if number <= 5.0 {
        return Some((number.round() as i32, VorbisRatingScale::Stars));
    }
    let stars = (number / 20.0).ceil().clamp(1.0, 5.0);
    Some((stars as i32, VorbisRatingScale::Percent))
}

/// Vorbisコメント（FLACなど）の、数値だけの`RATING`を読む
///
/// lofty（0.25）は、Vorbisコメントの`RATING`を汎用タグへ文字列のまま入れるだけで、
/// `Tag::ratings()`では読めない（読めるのは`RATING:書き手`の形だけ）。多くのアプリは
/// 書き手を付けない`RATING=80`の形で書くため、ここで解釈する。
fn vorbis_plain_rating(tag: &Tag) -> Option<(i32, VorbisRatingScale)> {
    if tag.tag_type() != TagType::VorbisComments {
        return None;
    }
    tag.get_strings(ItemKey::Popularimeter)
        .find_map(parse_vorbis_rating)
}

/// タグから評価（星の数。1〜5）を読み取る。評価のタグがない場合は0
///
/// 評価の数値の付け方は書き込んだアプリごとに違うため、loftyが書き手（POPMのemailなど）に
/// 応じて星の数へ直したものを使う。複数ある場合は最初の評価を使う。
/// Vorbisコメントの数値だけの`RATING`は、`vorbis_plain_rating`で読む。
fn extract_rating(tag: &Tag) -> i32 {
    tag.ratings()
        .next()
        .map(|popularimeter| popularimeter.rating as i32)
        .or_else(|| vorbis_plain_rating(tag).map(|(stars, _)| stars))
        .unwrap_or(0)
}

/// 星の数（1〜5）をloftyの評価にする（0・範囲外は評価なし）
fn star_rating(rating: i32) -> Option<StarRating> {
    match rating {
        1 => Some(StarRating::One),
        2 => Some(StarRating::Two),
        3 => Some(StarRating::Three),
        4 => Some(StarRating::Four),
        5 => Some(StarRating::Five),
        _ => None,
    }
}

/// タグの評価を書き換える（0は評価のタグを取り除く）
///
/// すでに評価がある場合は、その書き手と再生回数を引き継ぎ、星の数だけを変える
/// （書き込んだアプリが、自分の付け方のまま読めるようにする）。ない場合は、対応するアプリが
/// 多いMusicBeeの付け方で書く（ID3v2は1・64・128・196・255、それ以外は20刻み）。
///
/// Vorbisコメント（FLACなど）は、書き手を付けない`RATING`に数値だけを書く。
/// lofty（0.25）は汎用タグの評価をVorbisコメントへ変換せずに書き出すため、loftyの形の
/// まま入れると、他のアプリが読めない値（`RATING=MusicBee|4|0`）がファイルに書かれる。
fn set_tag_rating(tag: &mut Tag, rating: i32) {
    if tag.tag_type() == TagType::VorbisComments {
        // すでに星の数（1〜5）で書かれている場合はその付け方を保ち、それ以外は20刻みで書く
        let scale = vorbis_plain_rating(tag).map(|(_, scale)| scale);
        tag.remove_key(ItemKey::Popularimeter);
        if star_rating(rating).is_some() {
            let value = match scale {
                Some(VorbisRatingScale::Stars) => rating,
                _ => rating * 20,
            };
            tag.insert_text(ItemKey::Popularimeter, value.to_string());
        }
        return;
    }

    let existing = tag.ratings().next();
    tag.remove_key(ItemKey::Popularimeter);

    let Some(star) = star_rating(rating) else {
        return;
    };
    let popularimeter = match existing {
        Some(current) => {
            Popularimeter::custom(current.email().to_string(), star, current.play_counter)
        }
        None => Popularimeter::musicbee(star, 0),
    };
    tag.insert_text(ItemKey::Popularimeter, popularimeter.to_string());
}

/// タグから、空でない文字列の値を取得する（空白だけの値は、タグがないものとして扱う）
fn extract_text(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

/// タグから、並び順に使う値（ソート用のタグ）を取得する（前後の空白は除く。空の値はタグがないものとして扱う）
///
/// ID3v2は`TSOT`・`TSOP`・`TSOA`・`TSO2`、Vorbisコメントは`TITLESORT`・`ARTISTSORT`・`ALBUMSORT`・
/// `ALBUMARTISTSORT`、MP4は`sonm`・`soar`・`soal`・`soaa`。
fn extract_sort_text(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// タグからディスク番号を取得する（`1/2`のように総数と一緒に書かれている場合は、番号だけ）
fn extract_disc_number(tag: &Tag) -> Option<i32> {
    tag.get_string(ItemKey::DiscNumber)
        .and_then(|s| {
            s.split('/')
                .next()
                .and_then(|n| n.trim().parse::<i32>().ok())
        })
        .or_else(|| tag.disk().map(|d| d as i32))
}

/// タグから、ライブラリ（データベース）に登録する項目を取り出す
///
/// インポート・再スキャンで全ファイルについて読むため、編集画面だけで使う項目（歌詞など）は
/// 取り出さない。
fn library_metadata(tag: &Tag) -> Metadata {
    Metadata {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        genre: tag.genre().map(|s| s.to_string()),
        year: extract_year(tag),
        track_number: tag.track().map(|t| t as i32),
        disc_number: extract_disc_number(tag),
        album_artist: extract_album_artist(tag),
        composer: tag.get_string(ItemKey::Composer).map(|s| s.to_string()),
        title_sort: extract_sort_text(tag, ItemKey::TrackTitleSortOrder),
        artist_sort: extract_sort_text(tag, ItemKey::TrackArtistSortOrder),
        album_sort: extract_sort_text(tag, ItemKey::AlbumTitleSortOrder),
        album_artist_sort: extract_sort_text(tag, ItemKey::AlbumArtistSortOrder),
        ..Default::default()
    }
}

/// タグから、編集画面で扱うすべての項目を取り出す
fn editor_metadata(tag: &Tag) -> Metadata {
    // BPMは、タグの種類によって整数の項目（ID3v2の`TBPM`・MP4の`tmpo`）か、小数もありうる
    // 項目（Vorbisコメントの`BPM`）に入っている。どちらも整数に丸めて扱う
    let bpm = tag
        .get_string(ItemKey::IntegerBpm)
        .or_else(|| tag.get_string(ItemKey::Bpm))
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(|bpm| bpm.round() as i32)
        .filter(|bpm| BPM_RANGE.contains(bpm));
    let compilation = tag
        .get_string(ItemKey::FlagCompilation)
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "True"))
        .filter(|&compilation| compilation);

    Metadata {
        track_total: tag.track_total().map(|t| t as i32),
        disc_total: tag.disk_total().map(|t| t as i32),
        grouping: extract_text(tag, ItemKey::ContentGroup),
        bpm,
        compilation,
        comment: tag
            .comment()
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty()),
        // 歌詞は、時刻のないもの（ID3v2の`USLT`など）を優先して読む
        lyrics: extract_text(tag, ItemKey::UnsyncLyrics)
            .or_else(|| extract_text(tag, ItemKey::Lyrics)),
        ..library_metadata(tag)
    }
}

/// 音楽ファイルのタグから、編集画面で扱うすべての項目を読む（タグがない項目は値なし）
///
/// 作曲者・コメント・歌詞などは、データベースに保存していないため、編集画面を開く時に
/// ファイルから読む。長さ・ビットレートなどは読まない。
pub fn read_file_tags(file_path: &Path) -> AppResult<Metadata> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new().read_properties(false))?;

    Ok(tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
        .map(editor_metadata)
        .unwrap_or_default())
}

/// 音楽ファイルに埋め込まれた画像（アルバムアート）
#[derive(Debug, Clone)]
pub struct EmbeddedPicture {
    /// 画像データ
    pub data: Vec<u8>,
    /// MIMEタイプ (image/jpeg, image/png など)
    pub mime_type: String,
}

/// ファイルから一括抽出された情報（メタデータ、時間、ビットレート、サンプルレート）
///
/// 1回のProbe::openで全情報を取得することで、ファイルI/Oを4回→1回に削減する。
pub struct FileInfo {
    pub metadata: Metadata,
    pub duration: Option<i32>,
    pub bitrate: Option<i32>,
    pub sample_rate: Option<i32>,
    pub replay_gain: ReplayGain,
    /// 評価（星の数。0は評価なし）
    pub rating: i32,
}

/// 音楽ファイルから全情報（タグ・長さ・ビットレート・サンプルレート・ReplayGain・評価）を
/// 1回のファイルオープンでまとめて抽出する
pub fn extract_all_file_info(file_path: &Path) -> AppResult<FileInfo> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new())?;

    // メタデータ抽出
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let metadata = tag.map(library_metadata).unwrap_or_default();

    let replay_gain = tag.map(extract_replay_gain).unwrap_or_default();
    let rating = tag.map(extract_rating).unwrap_or(0);

    // オーディオプロパティ抽出
    let properties = tagged_file.properties();
    let duration = Some(properties.duration().as_secs() as i32);
    let bitrate = properties.audio_bitrate().map(|b| b as i32);
    let sample_rate = properties.sample_rate().map(|s| s as i32);

    Ok(FileInfo {
        metadata,
        duration,
        bitrate,
        sample_rate,
        replay_gain,
        rating,
    })
}

/// 埋め込める画像・フォルダの画像として読む画像のサイズの上限
pub const MAX_ALBUM_ART_BYTES: u64 = 10 * 1024 * 1024;

/// 画像データの先頭から、画像の種類（MIMEタイプ）を判定する（JPEG・PNGだけを扱う）
///
/// 拡張子ではなく中身で判定する（拡張子と中身が違うファイルを、誤った種類で埋め込まないため）。
pub fn sniff_image_mime_type(data: &[u8]) -> Option<&'static str> {
    const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(PNG_SIGNATURE) {
        Some("image/png")
    } else {
        None
    }
}

/// 画像の幅と高さ（ピクセル）を読む（JPEG・PNG。読めない場合はNone）
pub fn image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let information = match sniff_image_mime_type(data)? {
        "image/png" => PictureInformation::from_png(data).ok()?,
        _ => PictureInformation::from_jpeg(data).ok()?,
    };
    (information.width > 0 && information.height > 0)
        .then_some((information.width, information.height))
}

/// アルバムアートとして表示する画像の位置（フロントカバー。なければ最初の画像）
fn displayed_picture_index(pictures: &[Picture]) -> Option<usize> {
    pictures
        .iter()
        .position(|p| p.pic_type() == PictureType::CoverFront)
        .or(if pictures.is_empty() { None } else { Some(0) })
}

/// 音楽ファイルからアルバムアートを抽出
pub fn extract_album_art(file_path: &Path) -> AppResult<Option<EmbeddedPicture>> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new().read_properties(false))?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    Ok(tag.and_then(|tag| {
        let pictures = tag.pictures();
        let picture = &pictures[displayed_picture_index(pictures)?];
        Some(EmbeddedPicture {
            data: picture.data().to_vec(),
            mime_type: picture
                .mime_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string()),
        })
    }))
}

/// 画像を、アルバムアートとして音楽ファイルに埋め込む
///
/// 表示している画像（フロントカバー。なければ最初の画像）を置き換える。裏ジャケットなどの
/// ほかの画像は残す。画像は、JPEG・PNGで、`MAX_ALBUM_ART_BYTES`以下であること。
pub fn set_file_album_art(file_path: &Path, picture: &EmbeddedPicture) -> AppResult<()> {
    let mime_type = validate_album_art(&picture.data)?;

    modify_file_tag(file_path, |tag| {
        let new_picture = Picture::unchecked(picture.data.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(mime_type)
            .build();
        match displayed_picture_index(tag.pictures()) {
            Some(index) => tag.set_picture(index, new_picture),
            None => tag.push_picture(new_picture),
        }
    })
}

/// 埋め込む画像を検証し、タグに書く画像の種類を返す
fn validate_album_art(data: &[u8]) -> AppResult<MimeType> {
    if data.len() as u64 > MAX_ALBUM_ART_BYTES {
        return Err(AppError::Validation(format!(
            "画像が大きすぎます（{}MBまで）",
            MAX_ALBUM_ART_BYTES / (1024 * 1024)
        )));
    }
    match sniff_image_mime_type(data) {
        Some("image/png") => Ok(MimeType::Png),
        Some(_) => Ok(MimeType::Jpeg),
        None => Err(AppError::Validation(
            "JPEGまたはPNGの画像を選んでください".to_string(),
        )),
    }
}

/// 音楽ファイルから、埋め込みの画像をすべて取り除く
///
/// 表示している画像だけを取り除くと、残った画像（裏ジャケットなど）がアルバムアートとして
/// 表示されるため、すべてのタグのすべての画像を取り除く。
///
/// @returns 画像を取り除いたか（埋め込みの画像がないファイルは書き換えず、falseを返す）
pub fn remove_file_album_art(file_path: &Path) -> AppResult<bool> {
    let mut tagged_file = read_tagged_file(file_path, ParseOptions::new())?;

    let tag_types: Vec<TagType> = tagged_file
        .tags()
        .iter()
        .filter(|tag| !tag.pictures().is_empty())
        .map(|tag| tag.tag_type())
        .collect();
    if tag_types.is_empty() {
        return Ok(false);
    }

    for tag_type in tag_types {
        if let Some(tag) = tagged_file.tag_mut(tag_type) {
            while !tag.pictures().is_empty() {
                tag.remove_picture(0);
            }
        }
    }

    tagged_file
        .save_to_path(file_path, WriteOptions::default())
        .map_err(|e| AppError::Metadata(format!("メタデータの保存に失敗しました: {}", e)))?;
    Ok(true)
}

/// メタデータをバリデーション
pub fn validate_metadata(metadata: &Metadata) -> AppResult<()> {
    // 年のバリデーション
    if let Some(year) = metadata.year
        && !YEAR_RANGE.contains(&year)
    {
        return Err(AppError::Validation(
            "年は1000から9999の範囲で指定してください".to_string(),
        ));
    }

    // トラック番号・ディスク番号と、その総数のバリデーション
    for (value, name) in [
        (metadata.track_number, "トラック番号"),
        (metadata.track_total, "トラックの総数"),
        (metadata.disc_number, "ディスク番号"),
        (metadata.disc_total, "ディスクの総数"),
    ] {
        if let Some(value) = value
            && !NUMBER_RANGE.contains(&value)
        {
            return Err(AppError::Validation(format!(
                "{}は1から999の範囲で指定してください",
                name
            )));
        }
    }

    if let Some(bpm) = metadata.bpm
        && !BPM_RANGE.contains(&bpm)
    {
        return Err(AppError::Validation(
            "BPMは1から999の範囲で指定してください".to_string(),
        ));
    }

    Ok(())
}

/// 音楽ファイルのタグを書き換えて保存する
///
/// タグがないファイルには、その形式の既定のタグを作る。
fn modify_file_tag(file_path: &Path, modify: impl FnOnce(&mut Tag)) -> AppResult<()> {
    // ファイルを読み込み
    let mut tagged_file = read_tagged_file(file_path, ParseOptions::new())?;

    // プライマリタグを取得または作成
    let tag = match tagged_file.primary_tag_mut() {
        Some(tag) => tag,
        None => {
            // その形式の既定のタグがない場合は新規作成する。別の種類のタグだけがある場合
            // （WAVのRIFF INFO、AIFFのテキストチャンクなど）は、その内容を引き継ぐ
            // （読み取りでは既定のタグを優先するため、引き継がないと、評価だけを書いた後に
            // タイトル・アーティストなどが読めなくなる）
            let tag_type = tagged_file.primary_tag_type();
            let mut tag = tagged_file
                .first_tag()
                .cloned()
                .unwrap_or_else(|| Tag::new(tag_type));
            tag.re_map(tag_type);
            tagged_file.insert_tag(tag);
            tagged_file
                .primary_tag_mut()
                .ok_or_else(|| AppError::Metadata("タグの作成に失敗しました".to_string()))?
        }
    };

    modify(tag);

    // ファイルに保存
    tagged_file
        .save_to_path(file_path, WriteOptions::default())
        .map_err(|e| AppError::Metadata(format!("メタデータの保存に失敗しました: {}", e)))?;

    Ok(())
}

/// 文字列の項目の書き換え方
enum TextChange<'a> {
    Set(&'a str),
    Remove,
    Keep,
}

/// 文字列の項目を、どう書き換えるかを決める
///
/// 値があればその値にする。空の値は、項目を取り除く指定として扱う（タグの一括ツールで、
/// 置換の結果が空になった場合）。値がなければ、`clear_missing`の時だけ取り除く。
fn text_change(value: &Option<String>, clear_missing: bool) -> TextChange<'_> {
    match value.as_deref() {
        Some(value) if !value.is_empty() => TextChange::Set(value),
        Some(_) => TextChange::Remove,
        None if clear_missing => TextChange::Remove,
        None => TextChange::Keep,
    }
}

/// タグの文字列の項目を書き換える（`text_change`を参照）
fn apply_text(tag: &mut Tag, key: ItemKey, value: &Option<String>, clear_missing: bool) {
    match text_change(value, clear_missing) {
        TextChange::Set(value) => {
            tag.insert_text(key, value.to_string());
        }
        TextChange::Remove => tag.remove_key(key),
        TextChange::Keep => {}
    }
}

/// タグにメタデータを反映する
///
/// 値がある項目はその値にする。`clear_missing`がtrueなら、値がない項目をタグから取り除く
/// （1曲の編集で、欄を空にして保存した場合。編集画面は、すべての項目の値を渡す）。
/// falseなら、値がない項目は変えない（一括編集・データベースの内容の書き出し）。
/// 文字列の項目（コメント・歌詞を除く）は、空の値を「取り除く」指定として扱う
/// （タグの一括ツールで、置換の結果が空になった場合。`text_change`を参照）。
fn apply_metadata(tag: &mut Tag, metadata: &Metadata, clear_missing: bool) {
    match text_change(&metadata.title, clear_missing) {
        TextChange::Set(title) => tag.set_title(title.to_string()),
        TextChange::Remove => tag.remove_title(),
        TextChange::Keep => {}
    }

    match text_change(&metadata.artist, clear_missing) {
        TextChange::Set(artist) => tag.set_artist(artist.to_string()),
        TextChange::Remove => tag.remove_artist(),
        TextChange::Keep => {}
    }

    match text_change(&metadata.album, clear_missing) {
        TextChange::Set(album) => tag.set_album(album.to_string()),
        TextChange::Remove => tag.remove_album(),
        TextChange::Keep => {}
    }

    match text_change(&metadata.genre, clear_missing) {
        TextChange::Set(genre) => tag.set_genre(genre.to_string()),
        TextChange::Remove => tag.remove_genre(),
        TextChange::Keep => {}
    }

    match metadata.year {
        Some(year) => {
            // lofty 0.24 で set_year が廃止されたため date/Timestamp を使う。
            // 既存の月日などを消さないよう、現在の Timestamp の年だけを差し替える。
            let mut timestamp = tag.date().unwrap_or_default();
            timestamp.year = year as u16;
            tag.set_date(timestamp);
        }
        None if clear_missing => tag.remove_date(),
        None => {}
    }

    match metadata.track_number {
        Some(track_number) => tag.set_track(track_number as u32),
        None if clear_missing => tag.remove_track(),
        None => {}
    }

    match metadata.track_total {
        Some(track_total) => tag.set_track_total(track_total as u32),
        None if clear_missing => tag.remove_track_total(),
        None => {}
    }

    match metadata.disc_number {
        Some(disc_number) => tag.set_disk(disc_number as u32),
        None if clear_missing => tag.remove_disk(),
        None => {}
    }

    match metadata.disc_total {
        Some(disc_total) => tag.set_disk_total(disc_total as u32),
        None if clear_missing => tag.remove_disk_total(),
        None => {}
    }

    apply_text(
        tag,
        ItemKey::AlbumArtist,
        &metadata.album_artist,
        clear_missing,
    );
    apply_text(tag, ItemKey::Composer, &metadata.composer, clear_missing);
    apply_text(
        tag,
        ItemKey::ContentGroup,
        &metadata.grouping,
        clear_missing,
    );
    for (key, value) in [
        (ItemKey::TrackTitleSortOrder, &metadata.title_sort),
        (ItemKey::TrackArtistSortOrder, &metadata.artist_sort),
        (ItemKey::AlbumTitleSortOrder, &metadata.album_sort),
        (ItemKey::AlbumArtistSortOrder, &metadata.album_artist_sort),
    ] {
        apply_text(tag, key, value, clear_missing);
    }

    match &metadata.comment {
        Some(comment) => tag.set_comment(comment.clone()),
        None if clear_missing => tag.remove_comment(),
        None => {}
    }

    // BPM: タグの種類によって、整数の項目（ID3v2・MP4）か、もう一方の項目（Vorbisコメント）に書く
    if metadata.bpm.is_some() || clear_missing {
        tag.remove_key(ItemKey::IntegerBpm);
        tag.remove_key(ItemKey::Bpm);
    }
    if let Some(bpm) = metadata.bpm
        && !tag.insert_text(ItemKey::IntegerBpm, bpm.to_string())
    {
        tag.insert_text(ItemKey::Bpm, bpm.to_string());
    }

    // コンピレーション: 印を付ける時だけ値を書き、外す時は項目ごと取り除く
    match metadata.compilation {
        Some(true) => {
            tag.insert_text(ItemKey::FlagCompilation, "1".to_string());
        }
        Some(false) => tag.remove_key(ItemKey::FlagCompilation),
        None if clear_missing => tag.remove_key(ItemKey::FlagCompilation),
        None => {}
    }

    // 歌詞: 時刻のない歌詞として書く。ID3v2は`USLT`（`UnsyncLyrics`）だけを持てるため、
    // 歌詞の項目（Vorbisコメントの`LYRICS`・MP4の`©lyr`）に書けない場合はそちらへ書く
    if metadata.lyrics.is_some() || clear_missing {
        tag.remove_key(ItemKey::Lyrics);
        tag.remove_key(ItemKey::UnsyncLyrics);
    }
    if let Some(lyrics) = &metadata.lyrics
        && !tag.insert_text(ItemKey::Lyrics, lyrics.clone())
    {
        tag.insert_text(ItemKey::UnsyncLyrics, lyrics.clone());
    }
}

/// 音楽ファイルのメタデータを更新
///
/// `clear_missing`の意味は`apply_metadata`を参照。
pub fn update_file_metadata(
    file_path: &Path,
    metadata: &Metadata,
    clear_missing: bool,
) -> AppResult<()> {
    // メタデータをバリデーション
    validate_metadata(metadata)?;

    modify_file_tag(file_path, |tag| {
        apply_metadata(tag, metadata, clear_missing)
    })
}

/// 音楽ファイルの評価を更新（0は評価のタグを取り除く）
pub fn update_file_rating(file_path: &Path, rating: i32) -> AppResult<()> {
    validate_rating(rating)?;

    modify_file_tag(file_path, |tag| set_tag_rating(tag, rating))
}

/// 音楽ファイルのメタデータと評価を、1回の保存でまとめて更新する
///
/// メタデータは値がある項目だけを変える。評価は`Some`の場合だけ変える。
pub fn update_file_metadata_and_rating(
    file_path: &Path,
    metadata: &Metadata,
    rating: Option<i32>,
) -> AppResult<()> {
    validate_metadata(metadata)?;
    if let Some(rating) = rating {
        validate_rating(rating)?;
    }

    modify_file_tag(file_path, |tag| {
        apply_metadata(tag, metadata, false);
        if let Some(rating) = rating {
            set_tag_rating(tag, rating);
        }
    })
}

/// 評価をバリデーション
pub fn validate_rating(rating: i32) -> AppResult<()> {
    if !RATING_RANGE.contains(&rating) {
        return Err(AppError::Validation(
            "レーティングは0から5の間で指定してください".to_string(),
        ));
    }
    Ok(())
}

/// テスト用の小さな画像・音楽ファイル（アルバムアートのテストで共有する）
#[cfg(test)]
pub(crate) mod test_images {
    use std::path::Path;

    /// 幅と高さだけを持つPNG（ヘッダーのみ。表示はできない）
    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        // ビット深度8・RGB・圧縮/フィルター/インターレースなし + CRC（検証されない）
        bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    /// 幅と高さだけを持つJPEG（ヘッダーのみ。表示はできない）
    pub fn jpeg(width: u16, height: u16) -> Vec<u8> {
        // SOI + APP0（JFIF）
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        bytes.extend_from_slice(b"JFIF\0");
        bytes.extend_from_slice(&[1, 1, 0, 0, 1, 0, 1, 0, 0]);
        // SOF0（精度8ビット・高さ・幅・3成分）
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        // EOI
        bytes.extend_from_slice(&[0xFF, 0xD9]);
        bytes
    }

    /// 最小のWAVファイル（無音・タグなし）を作る
    pub fn write_wav(path: &Path) {
        let samples: u32 = 16;
        let data_len = samples * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // モノラル
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, data_len as usize));
        std::fs::write(path, bytes).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_images::{jpeg, png};
    use super::*;

    #[test]
    fn test_extract_all_file_info_nonexistent_file() {
        let result = extract_all_file_info(Path::new("nonexistent.mp3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_gain_db() {
        assert_eq!(parse_gain_db("-6.48 dB"), Some(-6.48));
        assert_eq!(parse_gain_db("+1.20 dB"), Some(1.2));
        assert_eq!(parse_gain_db(" -3.5dB "), Some(-3.5));
        assert_eq!(parse_gain_db("2"), Some(2.0));
        // 壊れた値・ありえない値は無視する
        assert_eq!(parse_gain_db("loud"), None);
        assert_eq!(parse_gain_db("NaN dB"), None);
        assert_eq!(parse_gain_db("-120 dB"), None);
    }

    #[test]
    fn test_parse_peak() {
        assert_eq!(parse_peak("0.988769"), Some(0.988769));
        assert_eq!(parse_peak("1.2"), Some(1.2));
        assert_eq!(parse_peak("0"), None);
        assert_eq!(parse_peak("-1"), None);
        assert_eq!(parse_peak("peak"), None);
    }

    #[test]
    fn test_parse_r128_gain_converts_to_replay_gain_reference() {
        // -6 dB（R128の基準） = -1 dB（ReplayGainの基準）
        assert_eq!(parse_r128_gain("-1536"), Some(-1.0));
        assert_eq!(parse_r128_gain("0"), Some(5.0));
        assert_eq!(parse_r128_gain("-6.5"), None);
    }

    #[test]
    fn test_extract_replay_gain_prefers_replay_gain_over_r128() {
        use lofty::tag::{ItemValue, TagItem, TagType};

        let mut tag = Tag::new(TagType::VorbisComments);
        let mut set = |key: ItemKey, value: &str| {
            tag.insert(TagItem::new(key, ItemValue::Text(value.to_string())));
        };
        set(ItemKey::ReplayGainTrackGain, "-7.00 dB");
        set(ItemKey::ReplayGainTrackPeak, "0.95");
        set(ItemKey::R128TrackGain, "-512");
        set(ItemKey::R128AlbumGain, "-1024");

        assert_eq!(
            extract_replay_gain(&tag),
            ReplayGain {
                track_gain: Some(-7.0),
                track_peak: Some(0.95),
                // アルバムはReplayGainのタグがないため、R128から変換する（-4 + 5 = 1 dB）
                album_gain: Some(1.0),
                album_peak: None,
            }
        );
    }

    #[test]
    fn test_tag_rating_round_trip() {
        for tag_type in [
            TagType::Id3v2,
            TagType::VorbisComments,
            TagType::Mp4Ilst,
            TagType::RiffInfo,
        ] {
            let mut tag = Tag::new(tag_type);
            assert_eq!(extract_rating(&tag), 0);

            for rating in 1..=5 {
                set_tag_rating(&mut tag, rating);
                assert_eq!(extract_rating(&tag), rating);
                // 書き直しても評価は1つだけ
                assert_eq!(tag.get_strings(ItemKey::Popularimeter).count(), 1);
            }

            // 0は評価のタグを取り除く
            set_tag_rating(&mut tag, 0);
            assert_eq!(extract_rating(&tag), 0);
            assert_eq!(tag.get_strings(ItemKey::Popularimeter).count(), 0);
        }
    }

    /// 他のアプリが書いた評価は、書き手と再生回数を引き継いで星の数だけを変える
    #[test]
    fn test_set_tag_rating_keeps_existing_writer_and_play_counter() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(
            ItemKey::Popularimeter,
            Popularimeter::windows_media_player(StarRating::Two, 7).to_string(),
        );

        set_tag_rating(&mut tag, 4);

        let rating = tag.ratings().next().expect("評価が残っていること");
        assert_eq!(rating.rating as i32, 4);
        assert_eq!(rating.play_counter, 7);
        assert_eq!(rating.email(), "Windows Media Player 9 Series");
    }

    #[test]
    fn test_parse_vorbis_rating() {
        use VorbisRatingScale::{Percent, Stars};

        // 20刻み（半分の星は切り上げる）
        assert_eq!(parse_vorbis_rating("20"), Some((1, Percent)));
        assert_eq!(parse_vorbis_rating("80"), Some((4, Percent)));
        assert_eq!(parse_vorbis_rating("90"), Some((5, Percent)));
        assert_eq!(parse_vorbis_rating("100"), Some((5, Percent)));
        assert_eq!(parse_vorbis_rating("10"), Some((1, Percent)));
        // 星の数
        assert_eq!(parse_vorbis_rating("1"), Some((1, Stars)));
        assert_eq!(parse_vorbis_rating(" 4 "), Some((4, Stars)));
        assert_eq!(parse_vorbis_rating("5"), Some((5, Stars)));
        // 0〜1の割合
        assert_eq!(parse_vorbis_rating("0.8"), Some((4, Percent)));
        assert_eq!(parse_vorbis_rating("0.1"), Some((1, Percent)));
        // 評価なし・不正な値
        assert_eq!(parse_vorbis_rating("0"), None);
        assert_eq!(parse_vorbis_rating("101"), None);
        assert_eq!(parse_vorbis_rating("-1"), None);
        assert_eq!(parse_vorbis_rating("MusicBee|4|0"), None);
        assert_eq!(parse_vorbis_rating(""), None);
    }

    /// Vorbisコメントには、他のアプリが読める数値だけの`RATING`を書く
    #[test]
    fn test_vorbis_rating_is_written_as_plain_number() {
        use lofty::tag::TagExt;

        let dump = |tag: &Tag| {
            let mut bytes = Vec::new();
            tag.dump_to(&mut bytes, WriteOptions::default()).unwrap();
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let mut tag = Tag::new(TagType::VorbisComments);
        set_tag_rating(&mut tag, 4);
        let written = dump(&tag);
        assert!(written.contains("RATING=80"), "{written:?}");
        assert!(!written.contains("MusicBee"), "{written:?}");
        assert_eq!(extract_rating(&tag), 4);

        // 評価を外すと、RATINGそのものがなくなる
        set_tag_rating(&mut tag, 0);
        assert!(!dump(&tag).contains("RATING"));
    }

    /// 他のアプリが書いた数値だけの`RATING`を読み、星の数で書かれていればその付け方を保つ
    #[test]
    fn test_vorbis_rating_reads_plain_values_and_keeps_star_scale() {
        // 20刻み（MusicBeeなど）
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "60".to_string());
        assert_eq!(extract_rating(&tag), 3);
        set_tag_rating(&mut tag, 5);
        assert_eq!(
            tag.get_string(ItemKey::Popularimeter),
            Some("100"),
            "20刻みのまま書く"
        );

        // 星の数
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "4".to_string());
        assert_eq!(extract_rating(&tag), 4);
        set_tag_rating(&mut tag, 2);
        assert_eq!(
            tag.get_string(ItemKey::Popularimeter),
            Some("2"),
            "星の数のまま書く"
        );
        assert_eq!(extract_rating(&tag), 2);

        // 以前のバージョンが書いたloftyの形も読め、書き直すと数値だけになる
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "MusicBee|3|0".to_string());
        assert_eq!(extract_rating(&tag), 3);
        set_tag_rating(&mut tag, 3);
        assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("60"));

        // 数値だけの値の解釈は、Vorbisコメントだけに使う
        let mut id3 = Tag::new(TagType::Id3v2);
        id3.insert_text(ItemKey::Popularimeter, "80".to_string());
        assert_eq!(extract_rating(&id3), 0);
    }

    #[test]
    fn test_apply_metadata_clears_missing_fields_only_when_asked() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Old Title".to_string());
        tag.set_artist("Old Artist".to_string());
        tag.set_genre("Rock".to_string());

        let metadata = Metadata {
            title: Some("New Title".to_string()),
            artist: None,
            album: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
            ..Default::default()
        };

        // 一括編集: 値がない項目は変えない
        apply_metadata(&mut tag, &metadata, false);
        assert_eq!(tag.title().as_deref(), Some("New Title"));
        assert_eq!(tag.artist().as_deref(), Some("Old Artist"));
        assert_eq!(tag.genre().as_deref(), Some("Rock"));

        // 1曲の編集: 空にした項目はタグから取り除く
        apply_metadata(&mut tag, &metadata, true);
        assert_eq!(tag.title().as_deref(), Some("New Title"));
        assert_eq!(tag.artist(), None);
        assert_eq!(tag.genre(), None);
    }

    /// 編集画面のすべての項目に値を入れたメタデータ
    fn full_metadata() -> Metadata {
        Metadata {
            title: Some("タイトル".to_string()),
            artist: Some("アーティスト".to_string()),
            album: Some("アルバム".to_string()),
            genre: Some("Jazz".to_string()),
            year: Some(2021),
            track_number: Some(3),
            disc_number: Some(1),
            album_artist: Some("アルバムアーティスト".to_string()),
            composer: Some("作曲者".to_string()),
            track_total: Some(12),
            disc_total: Some(2),
            grouping: Some("グループ".to_string()),
            bpm: Some(128),
            compilation: Some(true),
            comment: Some("コメント\n2行目".to_string()),
            lyrics: Some("歌詞の1行目\n歌詞の2行目".to_string()),
            title_sort: Some("たいとる".to_string()),
            artist_sort: Some("あーてぃすと".to_string()),
            album_sort: Some("あるばむ".to_string()),
            album_artist_sort: Some("あるばむあーてぃすと".to_string()),
        }
    }

    /// 読み書きを確かめる形式（タグの種類が違うもの）のファイルを、一時フォルダに用意する
    ///
    /// WAV（ID3v2）は生成し、MP3（ID3v2）・M4A（MP4）・Ogg Vorbis / Opus（Vorbisコメント）は、
    /// 再生エンジンのテスト用のファイルを写す。
    fn tag_test_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        let mut files = vec![dir.join("generated.wav")];
        write_test_wav(&files[0]);
        for name in ["lame.mp3", "apple_aac.m4a", "vorbis.ogg", "opus.opus"] {
            let file = dir.join(name);
            std::fs::copy(fixtures.join(name), &file).unwrap();
            files.push(file);
        }
        files
    }

    /// 編集画面のすべての項目を書き込み、読み直して同じ値を取得できる（タグの種類ごと）
    #[test]
    fn test_every_editor_field_round_trips_in_every_tag_type() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for file in tag_test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let metadata = full_metadata();

            update_file_metadata(&file, &metadata, true).unwrap();
            assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");
            // 取り込みで読む項目（一覧に使う項目）も、同じ値になる
            let library = extract_all_file_info(&file).unwrap().metadata;
            assert_eq!(library.track_number, Some(3), "{name}");
            assert_eq!(library.disc_number, Some(1), "{name}");
            assert_eq!(
                library.album_artist.as_deref(),
                Some("アルバムアーティスト"),
                "{name}"
            );
            assert_eq!(library.sort_tags(), metadata.sort_tags(), "{name}");
            // 列を追加する前に登録したトラックの読み込みでも、同じ値を読む
            assert_eq!(
                read_backfill_tags(&file).unwrap(),
                BackfillTags {
                    album_artist: Some("アルバムアーティスト".to_string()),
                    sort_tags: SortTags {
                        title: Some("たいとる".to_string()),
                        artist: Some("あーてぃすと".to_string()),
                        album: Some("あるばむ".to_string()),
                        album_artist: Some("あるばむあーてぃすと".to_string()),
                    },
                },
                "{name}"
            );

            // 一括編集: 値のある項目だけを変える
            let partial = Metadata {
                composer: Some("別の作曲者".to_string()),
                compilation: Some(false),
                ..Default::default()
            };
            update_file_metadata(&file, &partial, false).unwrap();
            let expected = Metadata {
                composer: Some("別の作曲者".to_string()),
                compilation: None,
                ..metadata.clone()
            };
            assert_eq!(read_file_tags(&file).unwrap(), expected, "{name}");

            // 空の値は、その項目だけをタグから取り除く（タグの一括ツールで、置換の結果が空になった場合）
            let removal = Metadata {
                genre: Some(String::new()),
                album_artist: Some(String::new()),
                artist_sort: Some(String::new()),
                ..Default::default()
            };
            update_file_metadata(&file, &removal, false).unwrap();
            let expected = Metadata {
                genre: None,
                album_artist: None,
                artist_sort: None,
                ..expected
            };
            assert_eq!(read_file_tags(&file).unwrap(), expected, "{name}");

            // 1曲の編集: 空にした項目は、タグから取り除く
            let cleared = Metadata {
                title: Some("タイトルだけ".to_string()),
                ..Default::default()
            };
            update_file_metadata(&file, &cleared, true).unwrap();
            assert_eq!(read_file_tags(&file).unwrap(), cleared, "{name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn embedded(data: Vec<u8>, mime_type: &str) -> EmbeddedPicture {
        EmbeddedPicture {
            data,
            mime_type: mime_type.to_string(),
        }
    }

    #[test]
    fn test_sniff_image_mime_type_and_dimensions() {
        assert_eq!(sniff_image_mime_type(&jpeg(4, 3)), Some("image/jpeg"));
        assert_eq!(sniff_image_mime_type(&png(4, 3)), Some("image/png"));
        assert_eq!(sniff_image_mime_type(b"GIF89a"), None);
        assert_eq!(sniff_image_mime_type(b""), None);

        assert_eq!(image_dimensions(&jpeg(1200, 900)), Some((1200, 900)));
        assert_eq!(image_dimensions(&png(500, 700)), Some((500, 700)));
        // 途中で切れた画像・画像ではないデータは、大きさを読めない
        assert_eq!(image_dimensions(&jpeg(1200, 900)[..8]), None);
        assert_eq!(image_dimensions(&png(500, 700)[..12]), None);
        assert_eq!(image_dimensions(b"not an image"), None);
    }

    /// 画像を埋め込み・置き換え・取り除きでき、ほかのタグは変わらない（タグの種類ごと）
    #[test]
    fn test_album_art_round_trips_in_every_tag_type() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for file in tag_test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let metadata = full_metadata();
            update_file_metadata(&file, &metadata, true).unwrap();
            assert!(extract_album_art(&file).unwrap().is_none(), "{name}");
            // 埋め込みの画像がないファイルは、書き換えない
            assert!(!remove_file_album_art(&file).unwrap(), "{name}");

            // 埋め込む
            set_file_album_art(&file, &embedded(jpeg(600, 600), "image/jpeg")).unwrap();
            let art = extract_album_art(&file).unwrap().unwrap();
            assert_eq!(art.data, jpeg(600, 600), "{name}");
            assert_eq!(art.mime_type, "image/jpeg", "{name}");

            // 置き換える（画像は増えない。種類は中身で決める）
            set_file_album_art(&file, &embedded(png(800, 800), "image/jpeg")).unwrap();
            let art = extract_album_art(&file).unwrap().unwrap();
            assert_eq!(art.data, png(800, 800), "{name}");
            assert_eq!(art.mime_type, "image/png", "{name}");
            let tagged = read_tagged_file(&file, ParseOptions::new()).unwrap();
            assert_eq!(tagged.primary_tag().unwrap().pictures().len(), 1, "{name}");

            // ほかのタグは変わらない
            assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");

            // 取り除く
            assert!(remove_file_album_art(&file).unwrap(), "{name}");
            assert!(extract_album_art(&file).unwrap().is_none(), "{name}");
            assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 埋め込む時は表示している画像（フロントカバー）だけを置き換え、取り除く時はすべて取り除く
    #[test]
    fn test_album_art_replaces_front_cover_and_removes_all_pictures() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);

        // 裏ジャケット・フロントカバーの順に入っているファイル
        modify_file_tag(&file, |tag| {
            for (pic_type, data) in [
                (PictureType::CoverBack, jpeg(10, 10)),
                (PictureType::CoverFront, jpeg(20, 20)),
            ] {
                tag.push_picture(
                    Picture::unchecked(data)
                        .pic_type(pic_type)
                        .mime_type(MimeType::Jpeg)
                        .build(),
                );
            }
        })
        .unwrap();
        assert_eq!(
            extract_album_art(&file).unwrap().unwrap().data,
            jpeg(20, 20)
        );

        set_file_album_art(&file, &embedded(png(30, 30), "image/png")).unwrap();
        assert_eq!(extract_album_art(&file).unwrap().unwrap().data, png(30, 30));
        let tagged = read_tagged_file(&file, ParseOptions::new()).unwrap();
        let pictures = tagged.primary_tag().unwrap().pictures();
        assert_eq!(pictures.len(), 2);
        assert_eq!(pictures[0].pic_type(), PictureType::CoverBack);
        assert_eq!(pictures[0].data(), jpeg(10, 10));

        assert!(remove_file_album_art(&file).unwrap());
        let tagged = read_tagged_file(&file, ParseOptions::new()).unwrap();
        assert!(tagged.tags().iter().all(|tag| tag.pictures().is_empty()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_set_file_album_art_rejects_unusable_images() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);
        let before = std::fs::read(&file).unwrap();

        // JPEGでもPNGでもないデータ
        let error = set_file_album_art(&file, &embedded(b"GIF89a....".to_vec(), "image/gif"));
        assert!(matches!(error, Err(AppError::Validation(_))));
        // 大きすぎる画像
        let mut huge = jpeg(10, 10);
        huge.resize(MAX_ALBUM_ART_BYTES as usize + 1, 0);
        let error = set_file_album_art(&file, &embedded(huge, "image/jpeg"));
        assert!(matches!(error, Err(AppError::Validation(_))));

        // ファイルは書き換えない
        assert_eq!(std::fs::read(&file).unwrap(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_read_file_tags_of_a_file_without_tags_or_missing() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);

        assert_eq!(read_file_tags(&file).unwrap(), Metadata::default());
        assert!(read_file_tags(&dir.join("missing.wav")).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 実際のファイルへすべての項目を書き込み、ほかのツール（ffprobeなど）で読めるか確かめる
    /// （手動で実行する）
    ///
    /// `TAG_TEST_DIR=<音楽ファイルを置いたフォルダ> cargo test write_tags_to_real_files -- --ignored`
    #[test]
    #[ignore = "実際のファイルが必要（TAG_TEST_DIRで指定する。ファイルを書き換える）"]
    fn write_tags_to_real_files() {
        let dir = std::env::var("TAG_TEST_DIR").expect("TAG_TEST_DIRを指定する");
        let metadata = full_metadata();
        for entry in std::fs::read_dir(dir).unwrap() {
            let file = entry.unwrap().path();
            if !file.is_file() {
                continue;
            }
            update_file_metadata(&file, &metadata, true).unwrap();
            let read = read_file_tags(&file).unwrap();
            println!("{}: {}", file.display(), read == metadata);
            assert_eq!(read, metadata, "{}", file.display());
        }
    }

    /// 実際のファイルへ画像を埋め込み・取り除き、ほかのツール（ffprobeなど）で読めるか確かめる
    /// （手動で実行する）
    ///
    /// `TAG_TEST_DIR=<音楽ファイルを置いたフォルダ> ART_TEST_IMAGE=<画像> ART_TEST_STEP=<embed|remove>
    /// cargo test write_album_art_to_real_files -- --ignored`
    #[test]
    #[ignore = "実際のファイルが必要（TAG_TEST_DIRで指定する。ファイルを書き換える）"]
    fn write_album_art_to_real_files() {
        let dir = std::env::var("TAG_TEST_DIR").expect("TAG_TEST_DIRを指定する");
        let step = std::env::var("ART_TEST_STEP").unwrap_or_else(|_| "embed".to_string());
        let image = (step == "embed").then(|| {
            let path = std::env::var("ART_TEST_IMAGE").expect("ART_TEST_IMAGEを指定する");
            let data = std::fs::read(path).unwrap();
            let mime_type = sniff_image_mime_type(&data).expect("JPEGかPNGを指定する");
            embedded(data, mime_type)
        });
        for entry in std::fs::read_dir(dir).unwrap() {
            let file = entry.unwrap().path();
            if !file.is_file() {
                continue;
            }
            let before = read_file_tags(&file).unwrap();
            match &image {
                Some(image) => {
                    set_file_album_art(&file, image).unwrap();
                    let art = extract_album_art(&file).unwrap().unwrap();
                    assert_eq!(art.data, image.data, "{}", file.display());
                    assert_eq!(art.mime_type, image.mime_type, "{}", file.display());
                }
                None => {
                    remove_file_album_art(&file).unwrap();
                    assert!(extract_album_art(&file).unwrap().is_none());
                }
            }
            assert_eq!(read_file_tags(&file).unwrap(), before, "{}", file.display());
            println!("{}: {step}", file.display());
        }
    }

    /// 最小のWAVファイル（無音）を作る
    fn write_test_wav(path: &Path) {
        super::test_images::write_wav(path);
    }

    /// その形式の既定のタグがなく、別の種類のタグだけがあるファイルに評価を書いても、
    /// 元のタグの内容を読める（WAVのRIFF INFOだけを持つファイル。ffmpegなどが書く形）
    #[test]
    fn test_rating_write_keeps_fields_of_other_tag_type() {
        use lofty::tag::TagExt;

        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);
        let mut info_tag = Tag::new(TagType::RiffInfo);
        info_tag.set_title("Info Title".to_string());
        info_tag.set_artist("Info Artist".to_string());
        info_tag
            .save_to_path(&file, WriteOptions::default())
            .unwrap();
        let before = extract_all_file_info(&file).unwrap();
        assert_eq!(before.metadata.title.as_deref(), Some("Info Title"));

        update_file_rating(&file, 3).unwrap();

        // 評価は既定のタグ（ID3v2）に書く。既定のタグが優先して読まれても、元の内容が残る
        let after = extract_all_file_info(&file).unwrap();
        assert_eq!(after.rating, 3);
        assert_eq!(after.metadata.title.as_deref(), Some("Info Title"));
        assert_eq!(after.metadata.artist.as_deref(), Some("Info Artist"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 拡張子と中身が違うファイルも、中身から形式を判定して読み書きできる
    #[test]
    fn test_file_type_is_detected_from_content() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 中身はWAVで、拡張子がflacのファイル
        let file = dir.join("a.flac");
        write_test_wav(&file);

        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.sample_rate, Some(8000));
        assert_eq!(read_backfill_tags(&file).unwrap(), BackfillTags::default());

        update_file_rating(&file, 5).unwrap();
        assert_eq!(extract_all_file_info(&file).unwrap().rating, 5);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// ファイルへ書き込んだメタデータと評価を、読み直して取得できる
    #[test]
    fn test_file_metadata_and_rating_round_trip() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);

        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.metadata.title, None);
        assert_eq!(info.rating, 0);

        let metadata = Metadata {
            title: Some("Title".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            genre: Some("Jazz".to_string()),
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
            ..Default::default()
        };
        update_file_metadata(&file, &metadata, true).unwrap();
        update_file_rating(&file, 4).unwrap();

        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Title"));
        assert_eq!(info.metadata.artist.as_deref(), Some("Artist"));
        assert_eq!(info.metadata.album.as_deref(), Some("Album"));
        assert_eq!(info.metadata.genre.as_deref(), Some("Jazz"));
        assert_eq!(info.rating, 4);

        // 評価を外しても、他の項目は残る
        update_file_rating(&file, 0).unwrap();
        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.rating, 0);
        assert_eq!(info.metadata.title.as_deref(), Some("Title"));

        // まとめて書き込む（値がある項目だけ）
        let partial = Metadata {
            title: None,
            artist: Some("Other".to_string()),
            ..metadata.clone()
        };
        update_file_metadata_and_rating(&file, &partial, Some(2)).unwrap();
        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Title"));
        assert_eq!(info.metadata.artist.as_deref(), Some("Other"));
        assert_eq!(info.rating, 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_validate_rating() {
        assert!(validate_rating(0).is_ok());
        assert!(validate_rating(5).is_ok());
        assert!(validate_rating(-1).is_err());
        assert!(validate_rating(6).is_err());
    }

    #[test]
    fn test_validate_metadata_valid() {
        let metadata = Metadata {
            title: Some("Test Title".to_string()),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            genre: Some("Rock".to_string()),
            year: Some(2023),
            track_number: Some(1),
            disc_number: Some(1),
            album_artist: None,
            composer: None,
            ..Default::default()
        };

        assert!(validate_metadata(&metadata).is_ok());
    }

    #[test]
    fn test_validate_metadata_invalid_year() {
        let metadata = Metadata {
            title: None,
            artist: None,
            album: None,
            genre: None,
            year: Some(999),
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
            ..Default::default()
        };

        assert!(validate_metadata(&metadata).is_err());
    }

    #[test]
    fn test_validate_metadata_checks_numbers_and_bpm() {
        assert!(validate_metadata(&full_metadata()).is_ok());
        for invalid in [
            Metadata {
                track_total: Some(0),
                ..Default::default()
            },
            Metadata {
                disc_number: Some(1000),
                ..Default::default()
            },
            Metadata {
                disc_total: Some(-1),
                ..Default::default()
            },
            Metadata {
                bpm: Some(0),
                ..Default::default()
            },
            Metadata {
                bpm: Some(1000),
                ..Default::default()
            },
        ] {
            assert!(
                matches!(validate_metadata(&invalid), Err(AppError::Validation(_))),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn test_validate_metadata_invalid_track_number() {
        let metadata = Metadata {
            title: None,
            artist: None,
            album: None,
            genre: None,
            year: None,
            track_number: Some(1000),
            disc_number: None,
            album_artist: None,
            composer: None,
            ..Default::default()
        };

        assert!(validate_metadata(&metadata).is_err());
    }
}
