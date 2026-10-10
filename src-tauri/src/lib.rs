mod album_art;
mod commands;
mod db;
mod device;
mod device_manifest;
mod device_sync;
mod device_transfer;
mod error;
mod events;
mod global_shortcuts;
mod library;
mod library_folder;
mod library_sync;
mod library_xml;
mod lyrics;
mod m3u;
mod media_controls;
mod menu;
mod metadata;
mod mini_player;
mod models;
mod playback;
mod playback_state;
mod playlist;
mod repository;
mod search_text;
mod settings;
mod smart_playlist;
mod state;
mod tag_backfill;
mod track_notification;
mod track_relink;
mod tray;
mod validation;

use commands::{
    add_tracks_to_playlist, apply_metadata_changes, cancel_device_sync,
    count_smart_playlist_tracks, create_playlist, create_playlist_folder,
    create_playlist_with_tracks, create_smart_playlist, delete_playlist, delete_playlist_folder,
    delete_tracks_command, delete_tracks_with_files_command, export_playlist_m3u, filter_tracks,
    get_album_art_info, get_album_tracks, get_albums, get_all_tracks, get_artist_albums,
    get_artists, get_current_track, get_favorite_tracks, get_genre_tracks, get_genres,
    get_global_shortcuts, get_library_folders, get_most_played_tracks, get_output_devices,
    get_play_history, get_playback_state, get_playlist_folders, get_playlist_tracks, get_playlists,
    get_settings, get_sync_devices, get_track_lyrics, get_track_tags, get_unique_albums,
    get_unique_artists, get_unique_genres, import_folder, import_library_xml, import_m3u_playlists,
    increment_play_count, increment_skip_count, move_playlist, open_project_page, plan_device_sync,
    playback_pause, playback_play, playback_resume, playback_seek, playback_set_equalizer,
    playback_set_next, playback_set_volume, playback_stop, refresh_library_metadata,
    register_sync_device, relink_sync_device, remove_album_art, remove_library_folder,
    remove_missing_tracks, remove_sync_device, remove_tracks_from_playlist, rename_playlist,
    rename_playlist_folder, reorder_playlist_folders, reorder_playlist_tracks, reorder_playlists,
    rescan_library_folder, reshuffle_smart_playlists, run_device_sync, save_playback_state,
    save_settings, search_tracks, set_album_art, set_current_track, set_favorite, set_mini_player,
    set_now_playing, set_playlist_description, set_rating, show_in_folder,
    update_multiple_tracks_metadata, update_smart_playlist, update_sync_device,
    update_track_metadata, write_library_metadata_to_files,
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
            remove_missing_tracks,
            get_sync_devices,
            register_sync_device,
            update_sync_device,
            relink_sync_device,
            remove_sync_device,
            plan_device_sync,
            run_device_sync,
            cancel_device_sync,
            get_settings,
            save_settings,
            get_global_shortcuts,
            set_mini_player,
            get_all_tracks,
            search_tracks,
            filter_tracks,
            get_unique_artists,
            get_unique_albums,
            get_unique_genres,
            get_albums,
            get_album_tracks,
            get_artists,
            get_artist_albums,
            get_genres,
            get_genre_tracks,
            get_track_tags,
            get_track_lyrics,
            update_track_metadata,
            update_multiple_tracks_metadata,
            apply_metadata_changes,
            write_library_metadata_to_files,
            get_album_art_info,
            set_album_art,
            remove_album_art,
            create_playlist,
            create_smart_playlist,
            update_smart_playlist,
            count_smart_playlist_tracks,
            reshuffle_smart_playlists,
            get_playlists,
            get_playlist_tracks,
            delete_playlist,
            rename_playlist,
            add_tracks_to_playlist,
            remove_tracks_from_playlist,
            reorder_playlist_tracks,
            create_playlist_with_tracks,
            set_playlist_description,
            get_playlist_folders,
            create_playlist_folder,
            rename_playlist_folder,
            delete_playlist_folder,
            move_playlist,
            reorder_playlists,
            reorder_playlist_folders,
            import_m3u_playlists,
            export_playlist_m3u,
            import_library_xml,
            set_current_track,
            get_current_track,
            save_playback_state,
            get_playback_state,
            playback_play,
            playback_set_next,
            playback_pause,
            playback_resume,
            playback_seek,
            playback_set_volume,
            playback_set_equalizer,
            playback_stop,
            get_output_devices,
            set_now_playing,
            show_in_folder,
            open_project_page,
            set_favorite,
            set_rating,
            increment_play_count,
            increment_skip_count,
            get_favorite_tracks,
            get_most_played_tracks,
            get_play_history,
            delete_tracks_command,
            delete_tracks_with_files_command,
            refresh_library_metadata
        ])
        .events(tauri_specta::collect_events![
            events::ImportProgress,
            events::LibraryScanProgress,
            events::DeviceSyncProgress,
            events::LibraryChanged,
            events::ShowAboutDialog,
            events::OpenImportDialog,
            events::ToggleSidebar,
            events::ToggleMiniPlayer,
            events::SettingsChanged,
            events::PlaybackEvent,
            events::PlaybackControl
        ])
        // 設定の値の範囲（フロントの設定画面・モックと共有する）
        .constant("DEFAULT_ACCENT_COLOR", settings::DEFAULT_ACCENT_COLOR)
        .constant("MAX_CROSSFADE_SECONDS", settings::MAX_CROSSFADE_SECONDS)
        .constant("LIBRARY_SCAN_INTERVALS", settings::LIBRARY_SCAN_INTERVALS)
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
        // 再生エンジンのデコーダー（symphonia）は、曲を開くたびに形式の情報などをinfo・warnで
        // 出すため、エラーだけを残す（再生できない場合の理由は、再生エンジンがログに残す）
        .filter(|metadata| {
            !metadata.target().starts_with("symphonia") || metadata.level() <= log::Level::Error
        })
        .max_file_size(LOG_MAX_FILE_SIZE)
        .rotation_strategy(RotationStrategy::KeepSome(LOG_KEEP_ROTATED_FILES))
        // 既定はUTCのため、ログの時刻とファイル名をローカル時刻にする
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

/// ウィンドウの状態（サイズ・位置・最大化・フルスクリーン）を記憶するプラグインを構築する
///
/// 終了時にアプリの設定フォルダの`.window-state.json`へ保存し、次回の起動時に復元する
/// （保存した位置のモニターが見つからない場合、位置はOSに任せる）。記憶がない初回は
/// `tauri.conf.json`の大きさで開く。対象はメインウィンドウだけで、設定ウィンドウは毎回
/// 同じ大きさで開く。
/// プラグインのJS API（`window-state:default`）はcapabilityに追加せず、WebViewには公開しない（ADR-005）
fn window_state_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_window_state::StateFlags;

    tauri_plugin_window_state::Builder::new()
        .with_state_flags(
            StateFlags::SIZE
                | StateFlags::POSITION
                | StateFlags::MAXIMIZED
                | StateFlags::FULLSCREEN,
        )
        .with_denylist(&[menu::SETTINGS_WINDOW])
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
        // メインウィンドウを作る前に登録し、作った時点で前回の大きさ・位置に戻す
        .plugin(window_state_plugin())
        .plugin(tauri_plugin_dialog::init())
        // グローバルホットキーと通知は、Rust側だけで使う（capabilityに追加せず、WebViewには公開しない）
        .plugin(global_shortcuts::plugin())
        .plugin(tauri_plugin_notification::init())
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
            let settings_state = settings::SettingsState::load(app_data_dir.join("settings.json"));
            let initial_settings = settings_state.get().unwrap_or_default();
            let playback_options = playback::PlaybackOptions::from(&initial_settings);
            app.manage(settings_state);

            // メニューバーへの常駐・グローバルホットキー・曲の変更の通知（どれも設定でオンにした時だけ）
            app.manage(tray::TrayState::<tauri::Wry>::default());
            app.manage(global_shortcuts::GlobalShortcutState::default());
            app.manage(track_notification::TrackNotifier::default());
            tray::apply(
                app.handle(),
                initial_settings.stay_in_menu_bar,
                initial_settings.language,
            );
            global_shortcuts::apply(app.handle(), initial_settings.global_shortcuts);

            // ミニプレーヤー: 前回、小さな表示のまま終了していたら、ウィンドウを元の大きさに戻す
            let mini_player =
                mini_player::MiniPlayerState::new(app_data_dir.join("mini-player.json"));
            if let Some(window) = app.get_webview_window(tray::MAIN_WINDOW) {
                mini_player::recover(&window, &mini_player);
            }
            app.manage(mini_player);

            // 前回の再生状態（音量・再生キューなど）を読み込む（ない・壊れている場合は空）
            app.manage(playback_state::PlaybackStateStore::load(
                app_data_dir.join("playback-state.json"),
            ));

            // 再生エンジン（スレッドを始めるだけで、出力は最初に再生するときに開く）
            let playback_events = app.handle().clone();
            app.manage(playback::PlaybackEngine::start(
                playback_options,
                move |event| {
                    if let Err(e) = event.emit(&playback_events) {
                        log::warn!("再生エンジンの通知を送れません: {}", e);
                    }
                },
            ));

            // OSのメディアキー・Now Playing（OSからの操作を受け取り始める。対応していないOSでは
            // 何もしない）
            app.manage(media_controls::MediaControls::start(app.handle()));

            // ライブラリフォルダの変更の自動反映（設定に応じて、起動時の再スキャン・定期的な
            // 再スキャン・フォルダの監視を別スレッドで始める）
            app.manage(library_sync::LibrarySync::default());
            library_sync::start(app.handle());

            // アルバムアーティストの列を追加する前に登録したトラックがあれば、別スレッドで
            // ファイルのタグから読み込む（対象がなければ何もしない）
            tag_backfill::start(app.handle());

            // デバイスへの同期の実行状態（同時に実行する同期は1つだけ）
            app.manage(device::DeviceSyncState::default());

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
                "toggle_mini_player" => {
                    // ミニプレーヤーの切り替えは、フロントエンドが行う（画面も切り替えるため）
                    let _ = events::ToggleMiniPlayer.emit(app);
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
                _ => {
                    // メニューバーのアイコンのメニュー
                    if tray::handle_menu_event(app, id) {
                        return;
                    }
                    // 「再生」メニュー: 再生の操作をフロントエンドに送信
                    if let Some(control) = menu::playback_control(id) {
                        let _ = control.emit(app);
                    }
                }
            }
        })
        .on_window_event(|window, event| {
            // メニューバーに常駐している間は、メインウィンドウを閉じても終了せず、隠すだけにする
            // （再生を続ける。アイコンのメニューから、もう一度表示できる）
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == tray::MAIN_WINDOW
                && tray::stays_in_menu_bar(window.app_handle())
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // macOS: Dockのアイコンを押した時に、隠してあるメインウィンドウを表示する
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                tray::show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
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
