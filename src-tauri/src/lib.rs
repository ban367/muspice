mod db;
mod models;
mod state;

use state::AppState;
use std::path::PathBuf;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // アプリケーションデータディレクトリを取得
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("アプリケーションデータディレクトリの取得に失敗しました");

            // ディレクトリが存在しない場合は作成
            std::fs::create_dir_all(&app_data_dir)
                .expect("アプリケーションデータディレクトリの作成に失敗しました");

            // データベースファイルのパスを設定
            let db_path: PathBuf = app_data_dir.join("muspice.db");

            // データベースを初期化
            let conn = db::init_db(db_path).expect("データベースの初期化に失敗しました");

            // アプリケーション状態を作成して管理
            let app_state = AppState::new(conn);
            app.manage(app_state);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
