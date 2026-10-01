//! システム関連コマンド

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::path::Path;
use tauri::State;

/// トラックのファイルをシステムのファイルマネージャーで表示
///
/// WebViewから任意のパスを指定できないよう、トラックIDからDB上のパスを解決する。
#[tauri::command]
#[specta::specta]
pub async fn show_in_folder(track_id: String, state: State<'_, AppState>) -> AppResult<()> {
    // トラックIDをバリデーション
    validate_track_id(&track_id)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;
    let file_path = Path::new(&file_path);

    if !file_path.exists() {
        return Err(AppError::NotFound("ファイルが見つかりません".to_string()));
    }

    // OSごとのファイルマネージャー起動とファイル選択はopenerに任せる
    // （LinuxはFileManager1非対応の環境でもXDGポータル経由でフォルダを開く）
    tauri_plugin_opener::reveal_item_in_dir(file_path)
        .map_err(|e| AppError::Io(format!("ファイルマネージャーを開けませんでした: {}", e)))
}
