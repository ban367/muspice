use crate::error::{AppError, AppResult};
use crate::models::{Metadata, ReplayGain};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::items::popularimeter::{Popularimeter, StarRating};
use lofty::tag::{Accessor, ItemKey, Tag, TagType};
use std::path::Path;

/// アプリが扱う年の有効範囲
const YEAR_RANGE: std::ops::RangeInclusive<i32> = 1000..=9999;

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

/// 音楽ファイルのタグから、アルバムアーティストだけを読む（タグにない場合はNone）
///
/// アルバムアーティストの列を追加する前に登録したトラックの読み込みに使う。
/// 長さ・ビットレートなどは読まない（全ファイルを読むため、1ファイルあたりの時間を抑える）。
pub fn read_album_artist(file_path: &Path) -> AppResult<Option<String>> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .options(ParseOptions::new().read_properties(false))
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?;

    Ok(tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
        .and_then(extract_album_artist))
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
    let tagged_file = Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?;

    // メタデータ抽出
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let metadata = if let Some(tag) = tag {
        let disc_number = tag
            .get_string(ItemKey::DiscNumber)
            .and_then(|s| {
                s.split('/')
                    .next()
                    .and_then(|n| n.trim().parse::<i32>().ok())
            })
            .or_else(|| tag.disk().map(|d| d as i32));

        Metadata {
            title: tag.title().map(|s| s.to_string()),
            artist: tag.artist().map(|s| s.to_string()),
            album: tag.album().map(|s| s.to_string()),
            genre: tag.genre().map(|s| s.to_string()),
            year: extract_year(tag),
            track_number: tag.track().map(|t| t as i32),
            disc_number,
            album_artist: extract_album_artist(tag),
            composer: tag.get_string(ItemKey::Composer).map(|s| s.to_string()),
        }
    } else {
        Metadata {
            title: None,
            artist: None,
            album: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            album_artist: None,
            composer: None,
        }
    };

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

/// 音楽ファイルからアルバムアートを抽出
pub fn extract_album_art(file_path: &Path) -> AppResult<Option<EmbeddedPicture>> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    if let Some(tag) = tag {
        // フロントカバーを優先的に探す
        let pictures = tag.pictures();

        // フロントカバーを探す
        let front_cover = pictures
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront);

        // フロントカバーがなければ最初の画像を使用
        let picture = front_cover.or_else(|| pictures.first());

        if let Some(pic) = picture {
            let mime_type = pic
                .mime_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());

            return Ok(Some(EmbeddedPicture {
                data: pic.data().to_vec(),
                mime_type,
            }));
        }
    }

    Ok(None)
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

    // トラック番号のバリデーション
    if let Some(track_number) = metadata.track_number
        && !(1..=999).contains(&track_number)
    {
        return Err(AppError::Validation(
            "トラック番号は1から999の範囲で指定してください".to_string(),
        ));
    }

    Ok(())
}

/// 音楽ファイルのタグを書き換えて保存する
///
/// タグがないファイルには、その形式の既定のタグを作る。
fn modify_file_tag(file_path: &Path, modify: impl FnOnce(&mut Tag)) -> AppResult<()> {
    // ファイルを読み込み
    let mut tagged_file = Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?;

    // プライマリタグを取得または作成
    let tag = match tagged_file.primary_tag_mut() {
        Some(tag) => tag,
        None => {
            // タグが存在しない場合は新規作成
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(Tag::new(tag_type));
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

/// タグにメタデータを反映する
///
/// 値がある項目はその値にする。`clear_missing`がtrueなら、編集画面の項目
/// （タイトル・アーティスト・アルバム・ジャンル・年）のうち値がないものをタグから取り除く
/// （1曲の編集で、欄を空にして保存した場合）。falseなら、値がない項目は変えない（一括編集）。
fn apply_metadata(tag: &mut Tag, metadata: &Metadata, clear_missing: bool) {
    match &metadata.title {
        Some(title) => tag.set_title(title.clone()),
        None if clear_missing => tag.remove_title(),
        None => {}
    }

    match &metadata.artist {
        Some(artist) => tag.set_artist(artist.clone()),
        None if clear_missing => tag.remove_artist(),
        None => {}
    }

    match &metadata.album {
        Some(album) => tag.set_album(album.clone()),
        None if clear_missing => tag.remove_album(),
        None => {}
    }

    match &metadata.genre {
        Some(genre) => tag.set_genre(genre.clone()),
        None if clear_missing => tag.remove_genre(),
        None => {}
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

    if let Some(track_number) = metadata.track_number {
        tag.set_track(track_number as u32);
    }

    if let Some(album_artist) = &metadata.album_artist {
        tag.insert_text(ItemKey::AlbumArtist, album_artist.clone());
    }

    if let Some(composer) = &metadata.composer {
        tag.insert_text(ItemKey::Composer, composer.clone());
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

#[cfg(test)]
mod tests {
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

    /// 最小のWAVファイル（無音）を作る
    fn write_test_wav(path: &Path) {
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
        };

        assert!(validate_metadata(&metadata).is_err());
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
        };

        assert!(validate_metadata(&metadata).is_err());
    }
}
