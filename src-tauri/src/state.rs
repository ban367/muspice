use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::sync::Mutex;
use tokio::sync::Semaphore;

/// アルバムアート抽出の同時実行数の上限
///
/// 一覧表示では表示中のカードの数だけ要求が同時に届くため、
/// ファイル読み取りが一斉に走ってディスクを奪い合わないよう制限する。
const ALBUM_ART_CONCURRENCY: usize = 4;

/// アプリケーション全体の状態を管理
pub struct AppState {
    /// データベース接続
    pub db: Mutex<Connection>,
    /// 現在再生中のトラックID
    pub current_track_id: Mutex<Option<String>>,
    /// アルバムアート抽出の同時実行数を制限するセマフォ
    pub album_art_limiter: Semaphore,
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            current_track_id: Mutex::new(None),
            album_art_limiter: Semaphore::new(ALBUM_ART_CONCURRENCY),
        }
    }

    /// DBロックを取得してクロージャを実行する共通ヘルパー
    ///
    /// ロック取得失敗時のエラーメッセージ生成を一元化する。
    /// トランザクションが必要な場合のため`&mut Connection`を渡す
    /// （読み取りのみの場合は`&Connection`として自動的に扱える）。
    pub fn with_db<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut db = self.db.lock().map_err(|e| {
            AppError::Lock(format!("データベースロックの取得に失敗しました: {}", e))
        })?;
        f(&mut db)
    }
}
