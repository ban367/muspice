use crate::ape_tag;
use crate::error::{AppError, AppResult};
use crate::models::{Metadata, ReplayGain, SortTags};
use crate::multi_value;
use crate::ogg_flac;
use crate::vorbis_tag;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::probe::Probe;
use lofty::tag::items::popularimeter::{Popularimeter, StarRating};
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagExt, TagItem, TagType};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

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
///
/// APEv2タグを持つ形式（WavPack・Monkey's Audio）は、汎用タグで読めない項目（BPM・評価・画像）を
/// 足して返す（`ape_tag`）。Oggの中のFLACは、loftyが判定できないため、先に確かめてFLACとして
/// 読む（`ogg_flac`）。
fn read_tagged_file(file_path: &Path, options: ParseOptions) -> AppResult<TaggedFile> {
    let read_error = |e: &dyn std::fmt::Display| {
        AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e))
    };
    let mut reader = BufReader::new(
        File::open(file_path)
            .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?,
    );
    if ogg_flac::is_ogg_flac(&mut reader).map_err(|e| read_error(&e))? {
        return ogg_flac::read_file(&mut reader, options).map_err(|e| read_error(&e));
    }

    // 中身から判定できなければ、拡張子から決めた形式で読む（`Probe::open`と同じ）
    let mut probe = Probe::new(reader).options(options);
    if let Some(file_type) = FileType::from_path(file_path) {
        probe = probe.set_file_type(file_type);
    }
    let probe = probe.guess_file_type().map_err(|e| read_error(&e))?;

    match probe.file_type() {
        Some(file_type) if ape_tag::uses_ape_tag(file_type) => {
            ape_tag::read_file(&mut probe.into_inner(), file_type, options)
        }
        _ => probe.read(),
    }
    .map_err(|e| read_error(&e))
}

/// 読み込んだファイルのタグを、ファイルへ保存する
///
/// loftyの汎用タグは、VorbisコメントとAPEv2タグを保存する時に、タグ全体を汎用タグの内容で
/// 置き換える（対応する項目がないものが、ファイルから消える）。この2つは、アプリが扱わない項目を
/// 残して書き込む（`vorbis_tag::save_tag`・`ape_tag::save_tag`）。ID3v2・MP4は、loftyが
/// 対応のない項目を残すため、そのまま保存する。
///
/// 書き込みは、元のファイルの写しに行ってから置き換える（`write_atomically`。ADR-074）。
fn save_tagged_file(tagged_file: &TaggedFile, file_path: &Path) -> AppResult<()> {
    let file_type = tagged_file.file_type();
    write_atomically(file_path, |temp| {
        tagged_file
            .tags()
            .iter()
            .try_for_each(|tag| match tag.tag_type() {
                TagType::Ape if ape_tag::uses_ape_tag(file_type) => {
                    ape_tag::save_tag(temp, file_type, tag)
                }
                TagType::VorbisComments if vorbis_tag::uses_vorbis_comments(file_type) => {
                    vorbis_tag::save_tag(temp, file_type, tag)
                }
                // 読めるだけで書けない種類のタグ（FLACのID3v2など）は、そのままにする
                tag_type if tagged_file.tag_support(tag_type).is_writable() => {
                    tag.save_to_path(temp, WriteOptions::default())
                }
                _ => Ok(()),
            })
            .map_err(|e| AppError::Metadata(format!("メタデータの保存に失敗しました: {}", e)))
    })
}

/// 音楽ファイルを書き換える処理を、同じフォルダの写しで行い、終わってから元のファイルと置き換える
///
/// タグを書く処理（loftyと、Ogg FLACの自前の処理）は、ファイルを切り詰めてから全体を書き戻す。
/// 元のファイルへ直接書くと、書いている途中でアプリが落ちる・ディスクが一杯になった時に、利用者の
/// 音楽ファイルが壊れたまま残る。そのため、元のファイルを写した一時ファイルへ書き、ディスクへ
/// 書き出してから、`rename`で置き換える（同じフォルダのため、置き換えは一度に行われる）。
/// 途中で失敗した場合は一時ファイルを消し、元のファイルはそのまま残る（ADR-074）。
///
/// - 一時ファイルは、名前を`.`で始める（ライブラリの走査・フォルダの監視が読み飛ばす）。拡張子は
///   元のまま（形式を拡張子で決める処理があるため）
/// - 写すのは`fs::copy`（macOSのAPFSではクローンになり、大きなファイルでもすぐに終わる。権限・
///   拡張属性も写すため、置き換えた後も元のファイルと同じ）
/// - シンボリックリンクは、リンクの先のファイルを置き換える（リンクは残す）
/// - ハードリンクは、置き換えたパスだけが新しい内容になる
fn write_atomically(file_path: &Path, write: impl FnOnce(&Path) -> AppResult<()>) -> AppResult<()> {
    let target = fs::canonicalize(file_path)
        .map_err(|e| AppError::Io(format!("ファイルを開けません: {}", e)))?;
    let temp = temp_path_for(&target);
    let result = fs::copy(&target, &temp)
        .map_err(|e| {
            AppError::Io(format!(
                "タグを書き込む前に、ファイルを写せません（空き容量・フォルダへの書き込みの権限を確かめてください）: {}",
                e
            ))
        })
        .and_then(|_| write(&temp))
        .and_then(|()| {
            // 置き換える前に、ディスクへ書き出す（電源が落ちても、置き換えた後のファイルが欠けない）
            File::open(&temp)
                .and_then(|file| file.sync_all())
                .map_err(|e| AppError::Io(format!("書き込んだファイルを保存できません: {}", e)))
        })
        .and_then(|()| {
            fs::rename(&temp, &target).map_err(|e| {
                AppError::Io(format!("書き込んだファイルで置き換えられません: {}", e))
            })
        });
    if result.is_err() {
        // 写しは消す（元のファイルは、そのまま）
        let _ = fs::remove_file(&temp);
    }
    result
}

/// 書き換える時の一時ファイル（同じフォルダの、`.`で始まる名前。拡張子は元のまま）
fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(path.file_stem().unwrap_or_default());
    name.push(format!(".muspice-{}", uuid::Uuid::new_v4().simple()));
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    path.with_file_name(name)
}

/// 列を追加する前に登録したトラックについて、後からファイルのタグを読む項目
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BackfillTags {
    /// アルバムアーティスト（タグにない場合はNone）
    pub album_artist: Option<String>,
    /// 並び順に使う値（ソート用のタグ）
    pub sort_tags: SortTags,
    /// アーティスト・ジャンル（複数ある場合は、`; `でつないだ文字列）
    pub artist: Option<String>,
    pub genre: Option<String>,
}

/// 音楽ファイルのタグから、アルバムアーティスト・ソート用のタグ・アーティスト・ジャンルだけを読む
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
                artist: metadata.artist,
                genre: metadata.genre,
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

/// 数値だけの`RATING`（Vorbisコメント・APEv2タグ）の値の付け方
#[derive(Debug, Clone, Copy, PartialEq)]
enum PlainRatingScale {
    /// 星の数をそのまま書く（1〜5）
    Stars,
    /// 20刻みで書く（20〜100。MusicBee・MediaMonkeyなど。こちらが多い）
    Percent,
}

/// `RATING`の値（数値だけの文字列）を、星の数と付け方に解釈する
///
/// - 1〜5: 星の数
/// - 6〜100: 20刻み（半分の星は切り上げる。例: 90 → 5）
/// - 0より大きく1未満の小数: 0〜1の割合（例: 0.8 → 4）
///
/// 0・範囲外・数値でない値はNone（評価なし）。
fn parse_plain_rating(value: &str) -> Option<(i32, PlainRatingScale)> {
    let number: f64 = value.trim().parse().ok()?;
    if !number.is_finite() || number <= 0.0 || number > 100.0 {
        return None;
    }
    if number < 1.0 {
        let stars = (number * 5.0).round().clamp(1.0, 5.0);
        return Some((stars as i32, PlainRatingScale::Percent));
    }
    if number <= 5.0 {
        return Some((number.round() as i32, PlainRatingScale::Stars));
    }
    let stars = (number / 20.0).ceil().clamp(1.0, 5.0);
    Some((stars as i32, PlainRatingScale::Percent))
}

/// 評価を、数値だけの`RATING`として読み書きするタグの種類か
///
/// Vorbisコメント（FLAC・Oggなど）とAPEv2タグ（WavPack・Monkey's Audio）。どちらも、多くのアプリが
/// 書き手を付けない`RATING=80`の形で書く。
fn uses_plain_rating(tag_type: TagType) -> bool {
    matches!(tag_type, TagType::VorbisComments | TagType::Ape)
}

/// 数値だけの`RATING`を読む
///
/// lofty（0.25）は、Vorbisコメントの`RATING`を汎用タグへ文字列のまま入れるだけで、
/// `Tag::ratings()`では読めない（読めるのは`RATING:書き手`の形だけ）。APEv2タグの`RATING`は、
/// `ape_tag`が同じ形で汎用タグへ入れる。
fn plain_rating(tag: &Tag) -> Option<(i32, PlainRatingScale)> {
    if !uses_plain_rating(tag.tag_type()) {
        return None;
    }
    tag.get_strings(ItemKey::Popularimeter)
        .find_map(parse_plain_rating)
}

/// タグから評価（星の数。1〜5）を読み取る。評価のタグがない場合は0
///
/// 評価の数値の付け方は書き込んだアプリごとに違うため、loftyが書き手（POPMのemailなど）に
/// 応じて星の数へ直したものを使う。複数ある場合は最初の評価を使う。
/// 数値だけの`RATING`（Vorbisコメント・APEv2タグ）は、`plain_rating`で読む。
fn extract_rating(tag: &Tag) -> i32 {
    tag.ratings()
        .next()
        .map(|popularimeter| popularimeter.rating as i32)
        .or_else(|| plain_rating(tag).map(|(stars, _)| stars))
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
/// Vorbisコメント（FLACなど）とAPEv2タグは、書き手を付けない`RATING`に数値だけを書く。
/// lofty（0.25）は汎用タグの評価をVorbisコメントへ変換せずに書き出すため、loftyの形の
/// まま入れると、他のアプリが読めない値（`RATING=MusicBee|4|0`）がファイルに書かれる。
/// APEv2タグは、loftyに対応する項目がないため、検査なしで入れる（保存時に`ape_tag`が書く）。
fn set_tag_rating(tag: &mut Tag, rating: i32) {
    if uses_plain_rating(tag.tag_type()) {
        // すでに星の数（1〜5）で書かれている場合はその付け方を保ち、それ以外は20刻みで書く
        let scale = plain_rating(tag).map(|(_, scale)| scale);
        tag.remove_key(ItemKey::Popularimeter);
        if star_rating(rating).is_some() {
            let value = match scale {
                Some(PlainRatingScale::Stars) => rating,
                _ => rating * 20,
            };
            tag.insert_unchecked(TagItem::new(
                ItemKey::Popularimeter,
                ItemValue::Text(value.to_string()),
            ));
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

/// タグから、複数の値を持てる項目（アーティスト・ジャンル）を取得する
///
/// 同じ項目が複数ある場合と、1つの値に`;`で区切って書かれている場合は、値を`; `でつないだ
/// 文字列にする（`multi_value`）。複数の値でない項目は、そのままの文字列。
fn extract_multi_value(tag: &Tag, key: ItemKey) -> Option<String> {
    let items: Vec<&str> = tag.get_strings(key).collect();
    multi_value::display(&items)
}

/// タグから、ライブラリ（データベース）に登録する項目を取り出す
///
/// インポート・再スキャンで全ファイルについて読むため、編集画面だけで使う項目（歌詞など）は
/// 取り出さない。
fn library_metadata(tag: &Tag) -> Metadata {
    Metadata {
        title: tag.title().map(|s| s.to_string()),
        artist: extract_multi_value(tag, ItemKey::TrackArtist),
        album: tag.album().map(|s| s.to_string()),
        genre: extract_multi_value(tag, ItemKey::Genre),
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
    // 項目（Vorbisコメント・APEv2タグの`BPM`）に入っている。どちらも整数に丸めて扱う
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

/// 音楽ファイルのタグから、表示する歌詞を読む（歌詞がなければ値なし）
///
/// 歌詞の項目が2つある場合（時刻のないもの・そうでないもの）は、時刻付き（LRC形式）のものを
/// 優先する（再生位置に合わせて表示できるため）。どちらも時刻付きでなければ、編集画面と同じく
/// 時刻のないものを優先する。
pub fn read_file_lyrics(file_path: &Path) -> AppResult<Option<String>> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new().read_properties(false))?;
    let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    else {
        return Ok(None);
    };

    let candidates = [
        extract_text(tag, ItemKey::UnsyncLyrics),
        extract_text(tag, ItemKey::Lyrics),
    ];
    let timed = candidates
        .iter()
        .flatten()
        .find(|text| crate::lyrics::has_timestamps(text));
    Ok(timed.or(candidates.iter().flatten().next()).cloned())
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

    save_tagged_file(&tagged_file, file_path)?;
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
    modify_file_tag_of_type(file_path, |tag, _| {
        modify(tag);
        Ok(true)
    })
    .map(|_| ())
}

/// 音楽ファイルのタグを書き換えて保存する（書き換える処理に、ファイルの形式も渡す）
///
/// 書き換える処理は、タグを変えたかを返す。変えなかった場合と、エラーを返した場合は、保存しない。
/// @returns ファイルへ保存したか
fn modify_file_tag_of_type(
    file_path: &Path,
    modify: impl FnOnce(&mut Tag, FileType) -> AppResult<bool>,
) -> AppResult<bool> {
    // ファイルを読み込み
    let mut tagged_file = read_tagged_file(file_path, ParseOptions::new())?;
    let file_type = tagged_file.file_type();

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

    if !modify(tag, file_type)? {
        return Ok(false);
    }

    save_tagged_file(&tagged_file, file_path)?;
    Ok(true)
}

/// ゲイン（dB）を、ReplayGainのタグの値にする（`-6.48 dB`）
fn format_gain_db(gain: f64) -> String {
    format!("{:+.2} dB", gain)
}

/// ピークを、ReplayGainのタグの値にする（`0.988769`）
fn format_peak(peak: f64) -> String {
    format!("{:.6}", peak)
}

/// ゲイン（ReplayGainの基準のdB）を、EBU R128のタグの値（Q7.8の整数）にする
fn format_r128_gain(gain: f64) -> String {
    let q78 = ((gain - R128_TO_REPLAY_GAIN_DB) * 256.0).round();
    (q78.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16).to_string()
}

/// タグに、ゲインとピークを書く（値のある項目だけ）
///
/// Opusは、Opusの決まり（RFC 7845）に合わせて、`R128_TRACK_GAIN`・`R128_ALBUM_GAIN`で書く
/// （ピークの項目はない）。読む時に優先する`REPLAYGAIN_*`が残っていると、書いた値が使われないため、
/// 書いた項目に対応する`REPLAYGAIN_*`は取り除く。
/// @returns タグを変えたか（すでに同じ値が書かれていれば、変えない）
fn set_tag_replay_gain(tag: &mut Tag, file_type: FileType, values: &ReplayGain) -> AppResult<bool> {
    /// 項目を書く（同じ値が書かれていれば、何もしない）
    fn insert(tag: &mut Tag, key: ItemKey, value: String) -> AppResult<bool> {
        if tag.get_string(key) == Some(value.as_str()) {
            Ok(false)
        } else if tag.insert_text(key, value) {
            Ok(true)
        } else {
            Err(AppError::Metadata(
                "この形式のタグには、音量の情報を書き込めません".to_string(),
            ))
        }
    }
    let mut changed = false;

    if file_type == FileType::Opus {
        let gains = [
            (
                values.track_gain,
                ItemKey::R128TrackGain,
                [ItemKey::ReplayGainTrackGain, ItemKey::ReplayGainTrackPeak],
            ),
            (
                values.album_gain,
                ItemKey::R128AlbumGain,
                [ItemKey::ReplayGainAlbumGain, ItemKey::ReplayGainAlbumPeak],
            ),
        ];
        for (gain, key, replaced) in gains {
            let Some(gain) = gain else { continue };
            changed |= insert(tag, key, format_r128_gain(gain))?;
            for key in replaced {
                if tag.get(key).is_some() {
                    tag.remove_key(key);
                    changed = true;
                }
            }
        }
        return Ok(changed);
    }

    let items = [
        (
            values.track_gain.map(format_gain_db),
            ItemKey::ReplayGainTrackGain,
        ),
        (
            values.track_peak.map(format_peak),
            ItemKey::ReplayGainTrackPeak,
        ),
        (
            values.album_gain.map(format_gain_db),
            ItemKey::ReplayGainAlbumGain,
        ),
        (
            values.album_peak.map(format_peak),
            ItemKey::ReplayGainAlbumPeak,
        ),
    ];
    for (value, key) in items {
        if let Some(value) = value {
            changed |= insert(tag, key, value)?;
        }
    }
    Ok(changed)
}

/// 音楽ファイルに、解析したゲインとピークを書き込む（値のある項目だけ。ほかのタグは変えない）
///
/// すでに同じ値が書かれているファイルには、書き込まない（ファイルを変えない）。
/// @returns 書き込んだ後に、ファイルのタグから読めるゲインとピーク（データベースに記録する値）と、
///   ファイルへ書き込んだか
pub fn update_file_replay_gain(
    file_path: &Path,
    values: &ReplayGain,
) -> AppResult<(ReplayGain, bool)> {
    let finite = [
        values.track_gain,
        values.track_peak,
        values.album_gain,
        values.album_peak,
    ]
    .into_iter()
    .flatten()
    .all(f64::is_finite);
    if !finite {
        return Err(AppError::Validation(
            "音量の値が正しくありません".to_string(),
        ));
    }

    let written = modify_file_tag_of_type(file_path, |tag, file_type| {
        set_tag_replay_gain(tag, file_type, values)
    })?;
    Ok((read_file_replay_gain(file_path)?, written))
}

/// 音楽ファイルのタグを、別のファイル（形式を変換したファイル）へ写す
///
/// アプリが扱う項目（編集画面の項目・評価・ゲインとピーク・表示するアルバムアート）を、
/// 変換先の形式のタグの書き方で書く。変換先には、まだタグがないものとして扱う。
/// 入力の検証（文字数など）はしない（元のファイルにある値を、そのまま写す）。
pub fn copy_file_tags(source: &Path, destination: &Path) -> AppResult<()> {
    let tagged_file = read_tagged_file(source, ParseOptions::new().read_properties(false))?;
    let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    else {
        return Ok(());
    };
    let metadata = editor_metadata(tag);
    let rating = extract_rating(tag);
    let replay_gain = extract_replay_gain(tag);
    let pictures = tag.pictures();
    let picture = displayed_picture_index(pictures).map(|index| pictures[index].clone());

    modify_file_tag_of_type(destination, |tag, file_type| {
        apply_metadata(tag, &metadata, false);
        set_tag_rating(tag, rating);
        set_tag_replay_gain(tag, file_type, &replay_gain)?;
        if let Some(picture) = picture {
            tag.push_picture(picture);
        }
        Ok(true)
    })
    .map(|_| ())
}

/// 音楽ファイルのタグから、ゲインとピークを読む
pub fn read_file_replay_gain(file_path: &Path) -> AppResult<ReplayGain> {
    let tagged_file = read_tagged_file(file_path, ParseOptions::new().read_cover_art(false))?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    Ok(tag.map(extract_replay_gain).unwrap_or_default())
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

/// タグの、複数の値を持てる項目（アーティスト・ジャンル）を書き換える
///
/// 値は、`;`で区切った文字列で受け取る。タグの今の値と同じなら、タグを変えない（書かれていた
/// 形のまま残す）。違う場合は、タグが「1つの値に`;`で区切って書く」形ならその形で、そうでなければ
/// 値ごとの項目として書く（形式ごとの書き方は、loftyが決める。Vorbisコメントは同じ項目を複数、
/// ID3v2.4は区切り付きの値、MP4は複数の値）。
fn apply_multi_value(tag: &mut Tag, key: ItemKey, value: &Option<String>, clear_missing: bool) {
    let text = match text_change(value, clear_missing) {
        TextChange::Set(text) => text,
        TextChange::Remove => {
            tag.remove_key(key);
            return;
        }
        TextChange::Keep => return,
    };

    let values = multi_value::split(text);
    let current: Vec<String> = tag.get_strings(key).map(str::to_string).collect();
    if values == multi_value::values_of(current.iter().map(String::as_str)) {
        return;
    }

    let was_joined =
        matches!(current.as_slice(), [single] if single.contains(multi_value::SEPARATOR));
    tag.remove_key(key);
    match values.as_slice() {
        // `;`だけの値は、値がないものとして扱う
        [] => {}
        // 複数の値でなければ、渡された文字列をそのまま書く
        [_] if !text.contains(multi_value::SEPARATOR) => {
            tag.insert_text(key, text.to_string());
        }
        _ if values.len() == 1 || was_joined => {
            tag.insert_text(key, multi_value::join(&values));
        }
        _ => {
            for value in values {
                tag.push(TagItem::new(key, ItemValue::Text(value)));
            }
        }
    }
}

/// タグにメタデータを反映する
///
/// 値がある項目はその値にする。`clear_missing`がtrueなら、値がない項目をタグから取り除く
/// （1曲の編集で、欄を空にして保存した場合。編集画面は、すべての項目の値を渡す）。
/// falseなら、値がない項目は変えない（一括編集・データベースの内容の書き出し）。
/// アーティストとジャンルは、`;`で区切ると複数の値になる（`apply_multi_value`）。
/// 文字列の項目（コメント・歌詞を除く）は、空の値を「取り除く」指定として扱う
/// （タグの一括ツールで、置換の結果が空になった場合。`text_change`を参照）。
fn apply_metadata(tag: &mut Tag, metadata: &Metadata, clear_missing: bool) {
    match text_change(&metadata.title, clear_missing) {
        TextChange::Set(title) => tag.set_title(title.to_string()),
        TextChange::Remove => tag.remove_title(),
        TextChange::Keep => {}
    }

    apply_multi_value(tag, ItemKey::TrackArtist, &metadata.artist, clear_missing);

    match text_change(&metadata.album, clear_missing) {
        TextChange::Set(album) => tag.set_album(album.to_string()),
        TextChange::Remove => tag.remove_album(),
        TextChange::Keep => {}
    }

    apply_multi_value(tag, ItemKey::Genre, &metadata.genre, clear_missing);

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

    // BPM: タグの種類によって、整数の項目（ID3v2・MP4）か、もう一方の項目（Vorbisコメント）に書く。
    // APEv2タグは、loftyに対応する項目がないため、検査なしで入れる（保存時に`ape_tag`が書く）
    if metadata.bpm.is_some() || clear_missing {
        tag.remove_key(ItemKey::IntegerBpm);
        tag.remove_key(ItemKey::Bpm);
    }
    if let Some(bpm) = metadata.bpm
        && !tag.insert_text(ItemKey::IntegerBpm, bpm.to_string())
        && !tag.insert_text(ItemKey::Bpm, bpm.to_string())
        && tag.tag_type() == TagType::Ape
    {
        tag.insert_unchecked(TagItem::new(ItemKey::Bpm, ItemValue::Text(bpm.to_string())));
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
    fn test_parse_plain_rating() {
        use PlainRatingScale::{Percent, Stars};

        // 20刻み（半分の星は切り上げる）
        assert_eq!(parse_plain_rating("20"), Some((1, Percent)));
        assert_eq!(parse_plain_rating("80"), Some((4, Percent)));
        assert_eq!(parse_plain_rating("90"), Some((5, Percent)));
        assert_eq!(parse_plain_rating("100"), Some((5, Percent)));
        assert_eq!(parse_plain_rating("10"), Some((1, Percent)));
        // 星の数
        assert_eq!(parse_plain_rating("1"), Some((1, Stars)));
        assert_eq!(parse_plain_rating(" 4 "), Some((4, Stars)));
        assert_eq!(parse_plain_rating("5"), Some((5, Stars)));
        // 0〜1の割合
        assert_eq!(parse_plain_rating("0.8"), Some((4, Percent)));
        assert_eq!(parse_plain_rating("0.1"), Some((1, Percent)));
        // 評価なし・不正な値
        assert_eq!(parse_plain_rating("0"), None);
        assert_eq!(parse_plain_rating("101"), None);
        assert_eq!(parse_plain_rating("-1"), None);
        assert_eq!(parse_plain_rating("MusicBee|4|0"), None);
        assert_eq!(parse_plain_rating(""), None);
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

        // 数値だけの値の解釈は、VorbisコメントとAPEv2タグだけに使う
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
    /// WAV（ID3v2）は生成し、MP3（ID3v2）・M4A（MP4）・FLAC / Ogg Vorbis / Opus / Oggの中のFLAC
    /// （Vorbisコメント）・WavPack / APE（APEv2）は、再生エンジンのテスト用のファイルを写す。
    fn tag_test_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        let mut files = vec![dir.join("generated.wav")];
        write_test_wav(&files[0]);
        for name in [
            "lame.mp3",
            "flac.flac",
            "apple_aac.m4a",
            "vorbis.ogg",
            "opus.opus",
            "ogg_flac.ogg",
            "wavpack.wv",
            "monkey.ape",
        ] {
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
                    artist: Some("アーティスト".to_string()),
                    genre: Some("Jazz".to_string()),
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

    /// アーティスト・ジャンルは、同じ項目が複数あるタグと、`;`で区切った値を、複数の値として読む
    #[test]
    fn test_reads_multiple_artists_and_genres() {
        let mut tag = Tag::new(TagType::VorbisComments);
        for artist in ["Artist A", "Artist B"] {
            tag.push(TagItem::new(
                ItemKey::TrackArtist,
                ItemValue::Text(artist.to_string()),
            ));
        }
        tag.insert_text(ItemKey::Genre, "Rock;Pop".to_string());
        let metadata = library_metadata(&tag);
        assert_eq!(metadata.artist.as_deref(), Some("Artist A; Artist B"));
        assert_eq!(metadata.genre.as_deref(), Some("Rock; Pop"));

        // 複数の値でない項目は、そのまま（`/`・`,`・`&`では分けない）
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::TrackArtist, "AC/DC".to_string());
        tag.insert_text(ItemKey::Genre, "Rock, Pop & Soul".to_string());
        let metadata = library_metadata(&tag);
        assert_eq!(metadata.artist.as_deref(), Some("AC/DC"));
        assert_eq!(metadata.genre.as_deref(), Some("Rock, Pop & Soul"));
        assert_eq!(library_metadata(&Tag::new(TagType::Id3v2)).artist, None);
    }

    /// アーティスト・ジャンルを書き換える時は、タグに書かれていた形（1つの値に`;`で区切る・
    /// 値ごとの項目）を保つ
    #[test]
    fn test_multi_value_fields_keep_the_form_of_the_tag() {
        let strings = |tag: &Tag, key: ItemKey| -> Vec<String> {
            tag.get_strings(key).map(str::to_string).collect()
        };
        let apply = |tag: &mut Tag, value: &str| {
            apply_multi_value(tag, ItemKey::Genre, &Some(value.to_string()), true);
        };

        // 1つの値に`;`で区切って書かれていたタグ
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Genre, "Rock;Pop".to_string());
        // 同じ値なら、タグを変えない（書かれていた文字列のまま）
        apply(&mut tag, "Rock; Pop");
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock;Pop"]);
        // 値を変えると、同じ形（1つの値）で書く
        apply(&mut tag, "Rock; Pop; Jazz");
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock; Pop; Jazz"]);

        // 値ごとの項目だったタグは、値ごとの項目のまま
        let mut tag = Tag::new(TagType::VorbisComments);
        for genre in ["Rock", "Pop"] {
            tag.push(TagItem::new(
                ItemKey::Genre,
                ItemValue::Text(genre.to_string()),
            ));
        }
        apply(&mut tag, "Rock;Pop");
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock", "Pop"]);
        apply(&mut tag, "Rock; Jazz ; Rock");
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock", "Jazz"]);

        // 1つの値だったタグを複数にした時は、値ごとの項目として書く
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::Genre, "Rock".to_string());
        apply(&mut tag, "Rock; Pop");
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock", "Pop"]);
        // 1つの値に戻すと、1つの項目（渡された文字列のまま）
        apply(&mut tag, "AC/DC, Live");
        assert_eq!(strings(&tag, ItemKey::Genre), ["AC/DC, Live"]);

        // 空の値・`;`だけの値は、項目を取り除く。値がなければ、`clear_missing`の時だけ取り除く
        apply(&mut tag, " ; ");
        assert!(strings(&tag, ItemKey::Genre).is_empty());
        tag.insert_text(ItemKey::Genre, "Rock".to_string());
        apply_multi_value(&mut tag, ItemKey::Genre, &None, false);
        assert_eq!(strings(&tag, ItemKey::Genre), ["Rock"]);
        apply_multi_value(&mut tag, ItemKey::Genre, &None, true);
        assert!(strings(&tag, ItemKey::Genre).is_empty());
    }

    /// 複数のアーティスト・ジャンルを書き込み、読み直して同じ値を取得できる（タグの種類ごと）
    #[test]
    fn test_multiple_artists_and_genres_round_trip_in_every_tag_type() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for file in tag_test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let items = |key: ItemKey| -> Vec<String> {
                let tagged = read_tagged_file(&file, ParseOptions::new()).unwrap();
                let tag = tagged.primary_tag().unwrap();
                tag.get_strings(key).map(str::to_string).collect()
            };
            let metadata = Metadata {
                title: Some("タイトル".to_string()),
                artist: Some("宇多田ヒカル; AC/DC; Earth, Wind & Fire".to_string()),
                genre: Some("Rock; J-Pop".to_string()),
                ..Default::default()
            };

            update_file_metadata(&file, &metadata, true).unwrap();
            assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");
            let library = extract_all_file_info(&file).unwrap().metadata;
            assert_eq!(library.artist, metadata.artist, "{name}");
            assert_eq!(library.genre, metadata.genre, "{name}");
            // タグには、値ごとの項目として書かれる
            assert_eq!(
                items(ItemKey::TrackArtist),
                ["宇多田ヒカル", "AC/DC", "Earth, Wind & Fire"],
                "{name}"
            );
            assert_eq!(items(ItemKey::Genre), ["Rock", "J-Pop"], "{name}");

            // ほかの項目を変えても、複数の値はそのまま
            let partial = Metadata {
                album: Some("アルバム".to_string()),
                ..Default::default()
            };
            update_file_metadata(&file, &partial, false).unwrap();
            assert_eq!(items(ItemKey::Genre), ["Rock", "J-Pop"], "{name}");
            // 評価を書いても、そのまま
            update_file_rating(&file, 4).unwrap();
            assert_eq!(
                read_file_tags(&file).unwrap().artist,
                metadata.artist,
                "{name}"
            );

            // 1つの値に戻す
            let single = Metadata {
                artist: Some("ZARD".to_string()),
                genre: Some("J-Pop".to_string()),
                ..Default::default()
            };
            update_file_metadata(&file, &single, false).unwrap();
            assert_eq!(items(ItemKey::TrackArtist), ["ZARD"], "{name}");
            assert_eq!(items(ItemKey::Genre), ["J-Pop"], "{name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 評価を書き込み・書き換え・取り除きでき、ほかのタグは変わらない（タグの種類ごと）
    #[test]
    fn test_rating_round_trips_in_every_tag_type() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for file in tag_test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let metadata = full_metadata();
            update_file_metadata(&file, &metadata, true).unwrap();
            assert_eq!(extract_all_file_info(&file).unwrap().rating, 0, "{name}");

            for rating in [4, 2, 0] {
                update_file_rating(&file, rating).unwrap();
                assert_eq!(
                    extract_all_file_info(&file).unwrap().rating,
                    rating,
                    "{name}"
                );
                assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");
            }

            // ほかの項目と一緒に書いても、同じ値を読む
            let partial = Metadata {
                genre: Some("Rock".to_string()),
                ..Default::default()
            };
            update_file_metadata_and_rating(&file, &partial, Some(5)).unwrap();
            let info = extract_all_file_info(&file).unwrap();
            assert_eq!(info.rating, 5, "{name}");
            assert_eq!(info.metadata.genre.as_deref(), Some("Rock"), "{name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ゲインとピークを書き込み、読み直して同じ値を取得できる（タグの種類ごと）
    #[test]
    fn test_replay_gain_round_trips_in_every_tag_type() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        for file in tag_test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let is_opus = name.ends_with(".opus");
            let metadata = full_metadata();
            update_file_metadata(&file, &metadata, true).unwrap();
            update_file_rating(&file, 4).unwrap();

            // 曲のゲインとピークだけを書く
            let track_only = ReplayGain {
                track_gain: Some(-6.484),
                track_peak: Some(0.9887694),
                ..ReplayGain::default()
            };
            let (written, saved) = update_file_replay_gain(&file, &track_only).unwrap();
            assert!(saved, "{name}");
            // 同じ値をもう一度書いても、ファイルを変えない
            let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
            assert_eq!(
                update_file_replay_gain(&file, &track_only).unwrap(),
                (written.clone(), false),
                "{name}"
            );
            assert_eq!(
                std::fs::metadata(&file).unwrap().modified().unwrap(),
                modified,
                "{name}"
            );
            if is_opus {
                // Opusは、R128のタグ（1/256 dB刻み。ピークはない）で書く
                let gain = written.track_gain.unwrap();
                assert!((gain + 6.484).abs() < 1.0 / 256.0, "{name}: {gain}");
                assert_eq!(written.track_peak, None, "{name}");
            } else {
                // タグに書く桁（ゲインは小数2桁、ピークは小数6桁）に丸めた値を読む
                assert_eq!(written.track_gain, Some(-6.48), "{name}");
                assert_eq!(written.track_peak, Some(0.988769), "{name}");
            }
            assert_eq!(
                (written.album_gain, written.album_peak),
                (None, None),
                "{name}"
            );

            // アルバムのゲインとピークを足す（曲のゲインは変えない）
            let album_only = ReplayGain {
                album_gain: Some(2.5),
                album_peak: Some(1.0),
                ..ReplayGain::default()
            };
            let (both, saved) = update_file_replay_gain(&file, &album_only).unwrap();
            assert!(saved, "{name}");
            assert_eq!(both.track_gain, written.track_gain, "{name}");
            assert_eq!(both.track_peak, written.track_peak, "{name}");
            assert_eq!(both.album_gain, Some(2.5), "{name}");
            assert_eq!(both.album_peak, (!is_opus).then_some(1.0), "{name}");

            // 取り込みで読む値と同じで、ほかの項目・評価は変わらない
            let info = extract_all_file_info(&file).unwrap();
            assert_eq!(info.replay_gain, both, "{name}");
            assert_eq!(info.rating, 4, "{name}");
            assert_eq!(read_file_tags(&file).unwrap(), metadata, "{name}");
            assert_eq!(read_file_replay_gain(&file).unwrap(), both, "{name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_opus_replay_gain_tags_are_replaced_by_r128() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        let file = dir.join("opus.opus");
        std::fs::copy(fixtures.join("opus.opus"), &file).unwrap();
        // ほかのアプリが、Opusの決まりにない`REPLAYGAIN_*`を書いていたファイル
        modify_file_tag(&file, |tag| {
            tag.insert_text(ItemKey::ReplayGainTrackGain, "-1.00 dB".to_string());
            tag.insert_text(ItemKey::ReplayGainTrackPeak, "0.5".to_string());
            tag.insert_text(ItemKey::ReplayGainAlbumGain, "-2.00 dB".to_string());
        })
        .unwrap();
        assert_eq!(read_file_replay_gain(&file).unwrap().track_gain, Some(-1.0));

        // 曲のゲインだけを書く: 曲の`REPLAYGAIN_*`は取り除き、アルバムの分は残す
        let values = ReplayGain {
            track_gain: Some(-8.0),
            track_peak: Some(0.9),
            ..ReplayGain::default()
        };
        let (written, saved) = update_file_replay_gain(&file, &values).unwrap();
        assert!(saved);

        assert_eq!(written.track_gain, Some(-8.0));
        assert_eq!(written.track_peak, None);
        assert_eq!(written.album_gain, Some(-2.0));
        // ピークは書けないが、同じゲインをもう一度書く時は、ファイルを変えない
        assert_eq!(
            update_file_replay_gain(&file, &values).unwrap(),
            (written, false)
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_replay_gain_values_are_formatted_for_tags() {
        assert_eq!(format_gain_db(-6.484), "-6.48 dB");
        assert_eq!(format_gain_db(1.2), "+1.20 dB");
        assert_eq!(format_gain_db(0.0), "+0.00 dB");
        assert_eq!(parse_gain_db(&format_gain_db(1.2)), Some(1.2));
        assert_eq!(format_peak(0.9887694), "0.988769");
        assert_eq!(format_peak(1.0), "1.000000");
        // R128: -23 LUFSが基準（ReplayGainより5 dB低い）のQ7.8
        assert_eq!(format_r128_gain(5.0), "0");
        assert_eq!(format_r128_gain(-1.0), "-1536");
        assert_eq!(parse_r128_gain(&format_r128_gain(-7.25)), Some(-7.25));
        assert_eq!(format_r128_gain(1000.0), "32767");

        // 数でない値は、書き込まない
        let invalid = ReplayGain {
            track_gain: Some(f64::NAN),
            ..ReplayGain::default()
        };
        assert!(matches!(
            update_file_replay_gain(Path::new("/missing.flac"), &invalid),
            Err(AppError::Validation(_))
        ));
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

    /// 実際のファイルへ複数のアーティスト・ジャンルを書き込み、ほかのツール（ffprobeなど）で
    /// 読めるか確かめる（手動で実行する）
    ///
    /// `TAG_TEST_DIR=<音楽ファイルを置いたフォルダ> cargo test write_multiple_values_to_real_files -- --ignored`
    #[test]
    #[ignore = "実際のファイルが必要（TAG_TEST_DIRで指定する。ファイルを書き換える）"]
    fn write_multiple_values_to_real_files() {
        let dir = std::env::var("TAG_TEST_DIR").expect("TAG_TEST_DIRを指定する");
        let metadata = Metadata {
            artist: Some("Artist A; Artist B".to_string()),
            genre: Some("Rock; Pop".to_string()),
            ..Default::default()
        };
        for entry in std::fs::read_dir(dir).unwrap() {
            let file = entry.unwrap().path();
            if !file.is_file() {
                continue;
            }
            let before = read_file_tags(&file).unwrap();
            update_file_metadata(&file, &metadata, false).unwrap();
            let read = read_file_tags(&file).unwrap();
            println!(
                "{}: {:?} / {:?} -> {:?} / {:?}",
                file.display(),
                before.artist,
                before.genre,
                read.artist,
                read.genre
            );
            assert_eq!(read.artist, metadata.artist, "{}", file.display());
            assert_eq!(read.genre, metadata.genre, "{}", file.display());
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

    /// フォルダの中のファイルの名前（並べ替えたもの）
    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// タグは写しに書いてから置き換え、一時ファイルを残さない
    #[test]
    fn test_tags_are_written_to_a_copy_and_then_replace_the_file() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);
        #[cfg(unix)]
        let before = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&file).unwrap());

        update_file_rating(&file, 3).unwrap();

        assert_eq!(extract_all_file_info(&file).unwrap().rating, 3);
        assert_eq!(file_names(&dir), ["a.wav"]);
        // 元のファイルへ直接は書かず、書き込んだ写しで置き換えている
        #[cfg(unix)]
        assert_ne!(
            std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&file).unwrap()),
            before
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 書いている途中で失敗しても、元のファイルはそのまま残り、写しも残らない
    #[test]
    fn test_failed_write_leaves_the_original_file_and_no_copy() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.wav");
        write_test_wav(&file);
        let original = std::fs::read(&file).unwrap();

        let result = write_atomically(&file, |temp| {
            // 書きかけの状態（切り詰めた後）で失敗する
            std::fs::write(temp, b"partial").unwrap();
            Err(AppError::Metadata("書き込みに失敗".to_string()))
        });

        assert!(matches!(result, Err(AppError::Metadata(_))));
        assert_eq!(std::fs::read(&file).unwrap(), original);
        assert_eq!(file_names(&dir), ["a.wav"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// シンボリックリンクで登録した曲は、リンクの先のファイルを書き換え、リンクは残す
    #[cfg(unix)]
    #[test]
    fn test_writing_through_a_symlink_keeps_the_link() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        let real = dir.join("real");
        let links = dir.join("links");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&links).unwrap();
        let file = real.join("a.wav");
        write_test_wav(&file);
        let link = links.join("a.wav");
        std::os::unix::fs::symlink(&file, &link).unwrap();

        update_file_rating(&link, 4).unwrap();

        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(extract_all_file_info(&file).unwrap().rating, 4);
        assert_eq!(file_names(&real), ["a.wav"]);
        assert_eq!(file_names(&links), ["a.wav"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 書き換える時の一時ファイルは、同じフォルダの隠しファイルで、拡張子は元のまま
    #[test]
    fn test_temporary_copy_is_hidden_next_to_the_file() {
        let temp = temp_path_for(Path::new("/music/Album/01 Song.flac"));
        assert_eq!(temp.parent(), Some(Path::new("/music/Album")));
        let name = temp.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(".01 Song.muspice-"), "{name}");
        assert_eq!(temp.extension().and_then(|e| e.to_str()), Some("flac"));
        assert!(crate::library::is_always_skipped(temp.file_name().unwrap()));
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
