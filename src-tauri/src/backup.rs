//! ライブラリのデータ（DB・設定）のバックアップと復元（ADR-073）
//!
//! - バックアップは、DBを`VACUUM INTO`で写したSQLiteのファイル（拡張子`.muspicebackup`）。写した後に、
//!   `backup_meta`の表へ、形式の版・作った日時・アプリの版・設定（`settings.json`の内容）を書く。
//!   お気に入り・再生回数・再生履歴・プレイリスト・追加日など、DBにだけある情報を含む（曲のファイルは含まない）
//! - 復元は、使用中のDBを置き換えずに、アプリデータの`restore/`へDBと設定を用意してからアプリを再起動し、
//!   次の起動でDBを開く前に置き換える（`apply_staged_restore`）。用意する前に、今のDBと設定を
//!   バックアップのフォルダ（`backups/`）へ退避する
//! - 自動のバックアップは、`backups/`へ1日に1回（起動の後と、起動している間の1時間ごとに確かめる）作り、
//!   新しい世代だけを残す（`start`）

use crate::error::{AppError, AppResult};
use crate::settings::SettingsState;
use crate::state::AppState;
use chrono::{Local, Utc};
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// バックアップのファイルの拡張子
pub const BACKUP_EXTENSION: &str = "muspicebackup";

/// バックアップの形式の版（`backup_meta`の`format_version`）
///
/// 表の作りを変えて、前の版のアプリで復元できなくなる時に上げる（新しい版のバックアップは復元しない）。
const FORMAT_VERSION: i64 = 1;

/// バックアップに足す、情報の表
const META_TABLE: &str = "backup_meta";

/// 自動のバックアップを残す世代の数（設定の画面の説明と共有する）
pub const AUTO_BACKUP_KEEP: u32 = 7;

/// 復元する前に退避したバックアップを残す数
const BEFORE_RESTORE_KEEP: u32 = 3;

/// 自動のバックアップを確かめる間隔
const AUTO_BACKUP_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// 起動してから、最初に自動のバックアップを確かめるまでの時間（起動時の再スキャンなどと重ねない）
const AUTO_BACKUP_DELAY: Duration = Duration::from_secs(60);

/// アプリデータの中の、DBのファイル名
const DB_FILE_NAME: &str = "muspice.db";

/// アプリデータの中の、設定のファイル名
const SETTINGS_FILE_NAME: &str = "settings.json";

/// アプリデータの中で、バックアップを置くフォルダ（自動のバックアップと、復元の前に退避したもの）
pub fn backups_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("backups")
}

/// アプリデータの中で、次の起動で当てる復元を用意するフォルダ
fn restore_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("restore")
}

/// アプリが作るバックアップの種類（`backups/`に置く。ファイル名の先頭で見分ける）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    /// 自動のバックアップ（1日に1回）
    Auto,
    /// 復元する前に退避した、その時のライブラリのデータ
    BeforeRestore,
}

impl BackupKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::BeforeRestore => "before-restore",
        }
    }

    /// 残す世代の数
    pub fn keep(self) -> usize {
        let keep = match self {
            Self::Auto => AUTO_BACKUP_KEEP,
            Self::BeforeRestore => BEFORE_RESTORE_KEEP,
        };
        keep as usize
    }

    /// ファイル名（`auto-20261021-093000.muspicebackup`。日時はローカル時刻）
    fn file_name(self, now: chrono::DateTime<Local>) -> String {
        format!(
            "{}-{}.{BACKUP_EXTENSION}",
            self.prefix(),
            now.format("%Y%m%d-%H%M%S")
        )
    }

    /// この種類のファイル名なら、日時の部分（`20261021-093000`）
    fn timestamp_of(self, file_name: &str) -> Option<&str> {
        file_name
            .strip_prefix(self.prefix())?
            .strip_prefix('-')?
            .strip_suffix(BACKUP_EXTENSION)?
            .strip_suffix('.')
            .filter(|timestamp| timestamp.len() == 15)
    }
}

/// バックアップに書く情報
pub struct BackupMeta<'a> {
    /// 作ったアプリの版
    pub app_version: &'a str,
    /// 設定（`settings.json`と同じJSON。含めない場合はNone）
    pub settings_json: Option<&'a str>,
}

/// DBを、バックアップのファイルへ写す（同じ名前のファイルがあれば置き換える）
///
/// 写す間はDBの接続を使う（呼び出し側のDBロックの中で行うため、書き込みと重ならない）。書き終わる
/// までは別の名前で書き、途中で失敗した場合は消す（前からあるファイルを壊さない）。
pub fn write_backup(conn: &Connection, path: &Path, meta: &BackupMeta) -> AppResult<()> {
    let partial = partial_path(path);
    remove_if_exists(&partial)?;
    let partial_str = partial
        .to_str()
        .ok_or_else(|| AppError::Validation("保存先のパスを扱えません".to_string()))?;

    let written = conn
        .execute("VACUUM INTO ?1", [partial_str])
        .map_err(|e| AppError::Database(format!("バックアップの作成に失敗しました: {}", e)))
        .and_then(|_| write_meta(&partial, meta))
        .and_then(|()| {
            fs::rename(&partial, path)
                .map_err(|e| AppError::Io(format!("バックアップの保存に失敗しました: {}", e)))
        });
    if written.is_err() {
        fs::remove_file(&partial).ok();
    }
    written
}

/// 書き終わるまでの、別の名前（同じフォルダに置き、書き終えたら名前を変える）
fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".partial");
    PathBuf::from(name)
}

fn remove_if_exists(path: &Path) -> AppResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(format!(
            "ファイルを消せません: {}: {}",
            path.display(),
            e
        ))),
    }
}

/// 写したDBへ、バックアップの情報の表を足す
fn write_meta(path: &Path, meta: &BackupMeta) -> AppResult<()> {
    let conn = Connection::open(path)
        .map_err(|e| AppError::Database(format!("バックアップを開けません: {}", e)))?;
    let rows: [(&str, Option<String>); 4] = [
        ("format_version", Some(FORMAT_VERSION.to_string())),
        ("created_at", Some(Utc::now().to_rfc3339())),
        ("app_version", Some(meta.app_version.to_string())),
        ("settings", meta.settings_json.map(str::to_string)),
    ];
    conn.execute_batch(&format!(
        "CREATE TABLE {META_TABLE} (key TEXT PRIMARY KEY, value TEXT)"
    ))
    .and_then(|()| {
        rows.iter()
            .filter_map(|(key, value)| value.as_ref().map(|value| (key, value)))
            .try_for_each(|(key, value)| {
                conn.execute(
                    &format!("INSERT INTO {META_TABLE} (key, value) VALUES (?1, ?2)"),
                    rusqlite::params![key, value],
                )
                .map(|_| ())
            })
    })
    .map_err(|e| AppError::Database(format!("バックアップの情報の書き込みに失敗しました: {}", e)))
}

/// バックアップのファイル（またはMuspiceのDB）の中身
#[derive(Debug, Clone, PartialEq)]
pub struct BackupInfo {
    /// 作った日時（RFC 3339。DBのファイルそのものなど、情報の表がない場合はNone）
    pub created_at: Option<String>,
    /// 作ったアプリの版
    pub app_version: Option<String>,
    pub track_count: u32,
    pub playlist_count: u32,
    pub library_folder_count: u32,
    /// 設定を含むか（含まない場合、復元しても設定は今のまま）
    pub includes_settings: bool,
}

/// 復元できないファイルの理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupProblem {
    /// Muspiceのバックアップ（DB）ではない
    NotBackup,
    /// ファイルが壊れている（読めない）
    Damaged,
    /// 新しい版のアプリが作った、この版では復元できない形式
    NewerFormat,
}

/// バックアップのファイルを調べる（読むだけ。DBのファイルそのもの（Time Machineから取り出した
/// `muspice.db`など）も、設定を含まないバックアップとして受け付ける）
pub fn inspect_backup(path: &Path) -> Result<BackupInfo, BackupProblem> {
    let conn = open_read_only(path).map_err(|_| BackupProblem::Damaged)?;
    let problem = |e: rusqlite::Error| match e.sqlite_error_code() {
        Some(ErrorCode::NotADatabase) => BackupProblem::NotBackup,
        _ => BackupProblem::Damaged,
    };

    let has_table = |name: &str| -> Result<bool, BackupProblem> {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .map_err(problem)
    };
    for table in ["tracks", "playlists", "library_folders"] {
        if !has_table(table)? {
            return Err(BackupProblem::NotBackup);
        }
    }
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(problem)?;
    if check != "ok" {
        return Err(BackupProblem::Damaged);
    }

    let meta = |key: &str| -> Result<Option<String>, BackupProblem> {
        conn.query_row(
            &format!("SELECT value FROM {META_TABLE} WHERE key = ?1"),
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(problem)
    };
    let has_meta = has_table(META_TABLE)?;
    let (format_version, created_at, app_version, settings) = if has_meta {
        (
            meta("format_version")?,
            meta("created_at")?,
            meta("app_version")?,
            meta("settings")?,
        )
    } else {
        (None, None, None, None)
    };
    if format_version
        .and_then(|version| version.parse::<i64>().ok())
        .is_some_and(|version| version > FORMAT_VERSION)
    {
        return Err(BackupProblem::NewerFormat);
    }

    let count = |table: &str| -> Result<u32, BackupProblem> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(problem)
    };
    Ok(BackupInfo {
        created_at,
        app_version,
        track_count: count("tracks")?,
        playlist_count: count("playlists")?,
        library_folder_count: count("library_folders")?,
        includes_settings: settings.is_some(),
    })
}

/// 読むだけの接続を開く（バックアップのファイルを書き換えない）
fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

/// バックアップのファイルを、次の起動で当てる復元として、アプリデータの`restore/`へ用意する
///
/// DBは`VACUUM INTO`で写し（元のファイルは変えない）、情報の表を除く。設定を含むバックアップなら、
/// 設定のファイルも用意する。DBは最後に名前を変えて置くため、途中で失敗した用意は当てない。
pub fn stage_restore(source: &Path, app_data_dir: &Path) -> AppResult<()> {
    let dir = restore_dir(app_data_dir);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|e| AppError::Io(format!("前の復元の用意を消せません: {}", e)))?;
    }
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Io(format!("復元の用意に失敗しました: {}", e)))?;

    let partial = dir.join(format!("{DB_FILE_NAME}.partial"));
    let partial_str = partial
        .to_str()
        .ok_or_else(|| AppError::Validation("アプリのデータのパスを扱えません".to_string()))?;
    open_read_only(source)
        .and_then(|conn| conn.execute("VACUUM INTO ?1", [partial_str]))
        .map_err(|e| AppError::Database(format!("バックアップを写せません: {}", e)))?;

    let staged = Connection::open(&partial)
        .map_err(|e| AppError::Database(format!("復元の用意に失敗しました: {}", e)))?;
    let settings: Option<String> = staged
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [META_TABLE],
            |row| row.get::<_, i64>(0),
        )
        .and_then(|count| {
            if count == 0 {
                return Ok(None);
            }
            staged
                .query_row(
                    &format!("SELECT value FROM {META_TABLE} WHERE key = 'settings'"),
                    [],
                    |row| row.get(0),
                )
                .optional()
        })
        .and_then(|settings| {
            staged.execute_batch(&format!("DROP TABLE IF EXISTS {META_TABLE}"))?;
            Ok(settings)
        })
        .map_err(|e| AppError::Database(format!("復元の用意に失敗しました: {}", e)))?;
    drop(staged);

    if let Some(json) = settings {
        fs::write(dir.join(SETTINGS_FILE_NAME), json)
            .map_err(|e| AppError::Io(format!("設定の復元の用意に失敗しました: {}", e)))?;
    }
    fs::rename(&partial, dir.join(DB_FILE_NAME))
        .map_err(|e| AppError::Io(format!("復元の用意に失敗しました: {}", e)))
}

/// 用意してある復元を当てる（起動時、DBと設定を開く前に呼ぶ）
///
/// 設定を含む復元なら設定のファイルを置き換えてから、DBのファイル（と、前のDBの一時ファイル）を
/// 置き換える。用意が途中で終わっていた（DBがない）場合は、用意を消すだけ。
///
/// 用意したDBがあることを、復元を当てる印にしているため、DBを最後に置き換える（DBを先に
/// 置き換えた後に落ちると、次の起動では印がなく、設定の復元が黙って失われる）。設定を置き換えた
/// 後に落ちた場合は、次の起動でDBだけを置き換える。
/// @returns 復元したか
pub fn apply_staged_restore(app_data_dir: &Path) -> AppResult<bool> {
    let dir = restore_dir(app_data_dir);
    if !dir.exists() {
        return Ok(false);
    }
    let staged_db = dir.join(DB_FILE_NAME);
    let applied = staged_db.is_file();
    if applied {
        let staged_settings = dir.join(SETTINGS_FILE_NAME);
        if staged_settings.is_file() {
            fs::rename(&staged_settings, app_data_dir.join(SETTINGS_FILE_NAME))
                .map_err(|e| AppError::Io(format!("設定を置き換えられません: {}", e)))?;
        }
        // 前のDBのジャーナルが残っていると、置き換えたDBに当てられてしまうため、先に消す
        for suffix in ["-journal", "-wal", "-shm"] {
            remove_if_exists(&app_data_dir.join(format!("{DB_FILE_NAME}{suffix}")))?;
        }
        fs::rename(&staged_db, app_data_dir.join(DB_FILE_NAME))
            .map_err(|e| AppError::Io(format!("DBを置き換えられません: {}", e)))?;
    }
    fs::remove_dir_all(&dir).map_err(|e| AppError::Io(format!("復元の用意を消せません: {}", e)))?;
    Ok(applied)
}

/// アプリが作るバックアップを、`backups/`へ作り、古い世代を消す
///
/// @returns 作ったファイル
pub fn create_backup_in(
    dir: &Path,
    kind: BackupKind,
    conn: &Connection,
    meta: &BackupMeta,
) -> AppResult<PathBuf> {
    fs::create_dir_all(dir)
        .map_err(|e| AppError::Io(format!("バックアップのフォルダを作れません: {}", e)))?;
    let path = dir.join(kind.file_name(Local::now()));
    write_backup(conn, &path, meta)?;
    prune_backups(dir, kind)?;
    Ok(path)
}

/// この種類のバックアップのファイル名を、新しい順に返す（フォルダがなければ空）
fn backup_names(dir: &Path, kind: BackupKind) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| kind.timestamp_of(name).is_some())
        .collect();
    // 日時は桁をそろえてあるため、名前の順が日時の順
    names.sort_by(|a, b| b.cmp(a));
    names
}

/// この種類の古いバックアップを消す（新しい`kind.keep()`件を残す）
fn prune_backups(dir: &Path, kind: BackupKind) -> AppResult<()> {
    for name in backup_names(dir, kind).iter().skip(kind.keep()) {
        remove_if_exists(&dir.join(name))?;
    }
    Ok(())
}

/// 今日の自動のバックアップが、まだないか（今日はローカル時刻の日付）
pub fn needs_auto_backup(dir: &Path, today: chrono::NaiveDate) -> bool {
    let today = today.format("%Y%m%d").to_string();
    backup_names(dir, BackupKind::Auto)
        .first()
        .and_then(|name| BackupKind::Auto.timestamp_of(name))
        .is_none_or(|timestamp| !timestamp.starts_with(&today))
}

/// 自動のバックアップを始める（別スレッドで、起動の少し後と、その後1時間ごとに確かめる）
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("auto-backup".to_string())
        .spawn(move || {
            std::thread::sleep(AUTO_BACKUP_DELAY);
            loop {
                if let Err(e) = run_auto_backup(&app) {
                    log::warn!("自動のバックアップに失敗しました: {}", e);
                }
                std::thread::sleep(AUTO_BACKUP_CHECK_INTERVAL);
            }
        });
    if let Err(e) = spawned {
        log::warn!("自動のバックアップを始められません: {}", e);
    }
}

/// 設定がオンで、今日の自動のバックアップがなければ作る（ライブラリが空なら作らない）
fn run_auto_backup(app: &AppHandle) -> AppResult<()> {
    let settings = app.state::<SettingsState>().get()?;
    if !settings.auto_backup {
        return Ok(());
    }
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("アプリのデータのフォルダを取得できません: {}", e)))?;
    let dir = backups_dir(&app_data_dir);
    if !needs_auto_backup(&dir, Local::now().date_naive()) {
        return Ok(());
    }

    let settings_json = settings_json(&settings)?;
    let version = app.package_info().version.to_string();
    let created = app.state::<AppState>().with_db(|db| {
        if is_empty_library(db)? {
            return Ok(None);
        }
        let meta = BackupMeta {
            app_version: &version,
            settings_json: Some(&settings_json),
        };
        create_backup_in(&dir, BackupKind::Auto, db, &meta).map(Some)
    })?;
    if let Some(path) = created {
        log::info!("自動のバックアップを作りました: {}", path.display());
    }
    Ok(())
}

/// 曲もプレイリストもないか（自動のバックアップを作らない）
fn is_empty_library(conn: &Connection) -> AppResult<bool> {
    conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM tracks) AND NOT EXISTS (SELECT 1 FROM playlists)",
        [],
        |row| row.get(0),
    )
    .map_err(|e| AppError::Database(format!("ライブラリの確認に失敗しました: {}", e)))
}

/// バックアップに書く設定（`settings.json`と同じ形）
pub fn settings_json(settings: &crate::settings::Settings) -> AppResult<String> {
    serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Io(format!("設定の変換に失敗しました: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    const SETTINGS: &str = r#"{"language":"en"}"#;

    fn meta(settings_json: Option<&str>) -> BackupMeta<'_> {
        BackupMeta {
            app_version: "0.1.0",
            settings_json,
        }
    }

    /// 曲・プレイリスト・ライブラリフォルダの入ったDB
    fn library_db() -> Connection {
        let conn = crate::db::open_in_memory();
        conn.execute_batch(
            "INSERT INTO tracks (id, file_path, file_name, format, file_size, play_count, is_favorite)
             VALUES ('t1', '/old/music/a.mp3', 'a.mp3', 'mp3', 5, 5, 1),
                    ('t2', '/old/music/b.mp3', 'b.mp3', 'mp3', 5, 0, 0);
             INSERT INTO playlists (id, name) VALUES ('p1', '通勤');
             INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES ('p1', 't1', 0);
             INSERT INTO play_history (track_id) VALUES ('t1');
             INSERT INTO library_folders (id, path) VALUES ('f1', '/old/music');",
        )
        .unwrap();
        conn
    }

    /// バックアップは、DBの中身と、作った日時・版・設定を持つ
    #[test]
    fn test_backup_keeps_the_library_and_settings() {
        let dir = TempDir::new();
        let path = dir.path().join("a.muspicebackup");
        write_backup(&library_db(), &path, &meta(Some(SETTINGS))).unwrap();

        let info = inspect_backup(&path).unwrap();
        assert_eq!(
            (
                info.track_count,
                info.playlist_count,
                info.library_folder_count
            ),
            (2, 1, 1)
        );
        assert!(info.includes_settings);
        assert_eq!(info.app_version.as_deref(), Some("0.1.0"));
        assert!(info.created_at.is_some());
        // 書き終えた後に、途中の名前のファイルは残らない
        assert!(!partial_path(&path).exists());

        // 同じ名前で保存し直すと、置き換える
        write_backup(&crate::db::open_in_memory(), &path, &meta(None)).unwrap();
        let info = inspect_backup(&path).unwrap();
        assert_eq!(info.track_count, 0);
        assert!(!info.includes_settings);
    }

    /// DBのファイルそのものも、設定を含まないバックアップとして受け付ける。ほかのファイルは受け付けない
    #[test]
    fn test_inspect_backup_accepts_a_database_and_rejects_other_files() {
        let dir = TempDir::new();
        let db_path = dir.path().join("muspice.db");
        library_db()
            .execute("VACUUM INTO ?1", [db_path.to_str().unwrap()])
            .unwrap();
        let info = inspect_backup(&db_path).unwrap();
        assert_eq!((info.track_count, info.includes_settings), (2, false));
        assert_eq!(info.created_at, None);

        let text = dir.path().join("notes.txt");
        fs::write(&text, "これはバックアップではありません。".repeat(50)).unwrap();
        assert_eq!(inspect_backup(&text), Err(BackupProblem::NotBackup));

        // ほかのアプリのSQLiteのファイル
        let other = dir.path().join("other.sqlite");
        Connection::open(&other)
            .unwrap()
            .execute_batch("CREATE TABLE notes (body TEXT)")
            .unwrap();
        assert_eq!(inspect_backup(&other), Err(BackupProblem::NotBackup));

        // 途中で切れたファイル
        let backup = dir.path().join("a.muspicebackup");
        write_backup(&library_db(), &backup, &meta(None)).unwrap();
        let bytes = fs::read(&backup).unwrap();
        let truncated = dir.path().join("truncated.muspicebackup");
        fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
        assert_eq!(inspect_backup(&truncated), Err(BackupProblem::Damaged));
    }

    /// 新しい版の形式のバックアップは、復元しない
    #[test]
    fn test_inspect_backup_rejects_newer_format() {
        let dir = TempDir::new();
        let path = dir.path().join("new.muspicebackup");
        write_backup(&library_db(), &path, &meta(None)).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE backup_meta SET value = '99' WHERE key = 'format_version'",
                [],
            )
            .unwrap();
        assert_eq!(inspect_backup(&path), Err(BackupProblem::NewerFormat));
    }

    /// 用意した復元は、次の起動でDBと設定を置き換える（前のDBのジャーナルは消す）
    #[test]
    fn test_staged_restore_replaces_database_and_settings() {
        let dir = TempDir::new();
        let app_data = dir.path().join("app");
        fs::create_dir_all(&app_data).unwrap();
        // 今のDB（空）と設定
        drop(crate::db::init_db(app_data.join(DB_FILE_NAME)).unwrap());
        fs::write(app_data.join(SETTINGS_FILE_NAME), r#"{"language":"ja"}"#).unwrap();
        fs::write(app_data.join("muspice.db-journal"), "stale").unwrap();

        let backup = dir.path().join("a.muspicebackup");
        write_backup(&library_db(), &backup, &meta(Some(SETTINGS))).unwrap();
        stage_restore(&backup, &app_data).unwrap();
        // 用意しただけでは、今のDBは変えない
        let current = Connection::open(app_data.join(DB_FILE_NAME)).unwrap();
        let count: i64 = current
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(current);

        assert!(apply_staged_restore(&app_data).unwrap());
        assert!(!restore_dir(&app_data).exists());
        assert!(!app_data.join("muspice.db-journal").exists());
        assert_eq!(
            fs::read_to_string(app_data.join(SETTINGS_FILE_NAME)).unwrap(),
            SETTINGS
        );
        // 置き換えたDBは、アプリと同じ設定で開け、統計・プレイリスト・再生履歴が戻る。情報の表は除く
        let restored = crate::db::init_db(app_data.join(DB_FILE_NAME)).unwrap();
        let row: (i32, bool, i64, i64, i64) = restored
            .query_row(
                "SELECT play_count, is_favorite,
                        (SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 't1'),
                        (SELECT COUNT(*) FROM play_history WHERE track_id = 't1'),
                        (SELECT COUNT(*) FROM sqlite_master WHERE name = 'backup_meta')
                 FROM tracks WHERE id = 't1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row, (5, true, 1, 1, 0));
        // 全文検索の表も使える
        restored
            .execute("UPDATE tracks SET title = 'Morning' WHERE id = 't1'", [])
            .unwrap();
        assert_eq!(
            crate::repository::search_tracks_by_query(&restored, "Morning")
                .unwrap()
                .len(),
            1
        );

        // 用意がなければ、何もしない
        assert!(!apply_staged_restore(&app_data).unwrap());
    }

    /// 設定を含まないバックアップ（DBのファイル）を復元しても、設定は今のまま
    #[test]
    fn test_restoring_a_database_keeps_current_settings() {
        let dir = TempDir::new();
        let app_data = dir.path().join("app");
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join(SETTINGS_FILE_NAME), r#"{"language":"ja"}"#).unwrap();
        let db_path = dir.path().join("muspice.db");
        library_db()
            .execute("VACUUM INTO ?1", [db_path.to_str().unwrap()])
            .unwrap();

        stage_restore(&db_path, &app_data).unwrap();
        assert!(apply_staged_restore(&app_data).unwrap());
        assert_eq!(
            fs::read_to_string(app_data.join(SETTINGS_FILE_NAME)).unwrap(),
            r#"{"language":"ja"}"#
        );
    }

    /// 途中で終わった用意（DBがない）は、当てずに消す
    #[test]
    fn test_incomplete_staged_restore_is_discarded() {
        let dir = TempDir::new();
        let staged = restore_dir(dir.path());
        fs::create_dir_all(&staged).unwrap();
        fs::write(staged.join("muspice.db.partial"), "partial").unwrap();
        fs::write(dir.path().join(DB_FILE_NAME), "current").unwrap();

        assert!(!apply_staged_restore(dir.path()).unwrap());
        assert!(!staged.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(DB_FILE_NAME)).unwrap(),
            "current"
        );
    }

    /// 復元を当てている途中（設定を置き換えた後）で落ちても、次の起動でDBを置き換える
    #[test]
    fn test_restore_interrupted_after_settings_is_finished_next_time() {
        let dir = TempDir::new();
        let app_data = dir.path().join("app");
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join(DB_FILE_NAME), "current").unwrap();
        let backup = dir.path().join("a.muspicebackup");
        write_backup(&library_db(), &backup, &meta(Some(SETTINGS))).unwrap();
        stage_restore(&backup, &app_data).unwrap();
        // 設定を置き換えたところで落ちた（用意したDBは、まだ残っている）
        fs::rename(
            restore_dir(&app_data).join(SETTINGS_FILE_NAME),
            app_data.join(SETTINGS_FILE_NAME),
        )
        .unwrap();

        assert!(apply_staged_restore(&app_data).unwrap());
        assert!(!restore_dir(&app_data).exists());
        assert_eq!(
            fs::read_to_string(app_data.join(SETTINGS_FILE_NAME)).unwrap(),
            SETTINGS
        );
        let restored = crate::db::init_db(app_data.join(DB_FILE_NAME)).unwrap();
        let count: i64 = restored
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .unwrap();
        assert!(count > 0);
    }

    /// アプリが作るバックアップは、種類ごとに新しい世代だけを残す
    #[test]
    fn test_prune_keeps_the_newest_backups_of_each_kind() {
        let dir = TempDir::new();
        for day in 1..=9 {
            fs::write(
                dir.path()
                    .join(format!("auto-202610{day:02}-120000.muspicebackup")),
                "",
            )
            .unwrap();
        }
        fs::write(
            dir.path()
                .join("before-restore-20261001-120000.muspicebackup"),
            "",
        )
        .unwrap();
        fs::write(dir.path().join("my notes.txt"), "").unwrap();

        prune_backups(dir.path(), BackupKind::Auto).unwrap();
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "auto-20261003-120000.muspicebackup",
                "auto-20261004-120000.muspicebackup",
                "auto-20261005-120000.muspicebackup",
                "auto-20261006-120000.muspicebackup",
                "auto-20261007-120000.muspicebackup",
                "auto-20261008-120000.muspicebackup",
                "auto-20261009-120000.muspicebackup",
                "before-restore-20261001-120000.muspicebackup",
                "my notes.txt",
            ]
        );
    }

    /// 自動のバックアップは、その日のものがなければ作る
    #[test]
    fn test_needs_auto_backup_once_a_day() {
        let dir = TempDir::new();
        let day = |d: u32| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        assert!(needs_auto_backup(dir.path(), day(21)));

        let path = create_backup_in(
            dir.path(),
            BackupKind::Auto,
            &library_db(),
            &meta(Some(SETTINGS)),
        )
        .unwrap();
        let today = Local::now().date_naive();
        assert!(!needs_auto_backup(dir.path(), today));
        assert!(needs_auto_backup(dir.path(), today.succ_opt().unwrap()));
        // 復元の前に退避したものは、自動のバックアップに数えない
        fs::remove_file(path).unwrap();
        create_backup_in(
            dir.path(),
            BackupKind::BeforeRestore,
            &library_db(),
            &meta(None),
        )
        .unwrap();
        assert!(needs_auto_backup(dir.path(), today));
    }

    #[test]
    fn test_empty_library_is_not_backed_up_automatically() {
        assert!(is_empty_library(&crate::db::open_in_memory()).unwrap());
        assert!(!is_empty_library(&library_db()).unwrap());
    }
}
