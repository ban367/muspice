use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// サポートされている音楽ファイル形式
const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a"];

/// フォルダから音楽ファイルをインポートする結果
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_count: u32,
    pub skipped_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
}

/// 重複ファイルの処理方法
#[derive(Debug, serde::Deserialize, Clone, Copy, specta::Type)]
pub enum DuplicateAction {
    Skip,    // 既存ファイルをスキップ
    Replace, // 既存ファイルを置き換え
}

/// トラック削除の結果
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// 削除に成功したトラック数
    pub success_count: u32,
    /// 削除に失敗したトラック数
    pub failed_count: u32,
    /// 削除に失敗したトラックの詳細
    pub failed_tracks: Vec<DeleteFailure>,
}

/// 削除失敗の詳細
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailure {
    /// トラックID
    pub track_id: String,
    /// ファイルパス
    pub file_path: String,
    /// 失敗の理由
    pub reason: String,
}

/// ディレクトリを再帰的にスキャンして音楽ファイルを検索
pub fn scan_directory(dir_path: &Path) -> AppResult<Vec<PathBuf>> {
    let mut audio_files = Vec::new();

    scan_directory_recursive(dir_path, &mut audio_files)?;

    Ok(audio_files)
}

/// 再帰的にディレクトリをスキャンする内部関数
fn scan_directory_recursive(dir_path: &Path, audio_files: &mut Vec<PathBuf>) -> AppResult<()> {
    let entries = fs::read_dir(dir_path)
        .map_err(|e| AppError::Io(format!("ディレクトリの読み取りに失敗しました: {}", e)))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| AppError::Io(format!("エントリの読み取りに失敗しました: {}", e)))?;
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

/// ファイル名からデフォルトのタイトルを生成
pub fn get_default_title(file_path: &Path) -> String {
    file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// ファイルサイズを取得
pub fn get_file_size(file_path: &Path) -> AppResult<i64> {
    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Io(format!("ファイルメタデータの取得に失敗しました: {}", e)))?;
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

/// 件数（usize）をフロントエンドへ返すu32へ変換する
///
/// u32に収まらない件数は現実的に発生しないが、`as`によるサイレントな
/// 切り捨てを避けるため明示的に変換し、収まらない場合はエラーとする。
fn to_count(value: usize) -> AppResult<u32> {
    u32::try_from(value)
        .map_err(|_| AppError::Database(format!("件数が扱える範囲を超えました: {}", value)))
}

/// トラックをデータベースから削除（ライブラリからのみ削除）
///
/// トラックIDのバリデーションはコマンド層（入力境界）で実施済みであることを前提とする。
pub fn delete_tracks(conn: &Connection, track_ids: &[String]) -> AppResult<u32> {
    if track_ids.is_empty() {
        return Ok(0);
    }

    // トランザクションを使用して一括削除
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    let mut deleted_count: usize = 0;

    for track_id in track_ids {
        deleted_count += crate::repository::delete_track(&tx, track_id)?;
    }

    tx.commit().map_err(|e| {
        AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
    })?;

    to_count(deleted_count)
}

/// トラックをデータベースとファイルシステムから削除
/// ファイル削除に失敗した場合でも、可能な限り処理を続行する
pub fn delete_tracks_with_files(
    conn: &Connection,
    track_ids: &[String],
) -> AppResult<DeleteResult> {
    if track_ids.is_empty() {
        return Ok(DeleteResult {
            success_count: 0,
            failed_count: 0,
            failed_tracks: Vec::new(),
        });
    }

    // まずファイルパスを取得（見つからないトラックはスキップ）
    let mut track_info: Vec<(String, String)> = Vec::new();
    for track_id in track_ids {
        if let Some(path) = crate::repository::try_find_file_path_by_track_id(conn, track_id)? {
            track_info.push((track_id.clone(), path));
        }
    }

    let mut success_count: usize = 0;
    let mut failed_tracks: Vec<DeleteFailure> = Vec::new();

    // トランザクションを使用してデータベースから削除
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    for (track_id, file_path) in &track_info {
        // ファイルの削除を試みる
        let file_delete_result = fs::remove_file(file_path);

        match file_delete_result {
            Ok(()) => {
                // ファイル削除成功、データベースからも削除
                crate::repository::delete_track(&tx, track_id)?;
                success_count += 1;
            }
            Err(e) => {
                // ファイル削除失敗
                // ファイルが見つからない場合はDBからも削除を許可
                if e.kind() == std::io::ErrorKind::NotFound {
                    crate::repository::delete_track(&tx, track_id)?;
                    success_count += 1;
                } else {
                    failed_tracks.push(DeleteFailure {
                        track_id: track_id.clone(),
                        file_path: file_path.clone(),
                        reason: format!("ファイルの削除に失敗しました: {}", e),
                    });
                }
            }
        }
    }

    tx.commit().map_err(|e| {
        AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
    })?;

    Ok(DeleteResult {
        success_count: to_count(success_count)?,
        failed_count: to_count(failed_tracks.len())?,
        failed_tracks,
    })
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

    #[test]
    fn test_to_count() {
        assert_eq!(to_count(0).unwrap(), 0);
        assert_eq!(to_count(42).unwrap(), 42);
        assert_eq!(to_count(u32::MAX as usize).unwrap(), u32::MAX);
    }

    /// u32に収まらない件数は切り捨てずエラーにする（64bit環境でのみ検証可能）
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_to_count_overflow() {
        assert!(to_count(u32::MAX as usize + 1).is_err());
    }
}
