//! 音楽再生関連コマンド

use crate::error::{AppError, AppResult};
use crate::metadata::{extract_album_art, AlbumArt};
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::path::Path;
use tauri::State;

/// トラックのファイルパスを取得
#[tauri::command]
#[specta::specta]
pub async fn get_track_file_path(
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))
}

/// 現在再生中のトラックIDを設定
#[tauri::command]
#[specta::specta]
pub async fn set_current_track(
    track_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let mut current_track = state
        .current_track_id
        .lock()
        .map_err(|e| AppError::Lock(format!("ステートロックの取得に失敗しました: {}", e)))?;

    *current_track = track_id;

    Ok(())
}

/// 現在再生中のトラック情報を取得
#[tauri::command]
#[specta::specta]
pub async fn get_current_track(state: State<'_, AppState>) -> AppResult<Option<Track>> {
    let track_id = {
        let current_track_id = state.current_track_id.lock().map_err(|e| {
            AppError::Lock(format!("現在のトラック情報の取得に失敗しました: {}", e))
        })?;

        match current_track_id.as_ref() {
            Some(id) => id.clone(),
            None => return Ok(None),
        }
    };

    state.with_db(
        |db| match crate::repository::find_track_by_id(db, &track_id) {
            Ok(track) => Ok(Some(track)),
            Err(_) => Ok(None),
        },
    )
}

/// トラックのアルバムアートを取得
#[tauri::command]
#[specta::specta]
pub async fn get_album_art(
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<Option<AlbumArt>> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    // トラックのファイルパスを取得
    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;

    // アルバムアートを抽出（DBロック外で実行）
    extract_album_art(Path::new(&file_path))
}
