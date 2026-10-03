//! アプリケーション設定のコマンド

use super::run_blocking;
use crate::error::AppResult;
use crate::events::SettingsChanged;
use crate::library_sync::LibrarySync;
use crate::settings::{Settings, SettingsState};
use tauri::{AppHandle, Manager, State};
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
    let previous_language = state.get()?.language;
    state.save(settings.clone())?;
    // 言語が変わったら、メニューバーと設定ウィンドウのタイトルを作り直す
    if settings.language != previous_language {
        crate::menu::apply_language(&app, settings.language);
    }
    if let Err(e) = SettingsChanged(settings.clone()).emit(&app) {
        log::warn!("設定変更の通知に失敗しました: {}", e);
    }

    // ライブラリフォルダの自動反映（監視の開始はフォルダの数によって時間がかかるため、別スレッドで）
    run_blocking(move || {
        app.state::<LibrarySync>().apply_settings(&app, &settings);
        Ok(())
    })
    .await
}
