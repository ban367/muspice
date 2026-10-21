//! ライブラリのデータ（DB・設定）のバックアップ・復元のコマンド（ADR-073）
//!
//! 保存先・復元するファイルは、Rust側のダイアログで選ぶ（WebViewからパスを受け取らない。ADR-065）。
//! 復元は、選んだファイルを調べて預かり（`PendingRestore`）、画面で確かめてから行う。

use super::run_blocking;
use crate::backup::{
    BACKUP_EXTENSION, BackupKind, BackupMeta, BackupProblem, backups_dir, create_backup_in,
    inspect_backup, settings_json, stage_restore, write_backup,
};
use crate::error::{AppError, AppResult};
use crate::settings::{Language, SettingsState};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// ダイアログの文言（設定の言語に合わせる）
struct Labels {
    save_title: &'static str,
    restore_title: &'static str,
    filter: &'static str,
    /// 保存するファイルの、既定の名前（日付の前）
    file_name: &'static str,
}

impl Labels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                save_title: "ライブラリのバックアップを保存",
                restore_title: "復元するバックアップを選択",
                filter: "Muspiceのバックアップ",
                file_name: "Muspiceのバックアップ",
            },
            Language::En => Self {
                save_title: "Save a Library Backup",
                restore_title: "Select a Backup to Restore",
                filter: "Muspice Backups",
                file_name: "Muspice Backup",
            },
        }
    }
}

/// バックアップを保存した結果
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSaved {
    /// 保存したファイル
    pub path: String,
    pub track_count: u32,
    pub playlist_count: u32,
}

/// 復元するファイルを選んだ結果
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RestorePlan {
    /// 復元できる（選んだファイルを預かる。復元するなら`restore_library_backup`、やめるなら
    /// `cancel_library_restore`）
    Ready {
        #[serde(rename = "fileName")]
        file_name: String,
        /// 作った日時（RFC 3339。DBのファイルそのものなど、分からなければnull）
        #[serde(rename = "createdAt")]
        created_at: Option<String>,
        #[serde(rename = "trackCount")]
        track_count: u32,
        #[serde(rename = "playlistCount")]
        playlist_count: u32,
        #[serde(rename = "libraryFolderCount")]
        library_folder_count: u32,
        /// 設定を含むか（含まなければ、設定は今のまま）
        #[serde(rename = "includesSettings")]
        includes_settings: bool,
    },
    /// Muspiceのバックアップ（DB）ではない
    NotBackup {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    /// ファイルが壊れている
    Damaged {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    /// 新しい版のアプリが作った、この版では復元できないバックアップ
    NewerFormat {
        #[serde(rename = "fileName")]
        file_name: String,
    },
}

/// 復元するファイルを選んでから、復元するかを決めるまで預かっているファイル（Tauriの管理状態。多くても1つ）
#[derive(Default)]
pub struct PendingRestore(Mutex<Option<PathBuf>>);

impl PendingRestore {
    fn hold(&self, path: PathBuf) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = Some(path);
        }
    }

    fn take(&self) -> Option<PathBuf> {
        self.0.lock().ok().and_then(|mut pending| pending.take())
    }
}

/// アプリのデータのフォルダ
fn app_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("アプリのデータのフォルダを取得できません: {}", e)))
}

/// 今の設定（バックアップに書く形）
fn current_settings_json(app: &AppHandle) -> AppResult<String> {
    settings_json(&app.state::<SettingsState>().get()?)
}

/// ライブラリのデータ（DB・設定）を、保存先を選ぶダイアログ（Rust側。呼び出したウィンドウの上に出す）で
/// 選んだファイルへバックアップする
///
/// 拡張子がなければ`.muspicebackup`を付ける。曲のファイルは含まない。
/// @returns 保存した結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn export_library_backup(
    window: WebviewWindow,
    app_handle: AppHandle,
) -> AppResult<Option<BackupSaved>> {
    run_blocking(move || {
        let labels = Labels::for_language(crate::menu::current_language(&app_handle));
        let file_name = format!(
            "{} {}.{BACKUP_EXTENSION}",
            labels.file_name,
            chrono::Local::now().format("%Y-%m-%d")
        );
        let Some(file) = app_handle
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(labels.save_title)
            .add_filter(labels.filter, &[BACKUP_EXTENSION])
            .set_file_name(file_name)
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("保存先を扱えません: {}", e)))?;
        save_backup(&app_handle, &with_backup_extension(path)).map(Some)
    })
    .await
}

/// `export_library_backup`のダイアログの後の処理
fn save_backup(app: &AppHandle, path: &Path) -> AppResult<BackupSaved> {
    let settings = current_settings_json(app)?;
    let version = app.package_info().version.to_string();
    let meta = BackupMeta {
        app_version: &version,
        settings_json: Some(&settings),
    };
    app.state::<AppState>()
        .with_db(|db| write_backup(db, path, &meta))?;
    let info = inspect_backup(path)
        .map_err(|_| AppError::Io("保存したバックアップを読めません".to_string()))?;
    log::info!("バックアップを保存しました: {}", path.display());
    Ok(BackupSaved {
        path: path.to_string_lossy().into_owned(),
        track_count: info.track_count,
        playlist_count: info.playlist_count,
    })
}

/// 拡張子がなければ`.muspicebackup`を付ける（ほかの拡張子は、利用者が付けたものとして残す）
fn with_backup_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(format!(".{BACKUP_EXTENSION}"));
    PathBuf::from(name)
}

/// 復元するファイルを選ぶダイアログ（Rust側。呼び出したウィンドウの上に、アプリのバックアップのフォルダを
/// 開いた状態で出す）を開き、復元できるかと、中身の件数を返す
///
/// Muspiceのバックアップのほか、DBのファイルそのもの（Time Machineから取り出した`muspice.db`など）も
/// 選べる（設定は含まない）。復元できるファイルなら預かる（`Ready`）。
/// @returns 選んだ結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn plan_library_restore(
    window: WebviewWindow,
    app_handle: AppHandle,
) -> AppResult<Option<RestorePlan>> {
    run_blocking(move || {
        let pending = app_handle.state::<PendingRestore>();
        // 前に選んだファイルは手放す（選び直したファイルだけを復元する）
        pending.take();

        let labels = Labels::for_language(crate::menu::current_language(&app_handle));
        let mut dialog = app_handle
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(labels.restore_title)
            .add_filter(labels.filter, &[BACKUP_EXTENSION, "db"]);
        let backups = backups_dir(&app_data_dir(&app_handle)?);
        if backups.is_dir() {
            dialog = dialog.set_directory(backups);
        }
        let Some(file) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("選んだファイルを扱えません: {}", e)))?;
        if !path.is_file() {
            return Err(AppError::NotFound(format!(
                "ファイルが見つかりません: {}",
                path.display()
            )));
        }

        let plan = plan_restore(&path);
        if matches!(plan, RestorePlan::Ready { .. }) {
            pending.hold(path);
        }
        Ok(Some(plan))
    })
    .await
}

/// 選んだファイルを調べ、復元できるかを返す
fn plan_restore(path: &Path) -> RestorePlan {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match inspect_backup(path) {
        Ok(info) => RestorePlan::Ready {
            file_name,
            created_at: info.created_at,
            track_count: info.track_count,
            playlist_count: info.playlist_count,
            library_folder_count: info.library_folder_count,
            includes_settings: info.includes_settings,
        },
        Err(BackupProblem::NotBackup) => RestorePlan::NotBackup { file_name },
        Err(BackupProblem::Damaged) => RestorePlan::Damaged { file_name },
        Err(BackupProblem::NewerFormat) => RestorePlan::NewerFormat { file_name },
    }
}

/// 預かっているファイル（`plan_library_restore`で選んだもの）から、ライブラリのデータを復元し、アプリを再起動する
///
/// 今のDBと設定を、アプリのバックアップのフォルダへ退避してから、復元するDB（と設定）を用意し、
/// 再起動を求める（DBは、次の起動で開く前に置き換える）。インポート・再スキャンが終わるのを待ってから行う。
/// 預かっているファイルがなければ`NOT_FOUND`。
#[tauri::command]
#[specta::specta]
pub async fn restore_library_backup(app_handle: AppHandle) -> AppResult<()> {
    run_blocking(move || {
        let source = app_handle
            .state::<PendingRestore>()
            .take()
            .ok_or_else(|| AppError::NotFound("復元するファイルが選ばれていません".to_string()))?;
        let app_data = app_data_dir(&app_handle)?;
        let state = app_handle.state::<AppState>();
        {
            // 取り込み・再スキャン・ファイルの整理の途中で再起動しないよう、終わるまで待つ
            let _scan = state.lock_library_scan();
            let settings = current_settings_json(&app_handle)?;
            let version = app_handle.package_info().version.to_string();
            let meta = BackupMeta {
                app_version: &version,
                settings_json: Some(&settings),
            };
            let saved = state.with_db(|db| {
                create_backup_in(
                    &backups_dir(&app_data),
                    BackupKind::BeforeRestore,
                    db,
                    &meta,
                )
            })?;
            log::info!("復元する前のライブラリを退避しました: {}", saved.display());
            stage_restore(&source, &app_data)?;
        }
        log::info!(
            "バックアップからの復元を用意しました。再起動します: {}",
            source.display()
        );
        app_handle.request_restart();
        Ok(())
    })
    .await
}

/// 預かっている復元するファイルを手放す（預かっていなければ、何もしない）
#[tauri::command]
#[specta::specta]
pub fn cancel_library_restore(pending: tauri::State<'_, PendingRestore>) {
    pending.take();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use rusqlite::Connection;
    use std::fs;

    #[test]
    fn test_with_backup_extension() {
        assert_eq!(
            with_backup_extension(PathBuf::from("/b/Backup")),
            PathBuf::from("/b/Backup.muspicebackup")
        );
        assert_eq!(
            with_backup_extension(PathBuf::from("/b/Backup.muspicebackup")),
            PathBuf::from("/b/Backup.muspicebackup")
        );
        assert_eq!(
            with_backup_extension(PathBuf::from("/b/library.db")),
            PathBuf::from("/b/library.db")
        );
    }

    /// 選んだファイルの中身を、復元できるか（できなければ理由）として返す
    #[test]
    fn test_plan_restore_describes_the_file() {
        let dir = TempDir::new();
        let backup = dir.path().join("a.muspicebackup");
        let conn = crate::db::open_in_memory();
        conn.execute_batch(
            "INSERT INTO tracks (id, file_path, file_name) VALUES ('t1', '/m/a.mp3', 'a.mp3');
             INSERT INTO library_folders (id, path) VALUES ('f1', '/m');",
        )
        .unwrap();
        let meta = BackupMeta {
            app_version: "0.1.0",
            settings_json: Some("{}"),
        };
        write_backup(&conn, &backup, &meta).unwrap();

        let RestorePlan::Ready {
            file_name,
            created_at,
            track_count,
            playlist_count,
            library_folder_count,
            includes_settings,
        } = plan_restore(&backup)
        else {
            panic!("復元できるはず");
        };
        assert_eq!(file_name, "a.muspicebackup");
        assert!(created_at.is_some());
        assert_eq!(
            (
                track_count,
                playlist_count,
                library_folder_count,
                includes_settings
            ),
            (1, 0, 1, true)
        );

        let text = dir.path().join("notes.txt");
        fs::write(&text, "x".repeat(4096)).unwrap();
        assert_eq!(
            plan_restore(&text),
            RestorePlan::NotBackup {
                file_name: "notes.txt".to_string()
            }
        );
        Connection::open(&backup)
            .unwrap()
            .execute(
                "UPDATE backup_meta SET value = '2' WHERE key = 'format_version'",
                [],
            )
            .unwrap();
        assert_eq!(
            plan_restore(&backup),
            RestorePlan::NewerFormat {
                file_name: "a.muspicebackup".to_string()
            }
        );
    }

    /// 預かったファイルは、取り出すと手放す
    #[test]
    fn test_pending_restore_is_taken_once() {
        let pending = PendingRestore::default();
        pending.hold(PathBuf::from("/b/a.muspicebackup"));
        assert_eq!(pending.take(), Some(PathBuf::from("/b/a.muspicebackup")));
        assert_eq!(pending.take(), None);
    }
}
