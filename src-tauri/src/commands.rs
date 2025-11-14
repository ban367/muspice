use crate::library::{
    get_default_title, get_file_format, get_file_size, is_duplicate_file, save_track,
    scan_directory, update_track, DuplicateAction, ImportResult,
};
use crate::metadata::{extract_bitrate, extract_duration, extract_metadata, extract_sample_rate};
use crate::models::Track;
use crate::state::AppState;
use chrono::Utc;
use std::path::Path;
use tauri::State;
use uuid::Uuid;

/// フォルダから音楽ファイルをインポート
#[tauri::command]
pub async fn import_folder(
    folder_path: String,
    duplicate_action: DuplicateAction,
    state: State<'_, AppState>,
) -> Result<ImportResult, String> {
    let path = Path::new(&folder_path);

    // ディレクトリをスキャン
    let audio_files = scan_directory(path)?;

    let mut imported_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    for file_path in audio_files {
        let file_path_str = file_path
            .to_str()
            .ok_or_else(|| "ファイルパスの変換に失敗しました".to_string())?;

        // 重複チェック
        let is_duplicate = match is_duplicate_file(&db, file_path_str) {
            Ok(dup) => dup,
            Err(e) => {
                errors.push(format!("{}: {}", file_path_str, e));
                error_count += 1;
                continue;
            }
        };

        if is_duplicate {
            match duplicate_action {
                DuplicateAction::Skip => {
                    skipped_count += 1;
                    continue;
                }
                DuplicateAction::Replace => {
                    // 既存のトラックを更新
                    match process_and_update_track(&db, &file_path) {
                        Ok(_) => imported_count += 1,
                        Err(e) => {
                            errors.push(format!("{}: {}", file_path_str, e));
                            error_count += 1;
                        }
                    }
                }
            }
        } else {
            // 新しいトラックを追加
            match process_and_save_track(&db, &file_path) {
                Ok(_) => imported_count += 1,
                Err(e) => {
                    errors.push(format!("{}: {}", file_path_str, e));
                    error_count += 1;
                }
            }
        }
    }

    Ok(ImportResult {
        imported_count,
        skipped_count,
        error_count,
        errors,
    })
}

/// トラックを処理してデータベースに保存
fn process_and_save_track(conn: &rusqlite::Connection, file_path: &Path) -> Result<(), String> {
    let track = create_track_from_file(file_path)?;
    save_track(conn, &track).map_err(|e| format!("トラックの保存に失敗しました: {}", e))?;
    Ok(())
}

/// トラックを処理してデータベースを更新
fn process_and_update_track(conn: &rusqlite::Connection, file_path: &Path) -> Result<(), String> {
    let track = create_track_from_file(file_path)?;
    update_track(conn, &track).map_err(|e| format!("トラックの更新に失敗しました: {}", e))?;
    Ok(())
}

/// ファイルからトラック情報を作成
fn create_track_from_file(file_path: &Path) -> Result<Track, String> {
    let file_name = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "ファイル名の取得に失敗しました".to_string())?
        .to_string();

    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| "ファイルパスの変換に失敗しました".to_string())?
        .to_string();

    // メタデータを抽出
    let metadata = extract_metadata(file_path)?;

    // タイトルがない場合はファイル名をデフォルトとして使用
    let title = metadata
        .title
        .or_else(|| Some(get_default_title(file_path)));

    // その他の情報を取得
    let duration = extract_duration(file_path)?;
    let bitrate = extract_bitrate(file_path)?;
    let sample_rate = extract_sample_rate(file_path)?;
    let file_size = get_file_size(file_path)?;
    let format = get_file_format(file_path);

    let now = Utc::now().to_rfc3339();

    Ok(Track {
        id: Uuid::new_v4().to_string(),
        file_path: file_path_str,
        file_name,
        title,
        artist: metadata.artist,
        album: metadata.album,
        genre: metadata.genre,
        year: metadata.year,
        duration,
        file_size,
        format,
        bitrate,
        sample_rate,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// すべてのトラックを取得
#[tauri::command]
pub async fn get_all_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    duration, file_size, format, bitrate, sample_rate, created_at, updated_at
             FROM tracks
             ORDER BY created_at DESC",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                duration: row.get(8)?,
                file_size: row.get(9)?,
                format: row.get(10)?,
                bitrate: row.get(11)?,
                sample_rate: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}

/// トラックを検索
#[tauri::command]
pub async fn search_tracks(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    let search_pattern = format!("%{}%", query);

    let mut stmt = db
        .prepare(
            "SELECT id, file_path, file_name, title, artist, album, genre, year,
                    duration, file_size, format, bitrate, sample_rate, created_at, updated_at
             FROM tracks
             WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 OR genre LIKE ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let tracks = stmt
        .query_map([&search_pattern], |row| {
            Ok(Track {
                id: row.get(0)?,
                file_path: row.get(1)?,
                file_name: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                album: row.get(5)?,
                genre: row.get(6)?,
                year: row.get(7)?,
                duration: row.get(8)?,
                file_size: row.get(9)?,
                format: row.get(10)?,
                bitrate: row.get(11)?,
                sample_rate: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|e| format!("クエリの実行に失敗しました: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("結果の取得に失敗しました: {}", e))?;

    Ok(tracks)
}
