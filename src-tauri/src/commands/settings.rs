//! アプリケーション設定のコマンド

use crate::error::AppResult;
use crate::events::SettingsChanged;
use crate::settings::{Settings, SettingsState};
use tauri::{AppHandle, State};
use tauri_specta::Event;

/// 現在の設定を取得
#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, SettingsState>) -> AppResult<Settings> {
    state.get()
}

/// 設定を保存
///
/// 保存後に`SettingsChanged`イベントを送り、設定ウィンドウ以外（メインウィンドウ）にも反映させる。
#[tauri::command]
#[specta::specta]
pub async fn save_settings(
    settings: Settings,
    state: State<'_, SettingsState>,
    app: AppHandle,
) -> AppResult<()> {
    state.save(settings.clone())?;
    if let Err(e) = SettingsChanged(settings).emit(&app) {
        log::warn!("設定変更の通知に失敗しました: {}", e);
    }
    Ok(())
}
