//! プレイリスト管理コマンド

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::validation::{validate_playlist_id, validate_playlist_name, validate_track_id};
use tauri::State;

/// プレイリストを作成
#[tauri::command]
pub async fn create_playlist(
    name: String,
    state: State<'_, AppState>,
) -> AppResult<crate::models::Playlist> {
    // プレイリスト名をバリデーション
    validate_playlist_name(&name)?;

    state.with_db(|db| {
        crate::playlist::create_playlist(db, &name)
            .map_err(|e| AppError::Database(format!("プレイリストの作成に失敗しました: {}", e)))
    })
}

/// すべてのプレイリストを取得
#[tauri::command]
pub async fn get_playlists(state: State<'_, AppState>) -> AppResult<Vec<crate::models::Playlist>> {
    state.with_db(|db| {
        crate::playlist::get_all_playlists(db)
            .map_err(|e| AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)))
    })
}

/// プレイリストにトラックを追加
#[tauri::command]
pub async fn add_track_to_playlist(
    playlist_id: String,
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    validate_track_id(&track_id)?;

    state.with_db(|db| {
        crate::playlist::add_track_to_playlist(db, &playlist_id, &track_id).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストまたはトラックが見つかりません".to_string())
            }
            _ => AppError::Database(format!("トラックの追加に失敗しました: {}", e)),
        })
    })
}

/// プレイリストからトラックを削除
#[tauri::command]
pub async fn remove_track_from_playlist(
    playlist_id: String,
    track_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    validate_track_id(&track_id)?;

    state.with_db(|db| {
        crate::playlist::remove_track_from_playlist(db, &playlist_id, &track_id).map_err(
            |e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound("プレイリストまたはトラックが見つかりません".to_string())
                }
                _ => AppError::Database(format!("トラックの削除に失敗しました: {}", e)),
            },
        )
    })
}

/// プレイリストの名前を変更
#[tauri::command]
pub async fn rename_playlist(
    playlist_id: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 名前をバリデーション
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;

    state.with_db(|db| {
        crate::playlist::rename_playlist(db, &playlist_id, &name).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストが見つかりません".to_string())
            }
            _ => AppError::Database(format!("プレイリスト名の変更に失敗しました: {}", e)),
        })
    })
}

/// プレイリストを削除
#[tauri::command]
pub async fn delete_playlist(playlist_id: String, state: State<'_, AppState>) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    state.with_db(|db| {
        crate::playlist::delete_playlist(db, &playlist_id).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストが見つかりません".to_string())
            }
            _ => AppError::Database(format!("プレイリストの削除に失敗しました: {}", e)),
        })
    })
}

/// プレイリスト内のトラックを並び替え
#[tauri::command]
pub async fn reorder_playlist_tracks(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    state.with_db(|db| {
        crate::playlist::reorder_playlist_tracks(db, &playlist_id, &track_ids).map_err(
            |e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound("プレイリストが見つかりません".to_string())
                }
                _ => AppError::Database(format!("トラックの並び替えに失敗しました: {}", e)),
            },
        )
    })
}
