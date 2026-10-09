//! メニューバーと設定ウィンドウのタイトル（設定の言語に合わせる）

use crate::events::PlaybackControl;
use crate::settings::{Language, SettingsState};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};

/// 設定ウィンドウのラベル
pub const SETTINGS_WINDOW: &str = "settings";

/// 「再生」メニューの項目（項目のIDと、選んだときにフロントエンドへ送る操作）
///
/// キーは割り当てない。ウィンドウの中のキー操作（`Player.svelte`。スペースで再生 / 一時停止など）と
/// 同じキーをメニューに割り当てると、検索欄などへの入力中でもメニューが先に受け取ってしまう。
const PLAYBACK_ITEMS: [(&str, PlaybackControl); 8] = [
    ("playback_toggle", PlaybackControl::Toggle),
    ("playback_next", PlaybackControl::Next),
    ("playback_previous", PlaybackControl::Previous),
    ("playback_volume_up", PlaybackControl::VolumeUp),
    ("playback_volume_down", PlaybackControl::VolumeDown),
    ("playback_toggle_mute", PlaybackControl::ToggleMute),
    ("playback_toggle_shuffle", PlaybackControl::ToggleShuffle),
    ("playback_toggle_repeat", PlaybackControl::ToggleRepeat),
];

/// 「再生」メニューの中で、前に区切り線を入れる項目（曲の移動・音量・再生の順のまとまりに分ける）
const PLAYBACK_GROUP_STARTS: [&str; 2] = ["playback_volume_up", "playback_toggle_shuffle"];

/// 「再生」メニューの項目のIDから、フロントエンドへ送る操作を決める（ほかの項目は`None`）
pub fn playback_control(id: &str) -> Option<PlaybackControl> {
    PLAYBACK_ITEMS
        .iter()
        .find(|(item_id, _)| *item_id == id)
        .map(|(_, control)| *control)
}

/// メニューの文言
struct MenuLabels {
    settings: &'static str,
    file: &'static str,
    import_folder: &'static str,
    edit: &'static str,
    view: &'static str,
    toggle_fullscreen: &'static str,
    toggle_sidebar: &'static str,
    playback: &'static str,
    /// 「再生」メニューの項目（`PLAYBACK_ITEMS`と同じ順）
    playback_items: [&'static str; PLAYBACK_ITEMS.len()],
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
                playback: "再生",
                playback_items: [
                    "再生 / 一時停止",
                    "次の曲",
                    "前の曲",
                    "音量を上げる",
                    "音量を下げる",
                    "ミュートの切り替え",
                    "シャッフルの切り替え",
                    "リピートの切り替え",
                ],
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
                playback: "Playback",
                playback_items: [
                    "Play/Pause",
                    "Next Track",
                    "Previous Track",
                    "Increase Volume",
                    "Decrease Volume",
                    "Toggle Mute",
                    "Toggle Shuffle",
                    "Toggle Repeat",
                ],
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

    let mut playback_menu = SubmenuBuilder::new(app, labels.playback);
    for ((id, _), label) in PLAYBACK_ITEMS.iter().zip(labels.playback_items) {
        if PLAYBACK_GROUP_STARTS.contains(id) {
            playback_menu = playback_menu.separator();
        }
        playback_menu = playback_menu.item(&MenuItemBuilder::with_id(*id, label).build(app)?);
    }
    let playback_menu = playback_menu.build()?;

    let help_menu = SubmenuBuilder::with_id(app, "help", labels.help)
        .item(&MenuItemBuilder::with_id("about", labels.about).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("open_github", labels.open_github).build(app)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &playback_menu,
            &help_menu,
        ])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_control_maps_menu_items_to_controls() {
        assert_eq!(
            playback_control("playback_toggle"),
            Some(PlaybackControl::Toggle)
        );
        assert_eq!(
            playback_control("playback_toggle_repeat"),
            Some(PlaybackControl::ToggleRepeat)
        );
        // ほかのメニューの項目は、再生の操作ではない
        assert_eq!(playback_control("toggle_sidebar"), None);
        assert_eq!(playback_control(""), None);
    }

    #[test]
    fn test_playback_items_have_unique_ids_and_labels_in_every_language() {
        let mut ids: Vec<&str> = PLAYBACK_ITEMS.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PLAYBACK_ITEMS.len());

        for start in PLAYBACK_GROUP_STARTS {
            assert!(playback_control(start).is_some());
        }
        for language in [Language::Ja, Language::En] {
            let labels = MenuLabels::for_language(language);
            assert!(labels.playback_items.iter().all(|label| !label.is_empty()));
        }
    }
}
