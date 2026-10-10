use crate::album_art::AlbumArtCache;
use crate::error::{AppError, AppResult};
use crate::smart_playlist::EvalContext;
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
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
    /// 抽出済みアルバムアートのキャッシュ（`albumart`プロトコルが使う）
    pub album_art_cache: Mutex<AlbumArtCache>,
    /// ライブラリフォルダの読み込み（インポート・再スキャン）を1つずつ行うためのロック
    ///
    /// 手動の操作と自動の再スキャンが同じファイルを同時に登録しないようにする。
    /// `lock_library_scan`で取得する。
    library_scan_lock: Mutex<()>,
    /// 自動プレイリストの、ランダムな並びの種（起動のたびに変わる。「選び直す」でも変える）
    shuffle_seed: AtomicU64,
}

/// ランダムな並びの、新しい種
fn new_shuffle_seed() -> u64 {
    // 並びを変えるためだけに使う（予測できないことは求めない）
    uuid::Uuid::new_v4().as_u64_pair().0
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            current_track_id: Mutex::new(None),
            album_art_limiter: Semaphore::new(ALBUM_ART_CONCURRENCY),
            album_art_cache: Mutex::new(AlbumArtCache::default()),
            library_scan_lock: Mutex::new(()),
            shuffle_seed: AtomicU64::new(new_shuffle_seed()),
        }
    }

    /// 自動プレイリストの条件から曲を求める時の状況（現在の日時と、ランダムな並びの種）
    pub fn smart_playlist_context(&self) -> EvalContext {
        EvalContext {
            now: chrono::Utc::now(),
            shuffle_seed: self.shuffle_seed.load(Ordering::Relaxed),
        }
    }

    /// ランダムな並びの自動プレイリストを、選び直す（種を変える）
    pub fn reshuffle_smart_playlists(&self) {
        self.shuffle_seed
            .store(new_shuffle_seed(), Ordering::Relaxed);
    }

    /// ライブラリフォルダの読み込みのロックを取得する（他の読み込みが終わるまで待つ）
    ///
    /// DBロックより先に取得する（逆の順序で取得しない）。
    /// 守るデータを持たないため、途中でパニックしたスレッドがあっても続ける。
    pub fn lock_library_scan(&self) -> MutexGuard<'_, ()> {
        self.library_scan_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
