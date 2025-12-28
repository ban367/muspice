mod commands;
mod db;
mod error;
mod library;
mod logger;
mod metadata;
mod models;
mod playlist;
mod state;
mod validation;

use commands::{
    add_track_to_playlist, create_playlist, filter_tracks, get_all_tracks, get_current_track,
    get_playlists, get_track_file_path, get_unique_albums, get_unique_artists, get_unique_genres,
    import_folder, remove_track_from_playlist, reorder_playlist_tracks, search_tracks,
    set_current_track, update_multiple_tracks_metadata, update_track_metadata,
    update_track_metadata_with_file, validate_metadata_command,
};
use state::AppState;
use std::path::PathBuf;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            import_folder,
            get_all_tracks,
            search_tracks,
            filter_tracks,
            get_unique_artists,
            get_unique_albums,
            get_unique_genres,
            update_track_metadata,
            update_track_metadata_with_file,
            update_multiple_tracks_metadata,
            validate_metadata_command,
            create_playlist,
            get_playlists,
            add_track_to_playlist,
            remove_track_from_playlist,
            reorder_playlist_tracks,
            get_track_file_path,
            set_current_track,
            get_current_track
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
