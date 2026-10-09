//! 再生中のトラックの記録と、再生状態の保存・復元
//!
//! 再生そのものは、再生エンジンのコマンド（`playback.rs`）で行う。

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::playback_state::{
    PlaybackCursor, PlaybackQueue, PlaybackStateStore, RestoredPlaybackState,
};
use crate::state::AppState;
use tauri::{AppHandle, Manager, State};

/// 再生状態（音量・シャッフル・リピート・再生キュー・再生していた曲）を保存する
///
/// `queue`は、キューが変わった時だけ渡す（nullなら、保存してあるキューから変えない）。
/// 再生位置は保存しない。
#[tauri::command]
#[specta::specta]
pub async fn save_playback_state(
    cursor: PlaybackCursor,
    queue: Option<PlaybackQueue>,
    app: AppHandle,
) -> AppResult<()> {
    // キューが数万曲の場合は、検証とファイルへの書き込みに時間がかかる
    run_blocking(move || app.state::<PlaybackStateStore>().save(cursor, queue)).await
}

/// 保存してある再生状態を取得する（起動時の復元用）
///
/// ライブラリからなくなった曲・ファイルが見つからない曲は、キューから除いて返す。
#[tauri::command]
#[specta::specta]
pub async fn get_playback_state(app: AppHandle) -> AppResult<RestoredPlaybackState> {
    run_blocking(move || {
        let store = app.state::<PlaybackStateStore>();
        app.state::<AppState>().with_db(|db| store.restore(db))
    })
    .await
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
