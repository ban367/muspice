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
use tauri::Manager;

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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
