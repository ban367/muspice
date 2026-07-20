use crate::error::{AppError, AppResult};
use crate::models::Metadata;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::Serialize;
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

/// アルバムアート情報
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumArt {
    /// Base64エンコードされた画像データ
    pub data: String,
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
}

/// 音楽ファイルから全情報を一括抽出する
///
/// `extract_metadata`, `extract_duration`, `extract_bitrate`, `extract_sample_rate`を
/// 個別に呼ぶ代わりに、1回のファイルオープンで全て取得する。
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
    })
}

/// 音楽ファイルからメタデータを抽出
pub fn extract_metadata(file_path: &Path) -> AppResult<Metadata> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| AppError::Metadata(format!("ファイルのオープンに失敗しました: {}", e)))?
        .read()
        .map_err(|e| AppError::Metadata(format!("ファイルの読み取りに失敗しました: {}", e)))?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let metadata = if let Some(tag) = tag {
        // ディスク番号を取得（ItemKey::DiscNumber を優先、tag.disk() をフォールバック）
        let disc_number = tag
            .get_string(lofty::tag::ItemKey::DiscNumber)
            .and_then(|s| {
                // "2/2" のような形式から先頭の数字を取得
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
            album_artist: tag
                .get_string(lofty::tag::ItemKey::AlbumArtist)
                .map(|s| s.to_string()),
            composer: tag
                .get_string(lofty::tag::ItemKey::Composer)
                .map(|s| s.to_string()),
        }
    } else {
        // タグが存在しない場合は空のメタデータを返す
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

    Ok(metadata)
}

/// 音楽ファイルからアルバムアートを抽出
pub fn extract_album_art(file_path: &Path) -> AppResult<Option<AlbumArt>> {
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
            let data = STANDARD.encode(pic.data());
            let mime_type = pic
                .mime_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());

            return Ok(Some(AlbumArt { data, mime_type }));
        }
    }

    Ok(None)
}

/// メタデータをバリデーション
pub fn validate_metadata(metadata: &Metadata) -> AppResult<()> {
    // 年のバリデーション
    if let Some(year) = metadata.year {
        if !YEAR_RANGE.contains(&year) {
            return Err(AppError::Validation(
                "年は1000から9999の範囲で指定してください".to_string(),
            ));
        }
    }

    // トラック番号のバリデーション
    if let Some(track_number) = metadata.track_number {
        if !(1..=999).contains(&track_number) {
            return Err(AppError::Validation(
                "トラック番号は1から999の範囲で指定してください".to_string(),
            ));
        }
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
    fn test_extract_metadata_nonexistent_file() {
        let result = extract_metadata(Path::new("nonexistent.mp3"));
        assert!(result.is_err());
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
