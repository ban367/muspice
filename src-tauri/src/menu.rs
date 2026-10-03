//! メニューバーと設定ウィンドウのタイトル（設定の言語に合わせる）

use crate::settings::{Language, SettingsState};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};

/// 設定ウィンドウのラベル
pub const SETTINGS_WINDOW: &str = "settings";

/// メニューの文言
struct MenuLabels {
    settings: &'static str,
    file: &'static str,
    import_folder: &'static str,
    edit: &'static str,
    view: &'static str,
    toggle_fullscreen: &'static str,
    toggle_sidebar: &'static str,
    help: &'static str,
    about: &'static str,
    open_github: &'static str,
}

impl MenuLabels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                settings: "設定...",
                file: "ファイル",
                import_folder: "フォルダをインポート...",
                edit: "編集",
                view: "表示",
                toggle_fullscreen: "フルスクリーン切替",
                toggle_sidebar: "サイドバーを表示/隠す",
                help: "ヘルプ",
                about: "Muspice について",
                open_github: "GitHub を開く",
            },
            Language::En => Self {
                settings: "Settings...",
                file: "File",
                import_folder: "Import Folder...",
                edit: "Edit",
                view: "View",
                toggle_fullscreen: "Toggle Full Screen",
                toggle_sidebar: "Show/Hide Sidebar",
                help: "Help",
                about: "About Muspice",
                open_github: "Open GitHub",
            },
        }
    }
}

/// 設定ウィンドウのタイトル
pub fn settings_window_title(language: Language) -> &'static str {
    match language {
        Language::Ja => "設定",
        Language::En => "Settings",
    }
}

/// 保存されている設定の言語（読めない場合は既定値）
pub fn current_language<R: Runtime>(app: &AppHandle<R>) -> Language {
    app.try_state::<SettingsState>()
        .and_then(|state| state.get().ok())
        .map(|settings| settings.language)
        .unwrap_or_default()
}

/// メニューバーを作る（項目のIDは言語によらず同じ。`lib.rs`の`on_menu_event`が使う）
pub fn build_menu<R: Runtime>(app: &AppHandle<R>, language: Language) -> tauri::Result<Menu<R>> {
    let labels = MenuLabels::for_language(language);

    let app_menu = SubmenuBuilder::new(app, "Muspice")
        .about(None)
        .separator()
        .item(
            &MenuItemBuilder::with_id("settings", labels.settings)
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .separator()
        .quit()
        .build()?;

    let file_menu = SubmenuBuilder::new(app, labels.file)
        .item(
            &MenuItemBuilder::with_id("import_folder", labels.import_folder)
                .accelerator("CmdOrCtrl+I")
                .build(app)?,
        )
        .separator()
        .close_window()
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, labels.edit)
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, labels.view)
        .item(
            &MenuItemBuilder::with_id("toggle_fullscreen", labels.toggle_fullscreen)
                .accelerator("Ctrl+CmdOrCtrl+F")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("toggle_sidebar", labels.toggle_sidebar)
                .accelerator("CmdOrCtrl+\\")
                .build(app)?,
        )
        .build()?;

    let help_menu = SubmenuBuilder::with_id(app, "help", labels.help)
        .item(&MenuItemBuilder::with_id("about", labels.about).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("open_github", labels.open_github).build(app)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &help_menu])
        .build()
}

/// 言語に合わせて、メニューバーと設定ウィンドウのタイトルを作り直す（言語を変えて保存したときに呼ぶ）
pub fn apply_language<R: Runtime>(app: &AppHandle<R>, language: Language) {
    match build_menu(app, language) {
        Ok(menu) => {
            if let Err(e) = app.set_menu(menu) {
                log::error!("メニューバーを作り直せませんでした: {}", e);
            }
        }
        Err(e) => log::error!("メニューバーを作れませんでした: {}", e),
    }
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW)
        && let Err(e) = window.set_title(settings_window_title(language))
    {
        log::warn!("設定ウィンドウのタイトルを変えられませんでした: {}", e);
    }
}
