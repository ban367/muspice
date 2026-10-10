//! アプリケーション設定のコマンド

use crate::error::AppResult;
use crate::events::SettingsChanged;
use crate::library_sync::LibrarySync;
use crate::playback::{PlaybackEngine, PlaybackOptions};
use crate::settings::{Settings, SettingsState};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 現在の設定を取得
#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, SettingsState>) -> AppResult<Settings> {
    state.get()
}

/// グローバルホットキーの割り当てと、OSに登録できているかを取得（設定画面に出す）
///
/// 設定がオフの間は、どれも登録されていない。オンでも、ほかのアプリが使っている組み合わせは
/// 登録できないことがある。
#[tauri::command]
#[specta::specta]
pub async fn get_global_shortcuts(
    app: AppHandle,
) -> AppResult<Vec<crate::global_shortcuts::GlobalShortcutInfo>> {
    Ok(crate::global_shortcuts::status(&app))
}

/// ミニプレーヤー（メインウィンドウの小さな表示）と、通常の表示を切り替える
///
/// ウィンドウの大きさを変えるだけで、画面の切り替えはフロントエンドが行う。ミニプレーヤーの間に
/// もう一度`enabled: true`で呼ぶと、「常に手前に表示」だけを変える。
#[tauri::command]
#[specta::specta]
pub async fn set_mini_player(
    enabled: bool,
    always_on_top: bool,
    app: AppHandle,
    mini_player: State<'_, crate::mini_player::MiniPlayerState>,
) -> AppResult<()> {
    let window = app
        .get_webview_window(crate::tray::MAIN_WINDOW)
        .ok_or_else(|| {
            crate::error::AppError::NotFound("メインウィンドウが見つかりません".to_string())
        })?;
    if enabled {
        crate::mini_player::enter(&window, &mini_player, always_on_top)
    } else {
        crate::mini_player::exit(&window, &mini_player)
    }
}

/// 設定を保存
///
/// 保存後に`SettingsChanged`イベントを送り、設定ウィンドウ以外（メインウィンドウ）にも反映させる。
#[tauri::command]
#[specta::specta]
pub async fn save_settings(
    settings: Settings,
    state: State<'_, SettingsState>,
    engine: State<'_, PlaybackEngine>,
    app: AppHandle,
) -> AppResult<()> {
    let previous = state.get()?;
    state.save(settings.clone())?;
    // 言語が変わったら、メニューバーと設定ウィンドウのタイトルを作り直す
    if settings.language != previous.language {
        crate::menu::apply_language(&app, settings.language);
    }
    // メニューバーへの常駐（言語が変わった場合も、メニューの文言を作り直す）と、グローバルホットキー
    if settings.stay_in_menu_bar != previous.stay_in_menu_bar
        || settings.language != previous.language
    {
        crate::tray::apply(&app, settings.stay_in_menu_bar, settings.language);
    }
    if settings.global_shortcuts != previous.global_shortcuts {
        crate::global_shortcuts::apply(&app, settings.global_shortcuts);
    }
    // 再生エンジンが使う設定（出力デバイス・音量の正規化・クロスフェード）を伝える
    // （出力デバイスが変わっていれば、再生中の曲を同じ位置から続ける）
    if let Err(e) = engine.apply_options(PlaybackOptions::from(&settings)) {
        log::warn!("設定の変更を再生エンジンへ伝えられません: {}", e);
    }
    if let Err(e) = SettingsChanged(settings.clone()).emit(&app) {
        log::warn!("設定変更の通知に失敗しました: {}", e);
    }

    // ライブラリフォルダの自動反映に反映する。監視の開始はフォルダの数によって時間がかかるため、
    // 別スレッドで行い、保存の完了を待たせない（反映するのは、その時点で保存済みの設定）
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<LibrarySync>().apply_saved_settings(&app);
    });
    Ok(())
}
