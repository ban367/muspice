//! 再生状態の保存・復元
//!
//! 再生そのものは、再生エンジンのコマンド（`playback.rs`）で行う。

use super::run_blocking;
use crate::error::AppResult;
use crate::playback_state::{
    PlaybackCursor, PlaybackQueue, PlaybackStateStore, RestoredPlaybackState,
};
use crate::state::AppState;
use tauri::{AppHandle, Manager};

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
