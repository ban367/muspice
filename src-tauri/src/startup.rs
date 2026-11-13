//! 起動時の、ライブラリのデータの開き方と、起動の途中で起きた問題の知らせ（ADR-088）
//!
//! ライブラリのDBを開けない時に、アプリを落とさない。
//!
//! - **DBが壊れている・移行に失敗した**: DBを別の名前で残し（`muspice-unreadable-<日時>.db`）、空の
//!   ライブラリで起動する。画面で知らせ（`StartupNotice`）、利用者は、設定の「バックアップ」から、
//!   戻す世代を選んで復元する（黙ってデータを巻き戻さない）
//! - **新しい版のアプリが作ったDB**・**環境の問題（書き込めない・空き容量がない・ほかで使用中）**:
//!   DBには触らず、理由を書いたダイアログを出して終了する（`StartupFailure`）。空のライブラリで
//!   起動すると、利用者が気付かずに使い始め、後で元のDBと食い違うため
//!
//! 起動時に用意してあった復元（`backup::apply_staged_restore`）を当てられなかった時も、画面で知らせる。

use crate::db::{self, DbOpenError};
use crate::settings::Language;
use chrono::{DateTime, Local};
use rusqlite::{Connection, ErrorCode};
use serde::Serialize;
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// アプリデータの中の、DBのファイル名
pub const DB_FILE_NAME: &str = "muspice.db";

/// DBと一緒に動かす、SQLiteの一時ファイルの接尾辞
const DB_SIDECAR_SUFFIXES: [&str; 3] = ["-journal", "-wal", "-shm"];

/// 起動の途中で起きた、画面で知らせる問題（起動は続けた）
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartupNotice {
    /// ライブラリのDBを開けなかったため、別の名前で残して、空のライブラリで起動した
    DatabaseQuarantined {
        /// 残したDBの場所
        #[serde(rename = "quarantinedPath")]
        quarantined_path: String,
        /// アプリのバックアップのフォルダに、復元できるバックアップがあるか
        #[serde(rename = "hasBackups")]
        has_backups: bool,
    },
    /// 用意してあった復元（「復元して再起動」）を当てられなかった（ライブラリは復元の前のまま。
    /// 用意は残してあり、次の起動でもう一度試す）
    RestoreFailed {
        /// 失敗の内容（ログと同じ文）
        detail: String,
    },
}

/// 画面がまだ受け取っていない、起動時の知らせ（Tauriの管理状態）
#[derive(Default)]
pub struct StartupNotices(Mutex<Vec<StartupNotice>>);

impl StartupNotices {
    pub fn new(notices: Vec<StartupNotice>) -> Self {
        Self(Mutex::new(notices))
    }

    /// 預かっている知らせを取り出す（取り出した分は、手放す）
    pub fn take(&self) -> Vec<StartupNotice> {
        self.0
            .lock()
            .map(|mut notices| std::mem::take(&mut *notices))
            .unwrap_or_default()
    }
}

/// 起動を続けられない理由（DBには触っていない）
#[derive(Debug, PartialEq)]
pub enum StartupFailure {
    /// アプリのデータのフォルダを用意できない
    DataDir(String),
    /// DBを開けない（書き込めない・空き容量がない・ほかのプロセスが使用中など）。壊れたDBを
    /// 別の名前で残せなかった場合・新しいDBを作れなかった場合も含む
    Database(String),
    /// 新しい版のアプリが作ったDB
    NewerDatabase,
}

/// 開いたDB
pub struct OpenedDatabase {
    pub conn: Connection,
    /// 開けなかったDBを別の名前で残した場合、その場所（`conn`は、新しく作った空のDB）
    pub quarantined: Option<PathBuf>,
}

/// アプリデータのフォルダのDBを開く（なければ作る）
///
/// 壊れている・移行に失敗したDBは、別の名前で残して、新しいDBを作る。新しい版のDBと、環境の問題で
/// 開けないDBは、触らずに`StartupFailure`を返す。
pub fn open_database(app_data_dir: &Path) -> Result<OpenedDatabase, StartupFailure> {
    let db_path = app_data_dir.join(DB_FILE_NAME);
    let error = match db::init_db(db_path.clone()) {
        Ok(conn) => {
            return Ok(OpenedDatabase {
                conn,
                quarantined: None,
            });
        }
        Err(DbOpenError::NewerSchema { found, supported }) => {
            log::error!(
                "The database was written by a newer version of the app (schema version {}, supported up to {})",
                found,
                supported
            );
            return Err(StartupFailure::NewerDatabase);
        }
        Err(DbOpenError::Sqlite(e)) => e,
    };
    if is_environment_error(&error) {
        log::error!("Cannot open the database: {}", error);
        return Err(StartupFailure::Database(error.to_string()));
    }

    log::error!(
        "The database is damaged or could not be migrated; moving it aside: {}",
        error
    );
    let quarantined = quarantine_database(&db_path, Local::now()).map_err(|e| {
        log::error!("Cannot move the damaged database aside: {}", e);
        StartupFailure::Database(format!("{} ({})", error, e))
    })?;
    let conn = db::init_db(db_path).map_err(|e| {
        log::error!("Cannot create a new database: {}", e);
        StartupFailure::Database(e.to_string())
    })?;
    log::warn!(
        "Started with an empty library; the previous database is kept at {}",
        quarantined.display()
    );
    Ok(OpenedDatabase {
        conn,
        quarantined: Some(quarantined),
    })
}

/// DBの中身ではなく、環境が原因のエラーか（DBを別の名前で残しても直らない。DBには触らない）
fn is_environment_error(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(
            ErrorCode::DatabaseBusy
                | ErrorCode::DatabaseLocked
                | ErrorCode::CannotOpen
                | ErrorCode::PermissionDenied
                | ErrorCode::ReadOnly
                | ErrorCode::DiskFull
                | ErrorCode::SystemIoFailure
                | ErrorCode::OutOfMemory
                | ErrorCode::FileLockingProtocolFailed
                | ErrorCode::OperationInterrupted
        )
    )
}

/// 開けなかったDBを、同じフォルダの別の名前に変えて残す（SQLiteの一時ファイルも一緒に動かす）
///
/// 一時ファイル（ジャーナルなど）を元の名前のまま残すと、新しく作るDBに当てられてしまうため、
/// 動かせなければ消し、消せなければエラーにする。
/// @returns 残したDBの場所
fn quarantine_database(db_path: &Path, now: DateTime<Local>) -> std::io::Result<PathBuf> {
    let quarantined = db_path.with_file_name(format!(
        "muspice-unreadable-{}.db",
        now.format("%Y%m%d-%H%M%S")
    ));
    fs::rename(db_path, &quarantined)?;
    for suffix in DB_SIDECAR_SUFFIXES {
        let sidecar = with_suffix(db_path, suffix);
        if !sidecar.exists() {
            continue;
        }
        if fs::rename(&sidecar, with_suffix(&quarantined, suffix)).is_err() {
            fs::remove_file(&sidecar)?;
        }
    }
    Ok(quarantined)
}

/// ファイル名の後ろに接尾辞を付けたパス（`muspice.db` → `muspice.db-journal`）
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// 起動を続けられない理由をダイアログで知らせ、閉じたらアプリを終了する
///
/// `setup`を途中でやめる時に呼ぶ（この後の管理状態は置かれないため、画面のコマンドは動かない）。
/// 画面（WebView）には何も読み込ませず（空のライブラリを見せない）、ダイアログは、メインウィンドウに
/// 付くシートとして出す。
///
/// - ダイアログの答えは待たない（`setup`の中で待つと、ダイアログを出すメインスレッドが止まる）
/// - 親のウィンドウを決めて出す。親を決めないと、ダイアログのライブラリ（rfd）が、アプリのウィンドウの
///   1つ（閉じている途中のウィンドウ・表示していないウィンドウを含む）に付くシートとして出し、
///   表示されないまま、答えも返らないことがある
pub fn report_failure<R: Runtime>(
    app: &AppHandle<R>,
    failure: &StartupFailure,
    app_data_dir: Option<&Path>,
    language: Language,
) {
    log::error!("Cannot start the app: {:?}", failure);
    let log_dir = app.path().app_log_dir().ok();
    let (title, message) = failure_message(failure, app_data_dir, log_dir.as_deref(), language);

    let mut dialog = app
        .dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error);
    if let Some(window) = app.get_webview_window(crate::tray::MAIN_WINDOW) {
        match "about:blank".parse() {
            Ok(blank) => {
                if let Err(e) = window.navigate(blank) {
                    log::warn!("Cannot clear the main window: {}", e);
                }
            }
            Err(e) => log::warn!("Cannot clear the main window: {}", e),
        }
        dialog = dialog.parent(&window);
    }
    let handle = app.clone();
    dialog.show(move |_| handle.exit(1));
}

/// 起動を続けられない理由の、ダイアログの題と文
fn failure_message(
    failure: &StartupFailure,
    app_data_dir: Option<&Path>,
    log_dir: Option<&Path>,
    language: Language,
) -> (&'static str, String) {
    let (title, body) = match (language, failure) {
        (Language::Ja, StartupFailure::NewerDatabase) => (
            "Muspice を起動できません",
            "ライブラリのデータは、新しい版の Muspice で作られています。この版では開けません。\n\
             Muspice を新しい版に更新してください。データは変更していません。"
                .to_string(),
        ),
        (Language::En, StartupFailure::NewerDatabase) => (
            "Muspice cannot start",
            "The library data was created by a newer version of Muspice and cannot be opened by \
             this version.\nPlease update Muspice. The data has not been changed."
                .to_string(),
        ),
        (Language::Ja, StartupFailure::Database(detail)) => (
            "Muspice を起動できません",
            format!(
                "ライブラリのデータを開けませんでした。\n\
                 ディスクの空き容量と、データのフォルダへの書き込みの権限、ほかに Muspice が\
                 起動していないかを確かめてから、もう一度起動してください。データは変更していません。\n\n\
                 詳細: {detail}"
            ),
        ),
        (Language::En, StartupFailure::Database(detail)) => (
            "Muspice cannot start",
            format!(
                "The library data could not be opened.\n\
                 Check the free disk space, the write permission of the data folder, and that no \
                 other Muspice is running, then start Muspice again. The data has not been \
                 changed.\n\nDetails: {detail}"
            ),
        ),
        (Language::Ja, StartupFailure::DataDir(detail)) => (
            "Muspice を起動できません",
            format!(
                "Muspice のデータのフォルダを用意できませんでした。\n\
                 フォルダへの書き込みの権限とディスクの空き容量を確かめてから、もう一度起動してください。\n\n\
                 詳細: {detail}"
            ),
        ),
        (Language::En, StartupFailure::DataDir(detail)) => (
            "Muspice cannot start",
            format!(
                "The data folder of Muspice could not be prepared.\n\
                 Check the write permission of the folder and the free disk space, then start \
                 Muspice again.\n\nDetails: {detail}"
            ),
        ),
    };

    let (data_label, log_label) = match language {
        Language::Ja => ("データの場所", "ログの場所"),
        Language::En => ("Data folder", "Log folder"),
    };
    let mut message = body;
    if let Some(dir) = app_data_dir {
        message.push_str(&format!("\n\n{data_label}: {}", dir.display()));
    }
    if let Some(dir) = log_dir {
        let separator = if app_data_dir.is_some() { "\n" } else { "\n\n" };
        message.push_str(&format!("{separator}{log_label}: {}", dir.display()));
    }
    (title, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    fn track_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .unwrap()
    }

    /// DBがなければ作り、あればそのまま開く
    #[test]
    fn test_opens_or_creates_the_database() {
        let dir = TempDir::new();
        let opened = open_database(dir.path()).unwrap();
        assert!(opened.quarantined.is_none());
        opened
            .conn
            .execute(
                "INSERT INTO tracks (id, file_path, file_name, format, file_size)
                 VALUES ('t1', '/m/a.mp3', 'a.mp3', 'MP3', 1)",
                [],
            )
            .unwrap();
        drop(opened);

        let opened = open_database(dir.path()).unwrap();
        assert!(opened.quarantined.is_none());
        assert_eq!(track_count(&opened.conn), 1);
    }

    /// 壊れたDBは、一時ファイルと一緒に別の名前で残し、空のDBを作って開く
    #[test]
    fn test_damaged_database_is_moved_aside() {
        let dir = TempDir::new();
        let db_path = dir.path().join(DB_FILE_NAME);
        fs::write(
            &db_path,
            b"this is not a database, but long enough to be read as one",
        )
        .unwrap();
        fs::write(with_suffix(&db_path, "-journal"), b"stale journal").unwrap();

        let opened = open_database(dir.path()).unwrap();

        let quarantined = opened.quarantined.expect("the damaged file should be kept");
        assert!(quarantined.is_file());
        assert_eq!(quarantined.parent(), Some(dir.path()));
        let name = quarantined.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("muspice-unreadable-") && name.ends_with(".db"));
        assert!(fs::read(&quarantined).unwrap().starts_with(b"this is not"));
        // 前のDBのジャーナルは、新しいDBの名前のままでは残さない
        assert!(!with_suffix(&db_path, "-journal").exists());
        // 新しいDBは空で、使える
        assert_eq!(track_count(&opened.conn), 0);
        assert!(db_path.is_file());
    }

    /// DBを別の名前に変える時は、SQLiteの一時ファイルも一緒に動かす
    #[test]
    fn test_quarantine_moves_sidecar_files() {
        let dir = TempDir::new();
        let db_path = dir.path().join(DB_FILE_NAME);
        fs::write(&db_path, b"db").unwrap();
        fs::write(with_suffix(&db_path, "-journal"), b"journal").unwrap();
        fs::write(with_suffix(&db_path, "-wal"), b"wal").unwrap();
        let now = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 11, 13, 9, 30, 0).unwrap();

        let quarantined = quarantine_database(&db_path, now).unwrap();

        assert_eq!(
            quarantined,
            dir.path().join("muspice-unreadable-20261113-093000.db")
        );
        assert_eq!(fs::read(&quarantined).unwrap(), b"db");
        assert_eq!(
            fs::read(with_suffix(&quarantined, "-journal")).unwrap(),
            b"journal"
        );
        assert_eq!(fs::read(with_suffix(&quarantined, "-wal")).unwrap(), b"wal");
        for suffix in ["", "-journal", "-wal", "-shm"] {
            assert!(!with_suffix(&db_path, suffix).exists());
        }
    }

    /// 移行に失敗するDB（表の形が想定と違う）も、別の名前で残す
    #[test]
    fn test_database_that_cannot_be_migrated_is_moved_aside() {
        let dir = TempDir::new();
        let db_path = dir.path().join(DB_FILE_NAME);
        let conn = Connection::open(&db_path).unwrap();
        // `tracks`が、表ではなくビューになっている（列を足す移行が失敗する）
        conn.execute_batch("CREATE VIEW tracks AS SELECT 1 AS id")
            .unwrap();
        drop(conn);

        let opened = open_database(dir.path()).unwrap();

        assert!(opened.quarantined.is_some_and(|path| path.is_file()));
        assert_eq!(track_count(&opened.conn), 0);
    }

    /// 新しい版のアプリが作ったDBは、触らずに、起動を続けられない理由として返す
    #[test]
    fn test_newer_database_is_left_untouched() {
        let dir = TempDir::new();
        let opened = open_database(dir.path()).unwrap();
        opened
            .conn
            .execute_batch(&format!(
                "UPDATE app_meta SET value = '{}' WHERE key = 'schema_version'",
                db::DB_SCHEMA_VERSION + 1
            ))
            .unwrap();
        drop(opened);
        let before = fs::read(dir.path().join(DB_FILE_NAME)).unwrap();

        let result = open_database(dir.path());

        assert!(matches!(result, Err(StartupFailure::NewerDatabase)));
        assert_eq!(fs::read(dir.path().join(DB_FILE_NAME)).unwrap(), before);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from(DB_FILE_NAME)]);
    }

    /// 環境の問題（DBの場所を開けない）は、何も動かさずに、起動を続けられない理由として返す
    #[test]
    fn test_unopenable_location_is_a_failure() {
        let dir = TempDir::new();
        // DBのファイル名の場所に、フォルダがある（開けない）
        fs::create_dir(dir.path().join(DB_FILE_NAME)).unwrap();

        let result = open_database(dir.path());

        assert!(matches!(result, Err(StartupFailure::Database(_))));
        assert!(dir.path().join(DB_FILE_NAME).is_dir());
    }

    /// 環境が原因のエラーと、DBの中身が原因のエラーを見分ける
    #[test]
    fn test_is_environment_error() {
        let failure =
            |code: i32| rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None);
        assert!(is_environment_error(&failure(rusqlite::ffi::SQLITE_BUSY)));
        assert!(is_environment_error(&failure(rusqlite::ffi::SQLITE_FULL)));
        assert!(is_environment_error(&failure(
            rusqlite::ffi::SQLITE_READONLY
        )));
        assert!(is_environment_error(&failure(
            rusqlite::ffi::SQLITE_CANTOPEN
        )));
        assert!(!is_environment_error(&failure(
            rusqlite::ffi::SQLITE_CORRUPT
        )));
        assert!(!is_environment_error(&failure(
            rusqlite::ffi::SQLITE_NOTADB
        )));
        assert!(!is_environment_error(&failure(rusqlite::ffi::SQLITE_ERROR)));
        assert!(!is_environment_error(&rusqlite::Error::QueryReturnedNoRows));
    }

    /// ダイアログの文に、理由と、データ・ログの場所を入れる
    #[test]
    fn test_failure_message() {
        let (title, message) = failure_message(
            &StartupFailure::Database("disk full".to_string()),
            Some(Path::new("/data")),
            Some(Path::new("/logs")),
            Language::Ja,
        );
        assert_eq!(title, "Muspice を起動できません");
        assert!(message.contains("詳細: disk full"));
        assert!(message.ends_with("データの場所: /data\nログの場所: /logs"));

        let (title, message) =
            failure_message(&StartupFailure::NewerDatabase, None, None, Language::En);
        assert_eq!(title, "Muspice cannot start");
        assert!(message.contains("newer version"));
        assert!(!message.contains("Data folder"));
    }

    /// 預かった知らせは、1回だけ取り出せる
    #[test]
    fn test_notices_are_taken_once() {
        let notices = StartupNotices::new(vec![StartupNotice::RestoreFailed {
            detail: "x".to_string(),
        }]);
        assert_eq!(notices.take().len(), 1);
        assert!(notices.take().is_empty());
    }
}
