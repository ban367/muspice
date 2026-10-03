//! ライブラリフォルダの変更の自動反映
//!
//! 設定に応じて、次のタイミングでライブラリフォルダを再スキャンする（手動の再スキャンと同じ処理）。
//!
//! - アプリの起動時（自動反映のどちらかが有効な場合。起動直後の表示を妨げないよう少し待つ）
//! - 一定の間隔ごと（`library_scan_interval_minutes`）
//! - フォルダ内のファイルの変更の通知を受けたとき（`watch_library_folders`）。
//!   短時間に続いた変更はまとめて、変更のあったフォルダだけを再スキャンする
//!
//! フォルダが見つからない場合（外付けドライブが外れているなど）は何もしない。
//! ライブラリが変わった場合は、再スキャンの処理が`LibraryChanged`イベントを送る。

use crate::commands::rescan_folder;
use crate::library::is_supported_audio_file;
use crate::library_folder::{FolderRecord, find_all_folders, track_path_prefix};
use crate::repository::count_tracks_under;
use crate::settings::{Settings, SettingsState};
use crate::state::AppState;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// フォルダの変更の通知をまとめる時間（この間に続いた変更は1回の再スキャンにする）
const WATCH_DEBOUNCE: Duration = Duration::from_secs(3);

/// 起動時の再スキャンを始めるまでの待ち時間
const STARTUP_SCAN_DELAY: Duration = Duration::from_secs(5);

/// ライブラリフォルダの自動反映（Tauriの状態として管理する）
#[derive(Default)]
pub struct LibrarySync {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// 定期的な再スキャンの間隔（分。0は再スキャンしない）
    interval_minutes: u32,
    /// 定期的な再スキャンのスレッドへの送信側（落とすとスレッドが止まる）
    timer: Option<mpsc::Sender<()>>,
    /// 設定でフォルダの監視が有効か
    watch_enabled: bool,
    /// フォルダの監視（落とすと止まる。有効でも、作れなかった場合はない）
    watcher: Option<Debouncer<RecommendedWatcher>>,
    /// 監視しているフォルダ
    watched: HashSet<PathBuf>,
}

impl LibrarySync {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // 守るのは監視・タイマーの持ち主だけのため、途中でパニックしたスレッドがあっても続ける
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 保存されている設定を反映する（定期的な再スキャンとフォルダの監視を、必要に応じて開始・停止する）
    ///
    /// 設定は呼んだ時点の保存済みの値を読む（設定の保存が続き、反映の順序が入れ替わっても、
    /// 最後に保存した設定になる）。監視を始めるときは、フォルダの数によって時間がかかるため、
    /// 非同期処理の中では呼ばない。
    pub fn apply_saved_settings(&self, app: &AppHandle) {
        let mut inner = self.lock();
        let settings = match app.state::<SettingsState>().get() {
            Ok(settings) => settings,
            Err(e) => {
                log::error!(
                    "設定を読み込めないため、ライブラリフォルダの自動反映を変えません: {}",
                    e
                );
                return;
            }
        };
        apply_settings(app, &mut inner, &settings);
    }

    /// 監視するフォルダを、記録しているライブラリフォルダに合わせる
    ///
    /// フォルダの追加・削除の後と、定期的な再スキャンのたびに呼ぶ（外れていた外付けドライブが
    /// つながった場合など、監視を始めた時点で見つからなかったフォルダも監視する）。
    /// 監視を作れていなければ作り直す。時間がかかることがあるため、非同期処理の中では呼ばない。
    pub fn refresh_watches(&self, app: &AppHandle) {
        let mut inner = self.lock();
        if inner.watch_enabled {
            ensure_watching(app, &mut inner);
        }
    }
}

/// 設定を反映する（`LibrarySync::apply_saved_settings`の本体）
fn apply_settings(app: &AppHandle, inner: &mut Inner, settings: &Settings) {
    if inner.interval_minutes != settings.library_scan_interval_minutes {
        // 前のスレッドは送信側を落とすと止まる
        inner.timer = None;
        inner.interval_minutes = settings.library_scan_interval_minutes;
        if inner.interval_minutes > 0 {
            let interval = Duration::from_secs(u64::from(inner.interval_minutes) * 60);
            inner.timer = Some(spawn_timer(app.clone(), interval));
            log::info!(
                "ライブラリフォルダを{}分ごとに再スキャンします",
                inner.interval_minutes
            );
        }
    }

    inner.watch_enabled = settings.watch_library_folders;
    if inner.watch_enabled {
        ensure_watching(app, inner);
    } else if inner.watcher.take().is_some() {
        inner.watched.clear();
        log::info!("ライブラリフォルダの監視を止めました");
    }
}

/// 監視を（なければ作って）記録しているライブラリフォルダに合わせる
///
/// 監視を作れなかった場合（OSの監視の数の上限など）は、次に呼ばれたとき（フォルダの追加・削除、
/// 定期的な再スキャン、設定の保存）に作り直す。
fn ensure_watching(app: &AppHandle, inner: &mut Inner) {
    if inner.watcher.is_none() {
        inner.watcher = create_watcher(app);
        inner.watched.clear();
    }
    update_watches(app, inner);
}

/// アプリの起動時に呼ぶ: 設定を反映し、自動反映が有効なら全フォルダを再スキャンする
///
/// 起動を遅らせないよう、別スレッドで行う。
pub fn start(app: &AppHandle) {
    let app = app.clone();
    spawn_named("library-sync-start", move || {
        let settings = match app.state::<SettingsState>().get() {
            Ok(settings) => settings,
            Err(e) => {
                log::error!(
                    "設定を読み込めないため、ライブラリフォルダの自動反映を始めません: {}",
                    e
                );
                return;
            }
        };
        app.state::<LibrarySync>().apply_saved_settings(&app);
        if settings.auto_sync_enabled() {
            thread::sleep(STARTUP_SCAN_DELAY);
            rescan_all_folders(&app, "起動時");
        }
    });
}

/// 名前付きのスレッドを起動する（失敗はログに残す）
fn spawn_named(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = thread::Builder::new().name(name.to_string()).spawn(f) {
        log::error!("スレッド（{}）を起動できませんでした: {}", name, e);
    }
}

/// 一定の間隔で全フォルダを再スキャンするスレッドを起動する
///
/// 返す送信側を落とす（設定が変わる）と、待っている間にスレッドが止まる。
fn spawn_timer(app: AppHandle, interval: Duration) -> mpsc::Sender<()> {
    let (stop, stopped) = mpsc::channel::<()>();
    spawn_named("library-scan-timer", move || {
        while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
            app.state::<LibrarySync>().refresh_watches(&app);
            rescan_all_folders(&app, "定期");
        }
    });
    stop
}

/// フォルダの監視を作る（変更の通知はまとめてから、変更のあったフォルダを再スキャンする）
fn create_watcher(app: &AppHandle) -> Option<Debouncer<RecommendedWatcher>> {
    let ignored = app_directories(app);
    let handler_app = app.clone();
    let handler = move |result: DebounceEventResult| match result {
        Ok(events) => {
            let has_tracks_under = |path: &Path| has_tracks_under(&handler_app, path);
            let paths: Vec<PathBuf> = events
                .into_iter()
                .map(|event| event.path)
                .filter(|path| is_relevant_change(path, &ignored, has_tracks_under))
                .collect();
            if !paths.is_empty() {
                rescan_folders(&handler_app, "変更の通知", |folder| {
                    contains_any(folder, &paths)
                });
            }
        }
        Err(e) => log::warn!("フォルダの監視でエラーが発生しました: {}", e),
    };

    match new_debouncer(WATCH_DEBOUNCE, handler) {
        Ok(watcher) => {
            log::info!("ライブラリフォルダの監視を始めました");
            Some(watcher)
        }
        Err(e) => {
            log::error!("フォルダの監視を始められませんでした: {}", e);
            None
        }
    }
}

/// 監視するフォルダを、記録しているライブラリフォルダ（今見つかるもの）に合わせる
fn update_watches(app: &AppHandle, inner: &mut Inner) {
    let Some(watcher) = inner.watcher.as_mut() else {
        return;
    };
    let folders = match app.state::<AppState>().with_db(|db| find_all_folders(db)) {
        Ok(folders) => folders,
        Err(e) => {
            log::error!("監視するライブラリフォルダを取得できませんでした: {}", e);
            return;
        }
    };
    let wanted: HashSet<PathBuf> = folders
        .into_iter()
        .map(|folder| PathBuf::from(folder.path))
        .filter(|path| path.is_dir())
        .collect();

    for path in inner.watched.difference(&wanted) {
        // 外れたドライブなどは、OSの側で監視が終わっていることがある
        if let Err(e) = watcher.watcher().unwatch(path) {
            log::debug!(
                "フォルダの監視を解除できませんでした: {}: {}",
                path.display(),
                e
            );
        }
    }
    let mut watched = HashSet::new();
    for path in wanted {
        if inner.watched.contains(&path) {
            watched.insert(path);
            continue;
        }
        match watcher.watcher().watch(&path, RecursiveMode::Recursive) {
            Ok(()) => {
                log::info!("フォルダを監視します: {}", path.display());
                watched.insert(path);
            }
            Err(e) => log::warn!("フォルダを監視できませんでした: {}: {}", path.display(), e),
        }
    }
    inner.watched = watched;
}

/// アプリのデータ（DB・ログなど）のフォルダ
///
/// ライブラリフォルダの中にあると、再スキャンでの書き込みが変更の通知になり、
/// 再スキャンを繰り返してしまうため、これらの中の変更は無視する。
fn app_directories(app: &AppHandle) -> Vec<PathBuf> {
    let path = app.path();
    [
        path.app_data_dir(),
        path.app_local_data_dir(),
        path.app_config_dir(),
        path.app_cache_dir(),
        path.app_log_dir(),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .collect()
}

/// ライブラリに影響しうる変更か
///
/// - 音楽ファイル（追加・変更・削除）
/// - 今あるフォルダ（移動・コピーされてきたフォルダ）
/// - なくなったパスのうち、ライブラリの曲を含むもの（フォルダごと消された・移された場合）
///
/// なくなった一時ファイル・画像などで再スキャン（フォルダ全体の走査）をしないよう、
/// なくなったパスは曲を含む場合だけにする。
fn is_relevant_change(
    path: &Path,
    ignored: &[PathBuf],
    has_tracks_under: impl Fn(&Path) -> bool,
) -> bool {
    if ignored.iter().any(|dir| path.starts_with(dir)) {
        return false;
    }
    if is_supported_audio_file(path) {
        return true;
    }
    if path.exists() {
        return path.is_dir();
    }
    has_tracks_under(path)
}

/// パスの下にライブラリの曲があるか（なくなったフォルダの判定に使う。調べられない場合はtrue）
fn has_tracks_under(app: &AppHandle, path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return true;
    };
    app.state::<AppState>()
        .with_db(|db| count_tracks_under(db, &track_path_prefix(path)))
        .map_or(true, |count| count > 0)
}

/// フォルダが、パスのいずれかを含むか
fn contains_any(folder: &FolderRecord, paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| path.starts_with(&folder.path))
}

/// すべてのライブラリフォルダを再スキャンする
fn rescan_all_folders(app: &AppHandle, reason: &str) {
    rescan_folders(app, reason, |_| true);
}

/// 条件に合うライブラリフォルダを再スキャンする
fn rescan_folders(app: &AppHandle, reason: &str, filter: impl Fn(&FolderRecord) -> bool) {
    let folders = match app.state::<AppState>().with_db(|db| find_all_folders(db)) {
        Ok(folders) => folders,
        Err(e) => {
            log::error!("ライブラリフォルダを取得できませんでした: {}", e);
            return;
        }
    };
    for folder in folders.iter().filter(|folder| filter(folder)) {
        rescan_quietly(app, folder, reason);
    }
}

/// フォルダを再スキャンする（見つからないフォルダは飛ばし、失敗はログに残す）
fn rescan_quietly(app: &AppHandle, folder: &FolderRecord, reason: &str) {
    if !Path::new(&folder.path).is_dir() {
        log::info!(
            "フォルダが見つからないため再スキャンしませんでした（{}）: {}",
            reason,
            folder.path
        );
        return;
    }
    let state = app.state::<AppState>();
    if let Err(e) = rescan_folder(&folder.id, state.inner(), app, false) {
        log::warn!(
            "ライブラリフォルダを自動で再スキャンできませんでした（{}）: {}: {}",
            reason,
            folder.path,
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn folder(path: &str) -> FolderRecord {
        FolderRecord {
            id: path.to_string(),
            path: path.to_string(),
            added_at: String::new(),
            last_scanned_at: None,
        }
    }

    #[test]
    fn test_contains_any_matches_whole_components() {
        let paths = [
            PathBuf::from("/music/rock/album/01.mp3"),
            // 名前の先頭が同じだけのフォルダは含まない
            PathBuf::from("/music/jazz-live/01.mp3"),
        ];

        assert!(contains_any(&folder("/music/rock"), &paths));
        assert!(!contains_any(&folder("/music/jazz"), &paths));
    }

    #[test]
    fn test_is_relevant_change() {
        let dir = std::env::temp_dir().join(format!("muspice-sync-test-{}", std::process::id()));
        let app_data = dir.join("app-data");
        fs::create_dir_all(&app_data).unwrap();
        let cover = dir.join("cover.jpg");
        fs::write(&cover, b"").unwrap();
        let song = dir.join("01.flac");
        fs::write(&song, b"").unwrap();
        let ignored = [app_data.clone()];
        // ライブラリの曲を含むパス（なくなったフォルダの判定に使う）
        let removed_album = dir.join("removed-album");
        let has_tracks_under = |path: &Path| path == removed_album;
        let relevant = |path: &Path| is_relevant_change(path, &ignored, has_tracks_under);

        // 音楽ファイル（なくなったものも）・今あるフォルダは含める
        assert!(relevant(&song));
        assert!(relevant(&dir.join("removed.mp3")));
        assert!(relevant(&dir));
        // なくなったパスは、ライブラリの曲を含む場合だけ含める（一時ファイルでは再スキャンしない）
        assert!(relevant(&removed_album));
        assert!(!relevant(&dir.join("download.part")));
        assert!(!relevant(&dir.join("removed-empty-folder")));
        // 音楽ファイル以外の、今あるファイルは含めない
        assert!(!relevant(&cover));
        // アプリのデータの中の変更は含めない（DBの書き込みで再スキャンを繰り返さない）
        assert!(!relevant(&app_data.join("muspice.db-journal")));
        assert!(!relevant(&app_data));

        fs::remove_dir_all(&dir).ok();
    }
}
