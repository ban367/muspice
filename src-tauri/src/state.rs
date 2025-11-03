use rusqlite::Connection;
use std::sync::Mutex;

/// アプリケーション全体の状態を管理
pub struct AppState {
    /// データベース接続
    pub db: Mutex<Connection>,
    /// 現在再生中のトラックID
    pub current_track_id: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            current_track_id: Mutex::new(None),
        }
    }
}
