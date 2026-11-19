//! メニューバーと設定ウィンドウのタイトル（設定の言語に合わせる）
//!
//! 項目とキーは、macOSのアプリの慣習（ミュージックなど）にそろえる（ADR-109）。

use crate::commands;
use crate::events::{MenuCommand, PlaybackControl};
use crate::settings::{Language, SettingsState};
use crate::tray;
use tauri::menu::{
    HELP_SUBMENU_ID, Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event;

/// 設定ウィンドウのラベル
pub const SETTINGS_WINDOW: &str = "settings";

/// 設定ウィンドウを開く時の大きさ（幅・高さ。論理ピクセル）
///
/// 左の項目の一覧（約190px）を除いた領域に、内容の幅（最大576pxと余白）が入り、
/// 13インチの画面（メニューバーとDockを除いて、高さ約700px）にも収まる大きさにする。
pub const SETTINGS_WINDOW_SIZE: (f64, f64) = (900.0, 680.0);

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

/// メインウィンドウで行う操作の項目（項目のIDと、キーと、選んだときにフロントエンドへ送る操作）
///
/// キーは、どれも⌘（macOS以外ではCtrl）付きにする。入力欄で使うキー（⌘A・⌘C・⌘Zなど）と重ならず、
/// 入力中にメニューが受け取っても困らない（ダイアログを開いている間は、フロントエンドが何もしない）。
const MENU_COMMAND_ITEMS: [(&str, &str, MenuCommand); 5] = [
    ("new_playlist", "CmdOrCtrl+N", MenuCommand::NewPlaylist),
    (
        "new_smart_playlist",
        "Alt+CmdOrCtrl+N",
        MenuCommand::NewSmartPlaylist,
    ),
    ("find", "CmdOrCtrl+F", MenuCommand::Find),
    ("edit_track_info", "CmdOrCtrl+I", MenuCommand::EditTrackInfo),
    (
        "show_in_finder",
        "Shift+CmdOrCtrl+R",
        MenuCommand::ShowInFinder,
    ),
];

/// 「再生」メニューの項目のIDから、フロントエンドへ送る操作を決める（ほかの項目は`None`）
pub fn playback_control(id: &str) -> Option<PlaybackControl> {
    PLAYBACK_ITEMS
        .iter()
        .find(|(item_id, _)| *item_id == id)
        .map(|(_, control)| *control)
}

/// メインウィンドウで行う操作の項目のIDから、フロントエンドへ送る操作を決める（ほかの項目は`None`）
pub fn menu_command(id: &str) -> Option<MenuCommand> {
    MENU_COMMAND_ITEMS
        .iter()
        .find(|(item_id, _, _)| *item_id == id)
        .map(|(_, _, command)| *command)
}

/// メインウィンドウで行う操作を、フロントエンドへ送る
///
/// 曲の情報の編集画面を開いている間は、メインウィンドウが操作を受け付けないため、送らずに編集画面を
/// 前面に出す（ADR-094）。ほかのウィンドウ（設定など）を前面にしている時・メインウィンドウを隠している時・
/// 最小化している時は、メインウィンドウを前面に出してから送る。
pub fn send_menu_command<R: Runtime>(app: &AppHandle<R>, command: MenuCommand) {
    if app
        .get_webview_window(commands::TRACK_EDITOR_WINDOW)
        .is_some()
    {
        commands::focus_track_editor(app);
        return;
    }
    tray::show_main_window(app);
    if let Err(e) = command.emit(app) {
        log::warn!("Failed to send the menu command to the main window: {}", e);
    }
}

/// メニューの文言
struct MenuLabels {
    about: &'static str,
    settings: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    quit: &'static str,
    file: &'static str,
    new_playlist: &'static str,
    new_smart_playlist: &'static str,
    import_folder: &'static str,
    import_files: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    find: &'static str,
    song: &'static str,
    edit_track_info: &'static str,
    show_in_finder: &'static str,
    view: &'static str,
    toggle_fullscreen: &'static str,
    toggle_sidebar: &'static str,
    toggle_mini_player: &'static str,
    playback: &'static str,
    /// 「再生」メニューの項目（`PLAYBACK_ITEMS`と同じ順）
    playback_items: [&'static str; PLAYBACK_ITEMS.len()],
    window: &'static str,
    minimize: &'static str,
    zoom: &'static str,
    bring_all_to_front: &'static str,
    help: &'static str,
    open_github: &'static str,
    open_log_folder: &'static str,
}

impl MenuLabels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                about: "Muspice について",
                settings: "設定...",
                services: "サービス",
                hide: "Muspice を隠す",
                hide_others: "ほかを隠す",
                show_all: "すべてを表示",
                quit: "Muspice を終了",
                file: "ファイル",
                new_playlist: "新規プレイリスト...",
                new_smart_playlist: "新規自動プレイリスト...",
                import_folder: "フォルダをインポート...",
                import_files: "ファイルをインポート...",
                close_window: "ウインドウを閉じる",
                edit: "編集",
                undo: "取り消す",
                redo: "やり直す",
                cut: "カット",
                copy: "コピー",
                paste: "ペースト",
                select_all: "すべてを選択",
                find: "検索",
                song: "曲",
                edit_track_info: "曲の情報を編集...",
                show_in_finder: "ファイルの場所を開く",
                view: "表示",
                toggle_fullscreen: "フルスクリーン切替",
                toggle_sidebar: "サイドバーを表示/隠す",
                toggle_mini_player: "ミニプレーヤー",
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
                window: "ウインドウ",
                minimize: "しまう",
                zoom: "拡大/縮小",
                bring_all_to_front: "すべてを手前に移動",
                help: "ヘルプ",
                open_github: "GitHub を開く",
                open_log_folder: "ログのフォルダを開く",
            },
            Language::En => Self {
                about: "About Muspice",
                settings: "Settings...",
                services: "Services",
                hide: "Hide Muspice",
                hide_others: "Hide Others",
                show_all: "Show All",
                quit: "Quit Muspice",
                file: "File",
                new_playlist: "New Playlist...",
                new_smart_playlist: "New Smart Playlist...",
                import_folder: "Import Folder...",
                import_files: "Import Files...",
                close_window: "Close Window",
                edit: "Edit",
                undo: "Undo",
                redo: "Redo",
                cut: "Cut",
                copy: "Copy",
                paste: "Paste",
                select_all: "Select All",
                find: "Find",
                song: "Song",
                edit_track_info: "Edit Song Info...",
                show_in_finder: "Show in Finder",
                view: "View",
                toggle_fullscreen: "Toggle Full Screen",
                toggle_sidebar: "Show/Hide Sidebar",
                toggle_mini_player: "Mini Player",
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
                window: "Window",
                minimize: "Minimize",
                zoom: "Zoom",
                bring_all_to_front: "Bring All to Front",
                help: "Help",
                open_github: "Open GitHub",
                open_log_folder: "Show Log Folder",
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

/// メインウィンドウで行う操作の項目を作る（キーは`MENU_COMMAND_ITEMS`から）
fn command_item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    label: &str,
) -> tauri::Result<MenuItem<R>> {
    let mut builder = MenuItemBuilder::with_id(id, label);
    if let Some((_, accelerator, _)) = MENU_COMMAND_ITEMS
        .iter()
        .find(|(item_id, _, _)| *item_id == id)
    {
        builder = builder.accelerator(*accelerator);
    }
    builder.build(app)
}

/// メニューバーを作る（項目のIDは言語によらず同じ。`lib.rs`の`on_menu_event`が使う）
pub fn build_menu<R: Runtime>(app: &AppHandle<R>, language: Language) -> tauri::Result<Menu<R>> {
    let labels = MenuLabels::for_language(language);

    // ほかのmacOSのアプリと同じ並び（隠す・ほかを隠す・すべてを表示はOSが動かす）
    let app_menu = SubmenuBuilder::new(app, "Muspice")
        .about_with_text(labels.about, None)
        .separator()
        .item(
            &MenuItemBuilder::with_id("settings", labels.settings)
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .separator()
        .services_with_text(labels.services)
        .separator()
        .hide_with_text(labels.hide)
        .hide_others_with_text(labels.hide_others)
        .show_all_with_text(labels.show_all)
        .separator()
        .quit_with_text(labels.quit)
        .build()?;

    let file_menu = SubmenuBuilder::new(app, labels.file)
        .item(&command_item(app, "new_playlist", labels.new_playlist)?)
        .item(&command_item(
            app,
            "new_smart_playlist",
            labels.new_smart_playlist,
        )?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("import_folder", labels.import_folder)
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("import_files", labels.import_files)
                .accelerator("Shift+CmdOrCtrl+O")
                .build(app)?,
        )
        .separator()
        .close_window_with_text(labels.close_window)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, labels.edit)
        .undo_with_text(labels.undo)
        .redo_with_text(labels.redo)
        .separator()
        .cut_with_text(labels.cut)
        .copy_with_text(labels.copy)
        .paste_with_text(labels.paste)
        .select_all_with_text(labels.select_all)
        .separator()
        .item(&command_item(app, "find", labels.find)?)
        .build()?;

    // 一覧で選んでいる曲への操作（右クリックのメニューと同じ。選んでいなければ何もしない）
    let song_menu = SubmenuBuilder::new(app, labels.song)
        .item(&command_item(
            app,
            "edit_track_info",
            labels.edit_track_info,
        )?)
        .item(&command_item(app, "show_in_finder", labels.show_in_finder)?)
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
        .item(
            &MenuItemBuilder::with_id("toggle_mini_player", labels.toggle_mini_player)
                .accelerator("CmdOrCtrl+Shift+M")
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

    // macOSの「ウインドウ」メニュー（IDで、OSがウインドウの一覧などを足すメニューにする）
    let window_menu = SubmenuBuilder::with_id(app, WINDOW_SUBMENU_ID, labels.window)
        .minimize_with_text(labels.minimize)
        .maximize_with_text(labels.zoom)
        .separator()
        .bring_all_to_front_with_text(labels.bring_all_to_front)
        .build()?;

    // macOSの「ヘルプ」メニュー（IDで、OSが項目の検索の欄を足すメニューにする）
    let help_menu = SubmenuBuilder::with_id(app, HELP_SUBMENU_ID, labels.help)
        .item(&MenuItemBuilder::with_id("about", labels.about).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("open_github", labels.open_github).build(app)?)
        .item(&MenuItemBuilder::with_id("open_log_folder", labels.open_log_folder).build(app)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &song_menu,
            &view_menu,
            &playback_menu,
            &window_menu,
            &help_menu,
        ])
        .build()
}

/// 言語に合わせて、メニューバーと設定ウィンドウのタイトルを作り直す（言語を変えて保存したときに呼ぶ）
pub fn apply_language<R: Runtime>(app: &AppHandle<R>, language: Language) {
    match build_menu(app, language) {
        Ok(menu) => {
            if let Err(e) = app.set_menu(menu) {
                log::error!("Failed to rebuild the menu bar: {}", e);
            }
        }
        Err(e) => log::error!("Failed to build the menu bar: {}", e),
    }
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW)
        && let Err(e) = window.set_title(settings_window_title(language))
    {
        log::warn!("Failed to change the settings window title: {}", e);
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

    #[test]
    fn test_menu_command_maps_menu_items_to_commands() {
        assert_eq!(menu_command("new_playlist"), Some(MenuCommand::NewPlaylist));
        assert_eq!(
            menu_command("new_smart_playlist"),
            Some(MenuCommand::NewSmartPlaylist)
        );
        assert_eq!(menu_command("find"), Some(MenuCommand::Find));
        assert_eq!(
            menu_command("edit_track_info"),
            Some(MenuCommand::EditTrackInfo)
        );
        assert_eq!(
            menu_command("show_in_finder"),
            Some(MenuCommand::ShowInFinder)
        );
        // ほかのメニューの項目・「再生」メニューの項目は、メインウィンドウの操作ではない
        assert_eq!(menu_command("import_folder"), None);
        assert_eq!(menu_command("playback_toggle"), None);
        assert_eq!(menu_command(""), None);
    }

    #[test]
    fn test_menu_command_items_have_unique_ids_and_keys() {
        let mut ids: Vec<&str> = MENU_COMMAND_ITEMS.iter().map(|(id, _, _)| *id).collect();
        // 「再生」メニューの項目と、IDが重ならない
        ids.extend(PLAYBACK_ITEMS.iter().map(|(id, _)| *id));
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);

        // メニューにじかに書いたキー（設定・インポート・表示）とも重ならない
        let mut keys: Vec<&str> = MENU_COMMAND_ITEMS.iter().map(|(_, key, _)| *key).collect();
        keys.extend([
            "CmdOrCtrl+,",
            "CmdOrCtrl+O",
            "Shift+CmdOrCtrl+O",
            "Ctrl+CmdOrCtrl+F",
            "CmdOrCtrl+\\\\",
            "CmdOrCtrl+Shift+M",
        ]);
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }
}
