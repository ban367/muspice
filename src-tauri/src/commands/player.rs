//! 音楽再生関連コマンド

use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use tauri::{AppHandle, Manager, State};

/// トラックのファイルパスを取得
///
/// asset protocolの静的スコープは空にしてあるため、ここで返すファイルだけを
/// その都度スコープへ追加する。これによりWebViewから`convertFileSrc`で読めるのは
/// ライブラリに登録済みのトラックに限られる。
#[tauri::command]
#[specta::specta]
pub async fn get_track_file_path(
    track_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> AppResult<String> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;

    app.asset_protocol_scope()
        .allow_file(&file_path)
        .map_err(|e| AppError::Io(format!("ファイルへのアクセス許可に失敗しました: {}", e)))?;

    Ok(file_path)
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
