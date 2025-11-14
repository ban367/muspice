use crate::models::Metadata;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metadata_nonexistent_file() {
        let result = extract_metadata(Path::new("nonexistent.mp3"));
        assert!(result.is_err());
    }
}
