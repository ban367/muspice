//! 音楽再生関連コマンド

use crate::metadata::{extract_album_art, AlbumArt};
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::path::Path;
use tauri::State;

/// トラックのファイルパスを取得
#[tauri::command]
pub async fn get_track_file_path(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::repository::find_file_path_by_track_id(&db, &track_id)
}

/// 現在再生中のトラックIDを設定
#[tauri::command]
pub async fn set_current_track(
    track_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut current_track = state
        .current_track_id
        .lock()
        .map_err(|e| format!("ステートロックの取得に失敗しました: {}", e))?;

    *current_track = track_id;

    Ok(())
}

/// 現在再生中のトラック情報を取得
#[tauri::command]
pub async fn get_current_track(state: State<'_, AppState>) -> Result<Option<Track>, String> {
    let current_track_id = state
        .current_track_id
        .lock()
        .map_err(|e| format!("現在のトラック情報の取得に失敗しました: {}", e))?;

    let track_id = match current_track_id.as_ref() {
        Some(id) => id.clone(),
        None => return Ok(None),
    };

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    match crate::repository::find_track_by_id(&db, &track_id) {
        Ok(track) => Ok(Some(track)),
        Err(_) => Ok(None),
    }
}

/// トラックのアルバムアートを取得
#[tauri::command]
pub async fn get_album_art(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AlbumArt>, String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // トラックのファイルパスを取得
    let mut stmt = db
        .prepare("SELECT file_path FROM tracks WHERE id = ?1")
        .map_err(|e| format!("クエリの準備に失敗しました: {}", e))?;

    let file_path: String = stmt
        .query_row([&track_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                "指定されたトラックが見つかりません".to_string()
            }
            _ => format!("トラックの取得に失敗しました: {}", e),
        })?;

    // アルバムアートを抽出
    let path = Path::new(&file_path);
    extract_album_art(path)
}
