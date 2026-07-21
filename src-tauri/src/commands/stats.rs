//! お気に入り・レーティング・再生統計コマンド

use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use tauri::State;

/// お気に入りを切り替え
#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite(track_id: String, state: State<'_, AppState>) -> AppResult<bool> {
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::toggle_track_favorite(db, &track_id))
}

/// レーティングを設定
#[tauri::command]
#[specta::specta]
pub async fn set_rating(
    track_id: String,
    rating: i32,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_track_id(&track_id)?;

    if !(0..=5).contains(&rating) {
        return Err(AppError::Validation(
            "レーティングは0から5の間で指定してください".to_string(),
        ));
    }

    state.with_db(|db| crate::repository::set_track_rating(db, &track_id, rating))
}

/// 再生回数をインクリメント
#[tauri::command]
#[specta::specta]
pub async fn increment_play_count(track_id: String, state: State<'_, AppState>) -> AppResult<i32> {
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::increment_track_play_count(db, &track_id))
}

/// お気に入りトラック一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_favorite_tracks(state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_favorite_tracks(db))
}

/// 最も再生されたトラック一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_most_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    let limit = limit.unwrap_or(50);
    state.with_db(|db| crate::repository::find_most_played_tracks(db, limit))
}

/// 最近再生されたトラック一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_recently_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    let limit = limit.unwrap_or(50);
    state.with_db(|db| crate::repository::find_recently_played_tracks(db, limit))
}
