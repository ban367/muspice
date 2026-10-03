use crate::error::{AppError, AppResult};
use crate::models::{Metadata, ReplayGain};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
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
}

/// 音楽ファイルから全情報（タグ・長さ・ビットレート・サンプルレート・ReplayGain）を
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
            album_artist: tag.get_string(ItemKey::AlbumArtist).map(|s| s.to_string()),
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

/// 音楽ファイルのメタデータを更新
pub fn update_file_metadata(file_path: &Path, metadata: &Metadata) -> AppResult<()> {
    // メタデータをバリデーション
    validate_metadata(metadata)?;

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

    // メタデータを更新
    if let Some(title) = &metadata.title {
        tag.set_title(title.clone());
    }

    if let Some(artist) = &metadata.artist {
        tag.set_artist(artist.clone());
    }

    if let Some(album) = &metadata.album {
        tag.set_album(album.clone());
    }

    if let Some(genre) = &metadata.genre {
        tag.set_genre(genre.clone());
    }

    if let Some(year) = metadata.year {
        // lofty 0.24 で set_year が廃止されたため date/Timestamp を使う。
        // 既存の月日などを消さないよう、現在の Timestamp の年だけを差し替える。
        let mut timestamp = tag.date().unwrap_or_default();
        timestamp.year = year as u16;
        tag.set_date(timestamp);
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

    // ファイルに保存
    tagged_file
        .save_to_path(file_path, WriteOptions::default())
        .map_err(|e| AppError::Metadata(format!("メタデータの保存に失敗しました: {}", e)))?;

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
