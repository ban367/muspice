mod commands;
mod db;
mod error;
mod library;
mod logger;
mod metadata;
mod models;
mod playlist;
mod repository;
mod state;
mod validation;

use commands::{
    add_track_to_playlist, create_playlist, delete_playlist, delete_tracks_command,
    delete_tracks_with_files_command, filter_tracks, get_album_art, get_albums_grouped,
    get_all_tracks, get_artists_grouped, get_current_track, get_favorite_tracks,
    get_genres_grouped, get_most_played_tracks, get_playlists, get_recently_played_tracks,
    get_track_file_path, get_unique_albums, get_unique_artists, get_unique_genres, import_folder,
    increment_play_count, refresh_library_metadata, remove_track_from_playlist, rename_playlist,
    reorder_playlist_tracks, search_tracks, set_current_track, set_rating, show_in_folder,
    toggle_favorite, update_multiple_tracks_metadata, update_track_metadata,
    update_track_metadata_with_file, validate_metadata_command,
};
use state::AppState;
use std::path::PathBuf;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::webview::WebviewWindowBuilder;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            import_folder,
            get_all_tracks,
            search_tracks,
            filter_tracks,
            get_unique_artists,
            get_unique_albums,
            get_unique_genres,
            get_albums_grouped,
            get_artists_grouped,
            get_genres_grouped,
            update_track_metadata,
            update_track_metadata_with_file,
            update_multiple_tracks_metadata,
            validate_metadata_command,
            create_playlist,
            get_playlists,
            delete_playlist,
            rename_playlist,
            add_track_to_playlist,
            remove_track_from_playlist,
            reorder_playlist_tracks,
            get_album_art,
            get_track_file_path,
            set_current_track,
            get_current_track,
            show_in_folder,
            toggle_favorite,
            set_rating,
            increment_play_count,
            get_favorite_tracks,
            get_most_played_tracks,
            get_recently_played_tracks,
            delete_tracks_command,
            delete_tracks_with_files_command,
            refresh_library_metadata
        ])
        .setup(|app| {
            // アプリケーションデータディレクトリを取得
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("アプリケーションデータディレクトリの取得に失敗しました");

            // ディレクトリが存在しない場合は作成
            std::fs::create_dir_all(&app_data_dir)
                .expect("アプリケーションデータディレクトリの作成に失敗しました");

            // ログディレクトリを設定
            let log_dir = app_data_dir.join("logs");

            // ロガーを初期化
            logger::init_logger(log_dir, logger::LogLevel::Info)
                .expect("ロガーの初期化に失敗しました");

            logger::info("アプリケーションを起動しました");

            // データベースファイルのパスを設定
            let db_path: PathBuf = app_data_dir.join("muspice.db");

            // データベースを初期化
            let conn = db::init_db(db_path).expect("データベースの初期化に失敗しました");

            logger::info("データベースを初期化しました");

            // アプリケーション状態を作成して管理
            let app_state = AppState::new(conn);
            app.manage(app_state);

            // メニューバーを構築
            let app_menu = SubmenuBuilder::new(app, "Muspice")
                .about(None)
                .separator()
                .item(
                    &MenuItemBuilder::with_id("settings", "設定...")
                        .accelerator("CmdOrCtrl+,")
                        .build(app)?,
                )
                .separator()
                .quit()
                .build()?;

            let file_menu = SubmenuBuilder::new(app, "ファイル")
                .item(
                    &MenuItemBuilder::with_id("import_folder", "フォルダをインポート...")
                        .accelerator("CmdOrCtrl+I")
                        .build(app)?,
                )
                .separator()
                .close_window()
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "編集")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            let view_menu = SubmenuBuilder::new(app, "表示")
                .item(
                    &MenuItemBuilder::with_id("toggle_fullscreen", "フルスクリーン切替")
                        .accelerator("Ctrl+CmdOrCtrl+F")
                        .build(app)?,
                )
                .separator()
                .item(
                    &MenuItemBuilder::with_id("toggle_sidebar", "サイドバーを表示/隠す")
                        .accelerator("CmdOrCtrl+\\")
                        .build(app)?,
                )
                .build()?;

            let help_menu = SubmenuBuilder::with_id(app, "help", "ヘルプ")
                .item(&MenuItemBuilder::with_id("about", "Muspice について").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("open_github", "GitHub を開く").build(app)?)
                .build()?;

            let menu = MenuBuilder::new(app)
                .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &help_menu])
                .build()?;

            app.set_menu(menu)?;

            logger::info("メニューバーを初期化しました");

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "settings" => {
                    // 設定ウィンドウを開く（既存なら前面に）
                    if let Some(window) = app.get_webview_window("settings") {
                        let _ = window.set_focus();
                    } else {
                        let _ = WebviewWindowBuilder::new(
                            app,
                            "settings",
                            tauri::WebviewUrl::App("/settings".into()),
                        )
                        .title("設定")
                        .inner_size(700.0, 500.0)
                        .resizable(true)
                        .build();
                    }
                }
                "about" => {
                    // Aboutダイアログを表示するイベントをフロントエンドに送信
                    let _ = app.emit("show-about-dialog", ());
                }
                "import_folder" => {
                    // インポートダイアログを開くイベントをフロントエンドに送信
                    let _ = app.emit("open-import-dialog", ());
                }
                "toggle_sidebar" => {
                    // サイドバー切替イベントをフロントエンドに送信
                    let _ = app.emit("toggle-sidebar", ());
                }
                "toggle_fullscreen" => {
                    // フルスクリーン切替
                    if let Some(window) = app.get_webview_window("main") {
                        if let Ok(is_fullscreen) = window.is_fullscreen() {
                            let _ = window.set_fullscreen(!is_fullscreen);
                        }
                    }
                }
                "open_github" => {
                    // GitHubを開く
                    let _ = tauri_plugin_opener::open_url(
                        "https://github.com/ban367/muspice",
                        None::<&str>,
                    );
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
