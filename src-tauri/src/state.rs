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

    /// DBロックを取得してクロージャを実行する共通ヘルパー
    ///
    /// ロック取得失敗時のエラーメッセージ生成を一元化する。
    /// トランザクションが必要な場合のため`&mut Connection`を渡す
    /// （読み取りのみの場合は`&Connection`として自動的に扱える）。
    pub fn with_db<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut db = self
            .db
            .lock()
            .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;
        f(&mut db)
    }
}
