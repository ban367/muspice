mod album_art;
mod commands;
mod db;
mod error;
mod events;
mod library;
mod library_folder;
mod library_sync;
mod menu;
mod metadata;
mod models;
mod playlist;
mod repository;
mod settings;
mod state;
mod validation;

use commands::{
    add_track_to_playlist, create_playlist, delete_playlist, delete_tracks_command,
    delete_tracks_with_files_command, filter_tracks, get_albums_grouped, get_all_tracks,
    get_artists_grouped, get_current_track, get_favorite_tracks, get_genres_grouped,
    get_library_folders, get_most_played_tracks, get_playlists, get_recently_played_tracks,
    get_settings, get_track_file_path, get_unique_albums, get_unique_artists, get_unique_genres,
    import_folder, increment_play_count, open_project_page, refresh_library_metadata,
    remove_library_folder, remove_track_from_playlist, rename_playlist, reorder_playlist_tracks,
    rescan_library_folder, save_settings, search_tracks, set_current_track, set_rating,
    show_in_folder, toggle_favorite, update_multiple_tracks_metadata, update_track_metadata,
    update_track_metadata_with_file,
};
use state::AppState;
use std::path::PathBuf;
use tauri::Manager;
use tauri::webview::WebviewWindowBuilder;
use tauri_specta::Event;

/// tauri-spectaビルダーを構築する
///
/// コマンド一覧はここで一元管理され、TypeScriptバインディング
/// （`src/lib/bindings.ts`）はデバッグビルド起動時に自動生成される。
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // エラーは{code, message}のAppErrorをthrowする（フロントのhandleErrorと整合）
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .commands(tauri_specta::collect_commands![
            import_folder,
            get_library_folders,
            remove_library_folder,
            rescan_library_folder,
            get_settings,
            save_settings,
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
            create_playlist,
            get_playlists,
            delete_playlist,
            rename_playlist,
            add_track_to_playlist,
            remove_track_from_playlist,
            reorder_playlist_tracks,
            get_track_file_path,
            set_current_track,
            get_current_track,
            show_in_folder,
            open_project_page,
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
        .events(tauri_specta::collect_events![
            events::ImportProgress,
            events::LibraryScanProgress,
            events::LibraryChanged,
            events::ShowAboutDialog,
            events::OpenImportDialog,
            events::ToggleSidebar,
            events::SettingsChanged
        ])
}

/// ログファイル1つあたりの上限（超えたら日時付きの名前に変えて新しいファイルに切り替える）
const LOG_MAX_FILE_SIZE: u128 = 5 * 1024 * 1024;
/// 切り替えた古いログファイルを残す数
const LOG_KEEP_ROTATED_FILES: usize = 4;

/// ログプラグインを構築する
///
/// `log`クレートのロガーとして登録し、アプリ・Tauri・依存クレートのログを標準出力と
/// OS標準のログフォルダ（例: macOSは`~/Library/Logs/<identifier>`）に出力する。
/// プラグインのJS API（`log:default`）はcapabilityに追加せず、WebViewには公開しない（ADR-005）
fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

    tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir { file_name: None }),
        ])
        .max_file_size(LOG_MAX_FILE_SIZE)
        .rotation_strategy(RotationStrategy::KeepSome(LOG_KEEP_ROTATED_FILES))
        // 既定はUTCのため、ログの時刻とファイル名をローカル時刻にする
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

/// TypeScriptエクスポート設定
fn typescript_exporter() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default()
}

/// TypeScriptバインディングの出力先
///
/// 実行時のカレントディレクトリに依存しないよう、コンパイル時に確定する
/// クレートルート（`CARGO_MANIFEST_DIR`）を基点に解決する。
#[cfg(any(debug_assertions, test))]
fn bindings_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // デバッグビルド時にTypeScriptバインディングを自動生成する
    #[cfg(debug_assertions)]
    builder
        .export(typescript_exporter(), bindings_path())
        .expect("TypeScriptバインディングのエクスポートに失敗しました");

    // WebViewに公開するプラグインはdialogのみ。ファイルアクセスやURL/フォルダを開く処理は
    // Rust側のコマンドで行い、WebViewから任意のパスやURLを扱えないようにする
    // （tauri-plugin-openerはRust側の自由関数のみを使うためプラグイン登録しない）
    tauri::Builder::default()
        // 他のプラグインの初期化中のログも記録できるよう、最初に登録する
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        // アルバムアートは`<img>`から`albumart://`で直接読み込む（IPCでbase64を渡さない）
        .register_asynchronous_uri_scheme_protocol(album_art::SCHEME, |ctx, request, responder| {
            album_art::handle_request(ctx.app_handle().clone(), request, responder);
        })
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // 型付きイベントを登録する（未登録のイベントをemitするとパニックする）
            builder.mount_events(app);

            // アプリケーションデータディレクトリを取得
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("アプリケーションデータディレクトリの取得に失敗しました");

            // ディレクトリが存在しない場合は作成
            std::fs::create_dir_all(&app_data_dir)
                .expect("アプリケーションデータディレクトリの作成に失敗しました");

            log::info!("アプリケーションを起動しました");

            // データベースファイルのパスを設定
            let db_path: PathBuf = app_data_dir.join("muspice.db");

            // データベースを初期化
            let conn = db::init_db(db_path).expect("データベースの初期化に失敗しました");

            log::info!("データベースを初期化しました");

            // アプリケーション状態を作成して管理
            let app_state = AppState::new(conn);
            app.manage(app_state);

            // 設定を読み込む（ない・壊れている場合は既定値）
            app.manage(settings::SettingsState::load(
                app_data_dir.join("settings.json"),
            ));

            // ライブラリフォルダの変更の自動反映（設定に応じて、起動時の再スキャン・定期的な
            // 再スキャン・フォルダの監視を別スレッドで始める）
            app.manage(library_sync::LibrarySync::default());
            library_sync::start(app.handle());

            // メニューバーを構築（設定の言語に合わせる。言語を変えると`save_settings`が作り直す）
            let menu = menu::build_menu(app.handle(), menu::current_language(app.handle()))?;
            app.set_menu(menu)?;

            log::info!("メニューバーを初期化しました");

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "settings" => {
                    // 設定ウィンドウを開く（既存なら前面に）
                    if let Some(window) = app.get_webview_window(menu::SETTINGS_WINDOW) {
                        let _ = window.set_focus();
                    } else {
                        let _ = WebviewWindowBuilder::new(
                            app,
                            menu::SETTINGS_WINDOW,
                            tauri::WebviewUrl::App("/settings".into()),
                        )
                        .title(menu::settings_window_title(menu::current_language(app)))
                        .inner_size(700.0, 500.0)
                        .resizable(true)
                        .build();
                    }
                }
                "about" => {
                    // Aboutダイアログを表示するイベントをフロントエンドに送信
                    let _ = events::ShowAboutDialog.emit(app);
                }
                "import_folder" => {
                    // インポートダイアログを開くイベントをフロントエンドに送信
                    let _ = events::OpenImportDialog.emit(app);
                }
                "toggle_sidebar" => {
                    // サイドバー切替イベントをフロントエンドに送信
                    let _ = events::ToggleSidebar.emit(app);
                }
                "toggle_fullscreen" => {
                    // フルスクリーン切替
                    if let Some(window) = app.get_webview_window("main")
                        && let Ok(is_fullscreen) = window.is_fullscreen()
                    {
                        let _ = window.set_fullscreen(!is_fullscreen);
                    }
                }
                "open_github" => {
                    // GitHubを開く
                    let _ = tauri_plugin_opener::open_url(commands::PROJECT_URL, None::<&str>);
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// TypeScriptバインディングを生成する（`cargo test`でも再生成可能にする）
    ///
    /// エクスポートが壊れた場合（Type未実装の型の混入等）はこのテストが失敗する。
    #[test]
    fn export_typescript_bindings() {
        super::specta_builder()
            .export(super::typescript_exporter(), super::bindings_path())
            .expect("TypeScriptバインディングのエクスポートに失敗しました");
    }
}
