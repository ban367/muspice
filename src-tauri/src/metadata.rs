use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::models::Metadata;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::Serialize;
use std::path::Path;

/// アルバムアート情報
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumArt {
    /// Base64エンコードされた画像データ
    pub data: String,
    /// MIMEタイプ (image/jpeg, image/png など)
    pub mime_type: String,
}

/// 音楽ファイルからメタデータを抽出
pub fn extract_metadata(file_path: &Path) -> Result<Metadata, String> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let metadata = if let Some(tag) = tag {
        Metadata {
            title: tag.title().map(|s| s.to_string()),
            artist: tag.artist().map(|s| s.to_string()),
            album: tag.album().map(|s| s.to_string()),
            genre: tag.genre().map(|s| s.to_string()),
            year: tag.year().map(|y| y as i32),
            track_number: tag.track().map(|t| t as i32),
            album_artist: tag
                .get_string(&lofty::tag::ItemKey::AlbumArtist)
                .map(|s| s.to_string()),
            composer: tag
                .get_string(&lofty::tag::ItemKey::Composer)
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
            album_artist: None,
            composer: None,
        }
    };

    Ok(metadata)
}

/// 音楽ファイルの再生時間を取得（秒単位）
pub fn extract_duration(file_path: &Path) -> Result<Option<i32>, String> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

    let duration = tagged_file.properties().duration().as_secs() as i32;

    Ok(Some(duration))
}

/// 音楽ファイルのビットレートを取得（kbps）
pub fn extract_bitrate(file_path: &Path) -> Result<Option<i32>, String> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

    let bitrate = tagged_file.properties().audio_bitrate().map(|b| b as i32);

    Ok(bitrate)
}

/// 音楽ファイルのサンプルレートを取得（Hz）
pub fn extract_sample_rate(file_path: &Path) -> Result<Option<i32>, String> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

    let sample_rate = tagged_file.properties().sample_rate().map(|s| s as i32);

    Ok(sample_rate)
}

/// 音楽ファイルからアルバムアートを抽出
pub fn extract_album_art(file_path: &Path) -> Result<Option<AlbumArt>, String> {
    let tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

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
            let mime_type = pic.mime_type().map(|m| m.to_string()).unwrap_or_else(|| "image/jpeg".to_string());

            return Ok(Some(AlbumArt { data, mime_type }));
        }
    }

    Ok(None)
}

/// メタデータをバリデーション
pub fn validate_metadata(metadata: &Metadata) -> Result<(), String> {
    // 年のバリデーション
    if let Some(year) = metadata.year {
        if year < 1000 || year > 9999 {
            return Err("年は1000から9999の範囲で指定してください".to_string());
        }
    }

    // トラック番号のバリデーション
    if let Some(track_number) = metadata.track_number {
        if track_number < 1 || track_number > 999 {
            return Err("トラック番号は1から999の範囲で指定してください".to_string());
        }
    }

    Ok(())
}

/// 音楽ファイルのメタデータを更新
pub fn update_file_metadata(file_path: &Path, metadata: &Metadata) -> Result<(), String> {
    // メタデータをバリデーション
    validate_metadata(metadata)?;

    // ファイルを読み込み
    let mut tagged_file = Probe::open(file_path)
        .map_err(|e| format!("ファイルのオープンに失敗しました: {}", e))?
        .read()
        .map_err(|e| format!("ファイルの読み取りに失敗しました: {}", e))?;

    // プライマリタグを取得または作成
    let tag = match tagged_file.primary_tag_mut() {
        Some(tag) => tag,
        None => {
            // タグが存在しない場合は新規作成
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(Tag::new(tag_type));
            tagged_file
                .primary_tag_mut()
                .ok_or_else(|| "タグの作成に失敗しました".to_string())?
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
        tag.set_year(year as u32);
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
        .map_err(|e| format!("メタデータの保存に失敗しました: {}", e))?;

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
            album_artist: None,
            composer: None,
        };

        assert!(validate_metadata(&metadata).is_err());
    }
}
