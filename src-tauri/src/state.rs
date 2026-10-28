use crate::album_art::AlbumArtCache;
use crate::error::AppResult;
use crate::smart_playlist::EvalContext;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
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
    /// 曲のファイルを書き換える処理を、曲ごとに1つずつ行うためのロック（`lock_track_file`）
    track_file_locks: TrackFileLocks,
    /// 自動プレイリストの、ランダムな並びの種（起動のたびに変わる。「選び直す」でも変える）
    shuffle_seed: AtomicU64,
}

/// 曲ごとの、ファイルを書き換えている印
///
/// タグの書き込みは「ファイルを読んで → 全体を書き戻す」ため、同じ曲へ2つの処理（ラウドネスの解析と
/// タグの編集など）が同時に書くと、後に書いた方の内容だけが残る。書き換える処理は、この印を取ってから
/// ファイルを読み、書き終えてから外す（同じ曲の処理は、前の処理が終わるのを待つ）。
#[derive(Default)]
struct TrackFileLocks {
    busy: Mutex<HashSet<String>>,
    released: Condvar,
}

/// 曲のファイルを書き換えている間、持つ印（落とすと、待っている処理が続ける）
pub struct TrackFileGuard<'a> {
    locks: &'a TrackFileLocks,
    track_id: String,
}

impl Drop for TrackFileGuard<'_> {
    fn drop(&mut self) {
        let mut busy = self
            .locks
            .busy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        busy.remove(&self.track_id);
        self.locks.released.notify_all();
    }
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
            track_file_locks: TrackFileLocks::default(),
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

    /// 曲のファイルを書き換える前に取る（同じ曲を書き換えている処理があれば、終わるまで待つ）
    ///
    /// ファイルのパスは、取った後にDBから読む（待っている間に、ファイルの整理などで場所が変わるため）。
    /// 一度に取るのは1曲だけにする（2曲を持ったまま待つと、逆の順で取る処理と互いに待って止まる）。
    /// ロックの順序は「スキャン（`lock_library_scan`） → 曲 → DB」で、DBのロックを持ったまま取らない。
    /// 守るデータを持たないため、途中でパニックしたスレッドがあっても続ける。
    pub fn lock_track_file(&self, track_id: &str) -> TrackFileGuard<'_> {
        let locks = &self.track_file_locks;
        let mut busy = locks
            .busy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while busy.contains(track_id) {
            busy = locks
                .released
                .wait(busy)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        busy.insert(track_id.to_string());
        TrackFileGuard {
            locks,
            track_id: track_id.to_string(),
        }
    }

    /// DBロックを取得してクロージャを実行する共通ヘルパー
    ///
    /// トランザクションが必要な場合のため`&mut Connection`を渡す
    /// （読み取りのみの場合は`&Connection`として自動的に扱える）。
    ///
    /// ほかの処理がDBを使っている途中でパニックした後も、ロックを回復して続ける（回復しないと、
    /// 再起動するまですべてのDBの操作が失敗する）。途中で止まった処理のトランザクションは、
    /// `Transaction`を捨てた時に取り消されるが、残っていれば取り消してから使う（ログに残す）。
    pub fn with_db<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut db = self.db.lock().unwrap_or_else(|poisoned| {
            log::error!("A database operation panicked; recovering the lock and continuing");
            let db = poisoned.into_inner();
            if !db.is_autocommit()
                && let Err(e) = db.execute_batch("ROLLBACK")
            {
                log::error!(
                    "Failed to roll back the transaction left by the panicked operation: {}",
                    e
                );
            }
            self.db.clear_poison();
            db
        });
        f(&mut db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn state() -> Arc<AppState> {
        Arc::new(AppState::new(Connection::open_in_memory().unwrap()))
    }

    /// DBを使う処理がパニックした後も、DBを使い続けられる（途中のトランザクションは取り消す）
    #[test]
    fn test_with_db_recovers_after_a_panic() {
        let state = state();
        state
            .with_db(|db| {
                db.execute_batch("CREATE TABLE items (id INTEGER)").unwrap();
                Ok(())
            })
            .unwrap();

        let panicked = {
            let state = state.clone();
            thread::spawn(move || {
                let _: AppResult<()> = state.with_db(|db| {
                    db.execute_batch("BEGIN; INSERT INTO items VALUES (1);")
                        .unwrap();
                    panic!("処理の途中で止まる");
                });
            })
            .join()
        };
        assert!(panicked.is_err());

        let (count, autocommit) = state
            .with_db(|db| {
                let count: i64 = db
                    .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                    .unwrap();
                Ok((count, db.is_autocommit()))
            })
            .expect("パニックの後もDBを使える");
        assert_eq!((count, autocommit), (0, true));
        assert!(!state.db.is_poisoned());
    }

    #[test]
    fn test_track_file_lock_makes_writers_of_the_same_track_wait() {
        let state = state();
        let first = state.lock_track_file("t1");

        let (sender, received) = mpsc::channel();
        let waiting = {
            let state = state.clone();
            thread::spawn(move || {
                let _second = state.lock_track_file("t1");
                sender.send(()).unwrap();
            })
        };
        // 同じ曲は、前の処理が印を外すまで待つ
        assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        waiting.join().unwrap();
    }

    #[test]
    fn test_track_file_lock_does_not_block_other_tracks() {
        let state = state();
        let _first = state.lock_track_file("t1");

        let (sender, received) = mpsc::channel();
        let other = {
            let state = state.clone();
            thread::spawn(move || {
                let _second = state.lock_track_file("t2");
                sender.send(()).unwrap();
            })
        };
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        other.join().unwrap();
        // 外した印は、もう一度取れる
        drop(_first);
        let _again = state.lock_track_file("t1");
    }
}
