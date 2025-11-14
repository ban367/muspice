use crate::models::Track;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// サポートされている音楽ファイル形式
const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a"];

/// フォルダから音楽ファイルをインポートする結果
#[derive(Debug, serde::Serialize)]
pub struct ImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub error_count: usize,
    pub errors: Vec<String>,
}

/// 重複ファイルの処理方法
#[derive(Debug, serde::Deserialize, Clone, Copy)]
pub enum DuplicateAction {
    Skip,    // 既存ファイルをスキップ
    Replace, // 既存ファイルを置き換え
}

/// ディレクトリを再帰的にスキャンして音楽ファイルを検索
pub fn scan_directory(dir_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut audio_files = Vec::new();

    scan_directory_recursive(dir_path, &mut audio_files)?;

    Ok(audio_files)
}

/// 再帰的にディレクトリをスキャンする内部関数
fn scan_directory_recursive(dir_path: &Path, audio_files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir_path)
        .map_err(|e| format!("ディレクトリの読み取りに失敗しました: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("エントリの読み取りに失敗しました: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            // サブディレクトリを再帰的にスキャン
            scan_directory_recursive(&path, audio_files)?;
        } else if path.is_file() {
            // サポートされている拡張子かチェック
            if is_supported_audio_file(&path) {
                audio_files.push(path);
            }
        }
    }

    Ok(())
}

/// ファイルがサポートされている音楽ファイルかチェック
pub fn is_supported_audio_file(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        if let Some(ext_str) = extension.to_str() {
            return SUPPORTED_EXTENSIONS.contains(&ext_str.to_lowercase().as_str());
        }
    }
    false
}

/// ファイルパスが既にデータベースに存在するかチェック
pub fn is_duplicate_file(conn: &Connection, file_path: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM tracks WHERE file_path = ?1")?;
    let count: i64 = stmt.query_row([file_path], |row| row.get(0))?;
    Ok(count > 0)
}

/// トラックをデータベースに保存
pub fn save_track(conn: &Connection, track: &Track) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO tracks (
            id, file_path, file_name, title, artist, album, genre, year,
            duration, file_size, format, bitrate, sample_rate, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            track.id,
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.created_at,
            track.updated_at,
        ],
    )?;
    Ok(())
}

/// 既存のトラックを更新（重複時の置き換え用）
pub fn update_track(conn: &Connection, track: &Track) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks SET
            file_name = ?2, title = ?3, artist = ?4, album = ?5, genre = ?6, year = ?7,
            duration = ?8, file_size = ?9, format = ?10, bitrate = ?11, sample_rate = ?12,
            updated_at = ?13
        WHERE file_path = ?1",
        rusqlite::params![
            track.file_path,
            track.file_name,
            track.title,
            track.artist,
            track.album,
            track.genre,
            track.year,
            track.duration,
            track.file_size,
            track.format,
            track.bitrate,
            track.sample_rate,
            track.updated_at,
        ],
    )?;
    Ok(())
}

/// ファイル名からデフォルトのタイトルを生成
pub fn get_default_title(file_path: &Path) -> String {
    file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// ファイルサイズを取得
pub fn get_file_size(file_path: &Path) -> Result<i64, String> {
    let metadata = fs::metadata(file_path)
        .map_err(|e| format!("ファイルメタデータの取得に失敗しました: {}", e))?;
    Ok(metadata.len() as i64)
}

/// ファイル形式を取得
pub fn get_file_format(file_path: &Path) -> String {
    file_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_audio_file() {
        assert!(is_supported_audio_file(Path::new("test.mp3")));
        assert!(is_supported_audio_file(Path::new("test.flac")));
        assert!(is_supported_audio_file(Path::new("test.wav")));
        assert!(is_supported_audio_file(Path::new("test.m4a")));
        assert!(is_supported_audio_file(Path::new("test.MP3")));
        assert!(!is_supported_audio_file(Path::new("test.txt")));
        assert!(!is_supported_audio_file(Path::new("test.jpg")));
    }

    #[test]
    fn test_get_default_title() {
        assert_eq!(get_default_title(Path::new("song.mp3")), "song");
        assert_eq!(
            get_default_title(Path::new("/path/to/my_song.flac")),
            "my_song"
        );
    }

    #[test]
    fn test_get_file_format() {
        assert_eq!(get_file_format(Path::new("test.mp3")), "MP3");
        assert_eq!(get_file_format(Path::new("test.flac")), "FLAC");
    }
}
