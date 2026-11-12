//! 音楽フォルダ（新しい曲を置く宛先）の設定のコマンド
//!
//! 音楽フォルダは、ライブラリフォルダの1つに付けた印（`library_folders.is_music_folder`。ADR-068）。
//! 音楽フォルダを変えると、ライブラリをそのフォルダの中身に置き換える（ADR-086）。新しいフォルダの外の
//! 曲と、ほかのライブラリフォルダの記録を外し（ファイルは消さない）、新しいフォルダを登録して中の曲を
//! 読み込む。新しいフォルダの中にある登録済みの曲は、記録（再生回数など）を残す。
//!
//! 外す曲がある時は、選んだフォルダを預かり（`PendingMusicFolder`）、画面で確かめてから置き換える。
//! その時は、通常のプレイリストも消し、選ばれれば、ライブラリのデータを`backups/`へ書き出してから
//! 置き換える。外す曲がない時（新しいフォルダが今の曲をすべて含む時）は、確かめずに音楽フォルダにする。
//! 音楽フォルダの書式は、設定（`Settings::music_folder_template`）に持つ。
//!
//! 既定の音楽フォルダは、OSの音楽フォルダの中の専用のフォルダ（`~/Music/Muspice`）。音楽フォルダに
//! する時に作る（ADR-083）。

use super::import::{FolderRegistration, import_folder_blocking};
use super::library_folders::notify_library_changed;
use super::{pick_folder, run_blocking};
use crate::backup::{BackupKind, BackupMeta, backups_dir, create_backup_in, settings_json};
use crate::error::{AppError, AppResult};
use crate::events::ImportProgress;
use crate::file_organize::Template;
use crate::library::{DuplicateAction, ImportResult};
use crate::library_folder::{
    find_all_folders, find_folder_by_path, find_music_folder, normalize_folder_path,
    track_path_prefix,
};
use crate::library_sync::LibrarySync;
use crate::models::Track;
use crate::settings::{Language, SettingsState};
use crate::state::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// 書式の例に出す曲の数
const TEMPLATE_EXAMPLE_COUNT: u32 = 5;

/// 既定の音楽フォルダの名前（OSの音楽フォルダの中に作る。ADR-083）
const DEFAULT_MUSIC_FOLDER_NAME: &str = "Muspice";

/// 音楽フォルダ（印を付けたライブラリフォルダ）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolder {
    /// ライブラリフォルダのID
    pub id: String,
    pub path: String,
    /// フォルダが今も存在するか（外付けドライブが外れている場合などはfalse）
    pub exists: bool,
}

/// 音楽フォルダの設定（画面に出す）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolderInfo {
    /// 音楽フォルダ（設定していなければnull）
    pub folder: Option<MusicFolder>,
    /// 既定の音楽フォルダ（OSの音楽フォルダの中の`Muspice`。まだないこともある。OSの音楽フォルダが
    /// 見つからなければnull）
    pub default_folder: Option<String>,
}

/// 音楽フォルダにするフォルダの選び方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MusicFolderSource {
    /// 既定の音楽フォルダ（`~/Music/Muspice`。なければ作る。パスはRust側で求める）
    Default,
    /// ダイアログで選ぶ
    Pick,
}

/// ライブラリを置き換えた結果（ADR-086）
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryReplaced {
    /// ライブラリから外した曲の数（ファイルは消していない）
    pub removed_track_count: u32,
    /// 消した通常のプレイリストの数
    pub removed_playlist_count: u32,
    /// 置き換える前に書き出したバックアップ（書き出さなかった場合はnull）
    pub backup_path: Option<String>,
}

/// 音楽フォルダを選んだ結果
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MusicFolderChange {
    /// 音楽フォルダにした
    Set {
        path: String,
        /// 中の曲を読み込んだ結果（登録済みのライブラリフォルダに、印を移しただけの場合はnull）
        imported: Option<ImportResult>,
        /// ライブラリを置き換えた結果（外す曲がなく、確かめずに音楽フォルダにした場合はnull）
        replaced: Option<LibraryReplaced>,
    },
    /// 外す曲があるため、置き換える前に確かめる（選んだフォルダを預かった。置き換えるなら
    /// `confirm_music_folder_change`、やめるなら`cancel_music_folder_change`）
    ConfirmReplace {
        path: String,
        /// ライブラリから外す曲の数（新しいフォルダの外の曲）
        #[serde(rename = "removedTrackCount")]
        removed_track_count: u32,
        /// 残る曲の数（新しいフォルダの中にある登録済みの曲）
        #[serde(rename = "keptTrackCount")]
        kept_track_count: u32,
        /// 消す通常のプレイリストの数
        #[serde(rename = "playlistCount")]
        playlist_count: u32,
    },
}

/// 置き換えを確かめる間、預かっている音楽フォルダ（Tauriの管理状態。多くても1つ）
#[derive(Default)]
pub struct PendingMusicFolder(Mutex<Option<PendingChange>>);

/// 預かっている音楽フォルダ
struct PendingChange {
    folder: PathBuf,
    /// 既定の音楽フォルダか（なければ、置き換える時に作る）
    is_default: bool,
}

impl PendingMusicFolder {
    fn hold(&self, change: PendingChange) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = Some(change);
        }
    }

    fn take(&self) -> Option<PendingChange> {
        self.0.lock().ok().and_then(|mut pending| pending.take())
    }
}

/// 置き換える前に、ライブラリのデータを書き出す関数（書き出したファイルを返す）
type BackupWriter<'a> = &'a dyn Fn(&Connection) -> AppResult<PathBuf>;

/// 音楽フォルダを変える時の、置き換えの扱い
enum ChangeMode<'a> {
    /// 外す曲があれば、置き換えずに確認を求める（`ConfirmReplace`を返す）。なければ音楽フォルダにする
    AskIfRemoving,
    /// 確かめた後に置き換える（通常のプレイリストも消す）。`backup`があれば、置き換える前に呼んで
    /// ライブラリのデータを書き出す（書き出したファイルを返す）
    Replace { backup: Option<BackupWriter<'a>> },
}

/// 音楽フォルダの設定と、既定の音楽フォルダ（候補）を取得
///
/// フォルダの存在確認は、外れたネットワークドライブなどで時間がかかることがあるため、
/// ブロッキング処理用スレッドで行う。
#[tauri::command]
#[specta::specta]
pub async fn get_music_folder(app_handle: AppHandle) -> AppResult<MusicFolderInfo> {
    run_blocking(move || {
        let record = app_handle
            .state::<AppState>()
            .with_db(|db| find_music_folder(db))?;
        Ok(MusicFolderInfo {
            folder: record.map(|record| MusicFolder {
                exists: Path::new(&record.path).is_dir(),
                id: record.id,
                path: record.path,
            }),
            default_folder: default_music_folder(&app_handle)
                .and_then(|path| path.to_str().map(str::to_string)),
        })
    })
    .await
}

/// 音楽フォルダを選ぶ
///
/// 既定の音楽フォルダ（なければ作る）か、ダイアログ（Rust側で、呼び出したウィンドウの上に出す。
/// ADR-065）で選んだフォルダを、音楽フォルダにし、ライブラリをそのフォルダの中身に置き換える（ADR-086）。
///
/// - 新しいフォルダの外に曲があれば、置き換えずに`ConfirmReplace`を返す（フォルダを預かる）
/// - なければ、ほかのライブラリフォルダの記録を外し、新しいフォルダを音楽フォルダにする。登録済みの
///   ライブラリフォルダなら印を付けるだけ（読み込み直さない）。それ以外は、空でもライブラリフォルダと
///   して登録し、中の曲を読み込む（`ImportProgress`を送る）。直下の既知のアプリのフォルダは、読み込まない
///   フォルダに入れる（ADR-062）
/// - 今の音楽フォルダを選んだ場合は、何もしない
///
/// @returns 結果（ダイアログで選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn set_music_folder(
    source: MusicFolderSource,
    window: WebviewWindow,
    app_handle: AppHandle,
) -> AppResult<Option<MusicFolderChange>> {
    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let pending = app_handle.state::<PendingMusicFolder>();
        // 前に預けたまま、決めずに終わったものは手放す
        pending.take();
        let (folder, is_default) = match source {
            MusicFolderSource::Default => (
                default_music_folder(&app_handle).ok_or_else(|| {
                    AppError::NotFound("OSの音楽フォルダが見つかりません".to_string())
                })?,
                true,
            ),
            MusicFolderSource::Pick => {
                let title = match crate::menu::current_language(&app_handle) {
                    Language::Ja => "音楽フォルダを選択",
                    Language::En => "Select a Music Folder",
                };
                // 今の音楽フォルダ（なければOSの音楽フォルダ）を開いた状態で出す
                let current = state
                    .with_db(|db| find_music_folder(db))?
                    .map(|folder| PathBuf::from(folder.path))
                    .filter(|path| path.is_dir());
                let mut dialog = app_handle
                    .dialog()
                    .file()
                    .set_parent(&window)
                    .set_title(title);
                if let Some(directory) = current.or_else(|| system_music_folder(&app_handle)) {
                    dialog = dialog.set_directory(directory);
                }
                let Some(folder) = pick_folder(dialog)? else {
                    return Ok(None);
                };
                (folder, false)
            }
        };

        let change = change_music_folder(
            state.inner(),
            &folder,
            is_default,
            ChangeMode::AskIfRemoving,
            &|progress| emit_progress(&app_handle, progress),
        )?;
        match &change {
            MusicFolderChange::ConfirmReplace { .. } => {
                pending.hold(PendingChange { folder, is_default });
            }
            MusicFolderChange::Set { .. } => after_change(&app_handle),
        }
        Ok(Some(change))
    })
    .await
}

/// 預かっている音楽フォルダで、ライブラリを置き換える（`set_music_folder`が`ConfirmReplace`を返した後）
///
/// `backup`がtrueなら、置き換える前に、ライブラリのデータ（DB・設定）を`backups/`へ書き出す
/// （設定の「バックアップ」の復元で戻せる）。新しいフォルダの外の曲・ほかのライブラリフォルダの記録・
/// 通常のプレイリストを外し、新しいフォルダを音楽フォルダにして中の曲を読み込む。
#[tauri::command]
#[specta::specta]
pub async fn confirm_music_folder_change(
    backup: bool,
    app_handle: AppHandle,
) -> AppResult<MusicFolderChange> {
    run_blocking(move || {
        let pending = app_handle
            .state::<PendingMusicFolder>()
            .take()
            .ok_or_else(|| {
                AppError::NotFound("置き換える音楽フォルダが選ばれていません".to_string())
            })?;
        let write_backup = |db: &Connection| backup_before_change(&app_handle, db);
        let change = change_music_folder(
            app_handle.state::<AppState>().inner(),
            &pending.folder,
            pending.is_default,
            ChangeMode::Replace {
                backup: backup.then_some(&write_backup as BackupWriter),
            },
            &|progress| emit_progress(&app_handle, progress),
        );
        // 途中で失敗した場合も、外した曲・読み込んだ曲があるため、画面に反映させる
        after_change(&app_handle);
        change
    })
    .await
}

/// 預かっている音楽フォルダを手放す（置き換えをやめた）
#[tauri::command]
#[specta::specta]
pub fn cancel_music_folder_change(pending: tauri::State<'_, PendingMusicFolder>) -> AppResult<()> {
    pending.take();
    Ok(())
}

/// 音楽フォルダの設定を外す（ライブラリフォルダの記録・ファイルは、そのまま）
#[tauri::command]
#[specta::specta]
pub async fn clear_music_folder(app_handle: AppHandle) -> AppResult<()> {
    app_handle
        .state::<AppState>()
        .with_db(|db| crate::library_folder::clear_music_folder(db))?;
    notify_library_changed(&app_handle);
    Ok(())
}

/// 音楽フォルダの書式の例（最近追加したアルバムの最初の曲を、書式に当てはめた、音楽フォルダからの場所）
///
/// 書式が読めない場合は`VALIDATION`。ライブラリに曲がなければ空。
#[tauri::command]
#[specta::specta]
pub async fn preview_music_folder_template(
    template: String,
    app_handle: AppHandle,
) -> AppResult<Vec<String>> {
    let template = Template::parse(&template)?;
    run_blocking(move || {
        let tracks = app_handle.state::<AppState>().with_db(|db| {
            crate::repository::find_recent_album_tracks(db, TEMPLATE_EXAMPLE_COUNT)
        })?;
        Ok(template_examples(&template, &tracks))
    })
    .await
}

/// 新しいフォルダの曲の読み込みの進み具合を送る
fn emit_progress(app: &AppHandle, progress: ImportProgress) {
    if let Err(e) = progress.emit(app) {
        log::warn!("Failed to emit progress event: {}", e);
    }
}

/// 音楽フォルダを変えた後に、監視と画面に反映させる
fn after_change(app: &AppHandle) {
    // 新しく登録したフォルダを監視し、外したフォルダは監視しない（監視が有効な場合）
    app.state::<LibrarySync>().refresh_watches(app);
    // 曲・プレイリストの一覧と、両方のウィンドウの音楽フォルダの表示に反映させる
    notify_library_changed(app);
}

/// 置き換える前に、ライブラリのデータ（DB・設定）を`backups/`へ書き出す
fn backup_before_change(app: &AppHandle, db: &Connection) -> AppResult<PathBuf> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("アプリのデータのフォルダを取得できません: {}", e)))?;
    let settings_json = settings_json(&app.state::<SettingsState>().get()?)?;
    let version = app.package_info().version.to_string();
    let meta = BackupMeta {
        app_version: &version,
        settings_json: Some(&settings_json),
    };
    let path = create_backup_in(
        &backups_dir(&app_data_dir),
        BackupKind::BeforeMusicFolderChange,
        db,
        &meta,
    )?;
    log::info!(
        "Backed up the library before changing the music folder: {}",
        path.display()
    );
    Ok(path)
}

/// OSの音楽フォルダ（見つからなければNone。既定の音楽フォルダと、ダイアログの最初の場所に使う）
fn system_music_folder(app: &AppHandle) -> Option<PathBuf> {
    app.path().audio_dir().ok().filter(|path| path.is_dir())
}

/// 既定の音楽フォルダ（OSの音楽フォルダの中の`Muspice`。OSの音楽フォルダが見つからなければNone）
fn default_music_folder(app: &AppHandle) -> Option<PathBuf> {
    system_music_folder(app).map(|folder| folder.join(DEFAULT_MUSIC_FOLDER_NAME))
}

/// 既定の音楽フォルダを作る
///
/// @returns 新しく作ったか（すでにあれば false）
fn create_default_folder(folder: &Path) -> AppResult<bool> {
    if folder.is_dir() {
        return Ok(false);
    }
    std::fs::create_dir(folder).map_err(|e| {
        AppError::Io(format!(
            "音楽フォルダ（{}）を作れませんでした: {}",
            folder.display(),
            e
        ))
    })?;
    Ok(true)
}

/// 音楽フォルダを変える前の、ライブラリの見通し
struct ReplacementPlan {
    /// 今の音楽フォルダと同じフォルダか
    is_current: bool,
    removed_track_count: u32,
    kept_track_count: u32,
    playlist_count: u32,
}

/// 音楽フォルダを`path`に変えた時に、外す曲・残る曲・消すプレイリストの数を求める
fn plan_replacement(conn: &Connection, path: &str) -> AppResult<ReplacementPlan> {
    let is_current = find_music_folder(conn)?.is_some_and(|folder| folder.path == path);
    let total = crate::repository::count_all_tracks(conn)?;
    let kept = crate::repository::count_tracks_under(conn, &track_path_prefix(path))?;
    Ok(ReplacementPlan {
        is_current,
        removed_track_count: total.saturating_sub(kept),
        kept_track_count: kept,
        playlist_count: crate::playlist::count_regular_playlists(conn)?,
    })
}

/// フォルダを音楽フォルダにし、ライブラリをそのフォルダの中身に置き換える（同期処理）
///
/// `is_default`なら、既定の音楽フォルダとして、なければ作る（確認を求める場合は作らない。音楽
/// フォルダにできなかった場合は、空のフォルダを残さない）。
fn change_music_folder(
    state: &AppState,
    folder: &Path,
    is_default: bool,
    mode: ChangeMode,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<MusicFolderChange> {
    let path = folder
        .to_str()
        .map(normalize_folder_path)
        .ok_or_else(|| AppError::Validation("フォルダの名前を扱えません".to_string()))?;

    // 自動の再スキャン・インポートと同時に曲を外したり登録したりしないよう、終わるまで待ってから始める
    let _scan = state.lock_library_scan();
    let plan = state.with_db(|db| plan_replacement(db, &path))?;
    if plan.is_current {
        return Ok(MusicFolderChange::Set {
            path,
            imported: None,
            replaced: None,
        });
    }
    let backup = match mode {
        ChangeMode::AskIfRemoving if plan.removed_track_count > 0 => {
            return Ok(MusicFolderChange::ConfirmReplace {
                path,
                removed_track_count: plan.removed_track_count,
                kept_track_count: plan.kept_track_count,
                playlist_count: plan.playlist_count,
            });
        }
        ChangeMode::AskIfRemoving => None,
        ChangeMode::Replace { backup } => Some(backup),
    };
    let replace = backup.is_some();
    let backup_path = match backup.flatten() {
        Some(write) => Some(state.with_db(|db| write(db))?),
        None => None,
    };

    let created = is_default && create_default_folder(folder)?;
    let applied = apply_change(state, &path, replace, on_progress);
    if created && applied.is_err() {
        let _ = std::fs::remove_dir(folder);
    }
    let (imported, removed_track_count, removed_playlist_count) = applied?;
    Ok(MusicFolderChange::Set {
        path,
        imported,
        replaced: replace.then(|| LibraryReplaced {
            removed_track_count,
            removed_playlist_count,
            backup_path: backup_path.map(|path| path.to_string_lossy().into_owned()),
        }),
    })
}

/// ライブラリを`path`の中身にする（`change_music_folder`の後半。呼び出し側がスキャンのロックを持つ）
///
/// 新しいフォルダの外の曲と、ほかのライブラリフォルダの記録（読み込まないフォルダを含む）を外し、
/// `delete_playlists`なら通常のプレイリストも消す。新しいフォルダが登録済みなら印を付け、なければ登録して
/// 中の曲を読み込む（登録済みの曲は読み飛ばし、記録を残す）。
///
/// @returns 読み込んだ結果・外した曲の数・消したプレイリストの数
fn apply_change(
    state: &AppState,
    path: &str,
    delete_playlists: bool,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<(Option<ImportResult>, u32, u32)> {
    let (removed_tracks, removed_playlists, registered) = state.with_db(|db| {
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        let removed_tracks =
            crate::repository::delete_tracks_outside(&tx, &track_path_prefix(path))?;
        let removed_playlists = if delete_playlists {
            crate::playlist::delete_regular_playlists(&tx)?
        } else {
            0
        };
        for folder in find_all_folders(&tx)? {
            if folder.path != path {
                crate::library_folder::delete_folder(&tx, &folder.id)?;
            }
        }
        let registered = find_folder_by_path(&tx, path)?;
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })?;
        let removed_tracks = u32::try_from(removed_tracks)
            .map_err(|_| AppError::Database("曲の数が多すぎます".to_string()))?;
        Ok((removed_tracks, removed_playlists, registered))
    })?;

    if let Some(registered) = registered {
        state.with_db(|db| crate::library_folder::set_music_folder(db, &registered.id))?;
        return Ok((None, removed_tracks, removed_playlists));
    }
    let (imported, folder_id) = import_folder_blocking(
        Path::new(path),
        DuplicateAction::Skip,
        state,
        on_progress,
        FolderRegistration::Always,
    )?;
    let folder_id = folder_id
        .ok_or_else(|| AppError::Database("音楽フォルダを登録できませんでした".to_string()))?;
    state.with_db(|db| crate::library_folder::set_music_folder(db, &folder_id))?;
    Ok((Some(imported), removed_tracks, removed_playlists))
}

/// 曲を書式に当てはめた、音楽フォルダからの場所（区切りは`/`）
fn template_examples(template: &Template, tracks: &[Track]) -> Vec<String> {
    tracks
        .iter()
        .map(|track| {
            template
                .components(track, Path::new(&track.file_path))
                .join("/")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::library_folder::{find_exclusions, register_folder};
    use crate::metadata::test_images::write_wav;
    use std::fs;

    fn test_state() -> AppState {
        AppState::new(crate::db::open_in_memory())
    }

    /// 確認を求める扱いで変える（外す曲がない時の、画面からの最初の操作）
    fn change(state: &AppState, folder: &Path) -> MusicFolderChange {
        change_music_folder(state, folder, false, ChangeMode::AskIfRemoving, &|_| {}).unwrap()
    }

    /// 確かめた後に置き換える（バックアップは書かない）
    fn replace(state: &AppState, folder: &Path) -> MusicFolderChange {
        change_music_folder(
            state,
            folder,
            false,
            ChangeMode::Replace { backup: None },
            &|_| {},
        )
        .unwrap()
    }

    fn music_folder_path(state: &AppState) -> Option<String> {
        state
            .with_db(|db| find_music_folder(db))
            .unwrap()
            .map(|folder| folder.path)
    }

    fn folder_paths(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| find_all_folders(db))
            .unwrap()
            .into_iter()
            .map(|folder| folder.path)
            .collect()
    }

    /// 登録してある曲の（ファイル名, 再生回数）を、名前の順に返す
    fn tracks(state: &AppState) -> Vec<(String, i64)> {
        state
            .with_db(|db| {
                let mut statement = db
                    .prepare("SELECT file_name, play_count FROM tracks ORDER BY file_name")
                    .unwrap();
                let rows = statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                Ok(rows)
            })
            .unwrap()
    }

    /// 通常のプレイリストを作り、登録してある曲をすべて入れる（ID を返す）
    fn create_playlist_with_all_tracks(state: &AppState, name: &str) -> String {
        state
            .with_db(|db| {
                let playlist = crate::playlist::create_playlist(db, name)?;
                let ids: Vec<String> = db
                    .prepare("SELECT id FROM tracks")
                    .unwrap()
                    .query_map([], |row| row.get(0))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                crate::playlist::add_tracks_to_playlist(db, &playlist.id, &ids)?;
                Ok(playlist.id)
            })
            .unwrap()
    }

    /// プレイリストの名前を、名前の順に返す
    fn playlist_names(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| {
                let mut statement = db
                    .prepare("SELECT name FROM playlists ORDER BY name")
                    .unwrap();
                let rows = statement
                    .query_map([], |row| row.get(0))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                Ok(rows)
            })
            .unwrap()
    }

    fn path_str(path: &Path) -> String {
        path.to_str().unwrap().to_string()
    }

    /// 既定の音楽フォルダは、なければ作って音楽フォルダにする（すでにあれば、そのまま使う）
    #[test]
    fn test_default_folder_is_created_and_set() {
        let dir = TempDir::new();
        let state = test_state();
        let folder = dir.path().join(DEFAULT_MUSIC_FOLDER_NAME);

        let result =
            change_music_folder(&state, &folder, true, ChangeMode::AskIfRemoving, &|_| {}).unwrap();

        assert!(matches!(result, MusicFolderChange::Set { .. }));
        assert!(folder.is_dir());
        assert_eq!(music_folder_path(&state), Some(path_str(&folder)));
        // 2回目は作らない（中のファイルも、そのまま）
        fs::write(folder.join("memo.txt"), b"memo").unwrap();
        assert!(!create_default_folder(&folder).unwrap());
        assert!(folder.join("memo.txt").exists());
    }

    /// 外す曲がある時は、確認を求め、何も変えない（既定の音楽フォルダも、まだ作らない）
    #[test]
    fn test_asks_before_removing_tracks() {
        let dir = TempDir::new();
        let old = dir.path().join("old");
        fs::create_dir_all(&old).unwrap();
        write_wav(&old.join("a.wav"));
        let state = test_state();
        change(&state, &old);
        create_playlist_with_all_tracks(&state, "Drive");
        let folder = dir.path().join(DEFAULT_MUSIC_FOLDER_NAME);

        let result =
            change_music_folder(&state, &folder, true, ChangeMode::AskIfRemoving, &|_| {}).unwrap();

        assert_eq!(
            result,
            MusicFolderChange::ConfirmReplace {
                path: path_str(&folder),
                removed_track_count: 1,
                kept_track_count: 0,
                playlist_count: 1,
            }
        );
        assert!(!folder.exists());
        assert_eq!(music_folder_path(&state), Some(path_str(&old)));
        assert_eq!(tracks(&state).len(), 1);
    }

    /// 置き換えると、新しいフォルダの外の曲・ほかのライブラリフォルダ・通常のプレイリストを外し、
    /// 新しいフォルダの曲を読み込む（自動プレイリストは残し、ファイルは消さない）
    #[test]
    fn test_replace_removes_tracks_folders_and_regular_playlists() {
        let dir = TempDir::new();
        let old = dir.path().join("old");
        let other = dir.path().join("other");
        let new = dir.path().join("new");
        for folder in [&old, &other, &new] {
            fs::create_dir_all(folder).unwrap();
        }
        write_wav(&old.join("a.wav"));
        write_wav(&new.join("b.wav"));
        let state = test_state();
        change(&state, &old);
        state
            .with_db(|db| register_folder(db, &path_str(&other), &[]))
            .unwrap();
        create_playlist_with_all_tracks(&state, "Drive");
        state
            .with_db(|db| {
                let folder = crate::playlist::create_playlist_folder(db, "Car")?;
                let smart = crate::playlist::create_smart_playlist(
                    db,
                    "自動",
                    &crate::smart_playlist::SmartRules::unreadable(),
                )?;
                crate::playlist::move_playlist(db, &smart.id, Some(&folder.id))
            })
            .unwrap();
        let backups = dir.path().join("backups");
        let write_backup = |db: &Connection| {
            let meta = BackupMeta {
                app_version: "test",
                settings_json: None,
            };
            create_backup_in(&backups, BackupKind::BeforeMusicFolderChange, db, &meta)
        };

        let result = change_music_folder(
            &state,
            &new,
            false,
            ChangeMode::Replace {
                backup: Some(&write_backup),
            },
            &|_| {},
        )
        .unwrap();

        let MusicFolderChange::Set {
            imported: Some(imported),
            replaced: Some(replaced),
            ..
        } = result
        else {
            panic!("置き換えていない: {:?}", result);
        };
        assert_eq!(imported.imported_count, 1);
        assert_eq!(replaced.removed_track_count, 1);
        assert_eq!(replaced.removed_playlist_count, 1);
        let backup = PathBuf::from(replaced.backup_path.unwrap());
        assert!(backup.is_file() && backup.starts_with(&backups));
        assert_eq!(tracks(&state), [("b.wav".to_string(), 0)]);
        assert_eq!(folder_paths(&state), [path_str(&new)]);
        assert_eq!(music_folder_path(&state), Some(path_str(&new)));
        // 自動プレイリストと、それが入っているフォルダは残る
        assert_eq!(playlist_names(&state), ["自動"]);
        let folders = state
            .with_db(|db| crate::playlist::get_playlist_folders(db))
            .unwrap();
        assert_eq!(folders.len(), 1);
        assert!(old.join("a.wav").exists());
    }

    /// 前のライブラリフォルダの中のフォルダに変えると、中の曲は記録（再生回数など）を残す
    #[test]
    fn test_replace_keeps_tracks_inside_the_new_folder() {
        let dir = TempDir::new();
        let inner = dir.path().join(DEFAULT_MUSIC_FOLDER_NAME);
        fs::create_dir_all(&inner).unwrap();
        write_wav(&dir.path().join("x.wav"));
        write_wav(&inner.join("y.wav"));
        let state = test_state();
        change(&state, dir.path());
        state
            .with_db(|db| {
                db.execute("UPDATE tracks SET play_count = 3", [])
                    .map_err(|e| AppError::Database(e.to_string()))
            })
            .unwrap();
        // `~/Music`の中の`Muspice`のように、ライブラリフォルダの中のフォルダも選べる
        assert!(matches!(
            change(&state, &inner),
            MusicFolderChange::ConfirmReplace {
                removed_track_count: 1,
                kept_track_count: 1,
                ..
            }
        ));

        replace(&state, &inner);

        assert_eq!(tracks(&state), [("y.wav".to_string(), 3)]);
        assert_eq!(folder_paths(&state), [path_str(&inner)]);
        assert_eq!(music_folder_path(&state), Some(path_str(&inner)));
    }

    /// 外す曲がない時（新しいフォルダが今の曲をすべて含む時）は、確かめずに音楽フォルダにし、
    /// プレイリストを残す
    #[test]
    fn test_outer_folder_keeps_playlists_without_confirmation() {
        let dir = TempDir::new();
        let inner = dir.path().join("Downloads");
        fs::create_dir_all(&inner).unwrap();
        write_wav(&inner.join("a.wav"));
        let state = test_state();
        change(&state, &inner);
        create_playlist_with_all_tracks(&state, "Drive");

        let result = change(&state, dir.path());

        let MusicFolderChange::Set {
            imported: Some(_),
            replaced: None,
            ..
        } = result
        else {
            panic!("確かめずに音楽フォルダにしていない: {:?}", result);
        };
        assert_eq!(tracks(&state), [("a.wav".to_string(), 0)]);
        assert_eq!(folder_paths(&state), [path_str(dir.path())]);
        assert_eq!(playlist_names(&state), ["Drive"]);
    }

    /// 空のフォルダも、ライブラリフォルダとして登録して音楽フォルダにする
    #[test]
    fn test_empty_folder_becomes_music_folder() {
        let dir = TempDir::new();
        let state = test_state();

        let result = change(&state, dir.path());

        assert_eq!(
            result,
            MusicFolderChange::Set {
                path: path_str(dir.path()),
                imported: Some(ImportResult::default()),
                replaced: None,
            }
        );
        assert_eq!(folder_paths(&state), [path_str(dir.path())]);
        assert_eq!(music_folder_path(&state), Some(path_str(dir.path())));
    }

    /// OSの音楽フォルダのような構成では、曲を読み込み、ほかのアプリのフォルダは除外の一覧に入れる
    #[test]
    fn test_music_folder_imports_tracks_and_records_app_folder_exclusions() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        for path in [
            "Artist/Album/01.wav",
            "Music/Media.localized/Music/a.wav",
            "MusicBee/Podcasts/b.wav",
            "GarageBand/c.wav",
        ] {
            let path = music.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            write_wav(&path);
        }
        fs::create_dir_all(music.join("Music/Music Library.musiclibrary")).unwrap();
        let state = test_state();
        let progress = std::cell::RefCell::new(Vec::new());

        let result =
            change_music_folder(&state, &music, false, ChangeMode::AskIfRemoving, &|event| {
                progress.borrow_mut().push(event.current_file);
            })
            .unwrap();

        let MusicFolderChange::Set {
            imported: Some(imported),
            ..
        } = result
        else {
            panic!("音楽フォルダにしていない: {:?}", result);
        };
        assert_eq!(imported.imported_count, 1);
        assert_eq!(*progress.borrow(), ["01.wav"]);
        assert_eq!(music_folder_path(&state), Some(path_str(&music)));
        let exclusions: Vec<String> = state
            .with_db(|db| {
                let folder = find_music_folder(db)?.unwrap();
                find_exclusions(db, &folder.id)
            })
            .unwrap()
            .into_iter()
            .map(|exclusion| exclusion.path)
            .collect();
        assert_eq!(exclusions, ["GarageBand", "Music", "MusicBee"]);
    }

    /// 登録済みのライブラリフォルダを選ぶと、印を付けるだけで読み込み直さない（ほかのフォルダの記録は外す）
    #[test]
    fn test_registered_folder_only_moves_the_mark() {
        let dir = TempDir::new();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        // 登録した後に置いた曲は、印を付けるだけでは読まない
        let state = test_state();
        change(&state, &first);
        state
            .with_db(|db| register_folder(db, second.to_str().unwrap(), &[]))
            .unwrap();
        write_wav(&second.join("later.wav"));

        let result = change(&state, &second);

        assert_eq!(
            result,
            MusicFolderChange::Set {
                path: path_str(&second),
                imported: None,
                replaced: None,
            }
        );
        assert_eq!(music_folder_path(&state), Some(path_str(&second)));
        assert_eq!(folder_paths(&state), [path_str(&second)]);
        assert!(tracks(&state).is_empty());
    }

    /// 今の音楽フォルダを選んだ場合は、何もしない
    #[test]
    fn test_current_music_folder_changes_nothing() {
        let dir = TempDir::new();
        let state = test_state();
        change(&state, dir.path());
        state
            .with_db(|db| register_folder(db, "/Volumes/Other", &[]))
            .unwrap();

        let result = replace(&state, dir.path());

        assert_eq!(
            result,
            MusicFolderChange::Set {
                path: path_str(dir.path()),
                imported: None,
                replaced: None,
            }
        );
        assert_eq!(folder_paths(&state).len(), 2);
    }

    /// 預かった音楽フォルダは、取り出すと手放す
    #[test]
    fn test_pending_music_folder_is_taken_once() {
        let pending = PendingMusicFolder::default();
        pending.hold(PendingChange {
            folder: PathBuf::from("/m"),
            is_default: false,
        });
        assert!(pending.take().is_some());
        assert!(pending.take().is_none());
    }

    #[test]
    fn test_template_examples_are_relative_paths() {
        let track = |file_path: &str, album_artist: Option<&str>, disc: Option<i32>| Track {
            id: "a1000000-0000-4000-8000-000000000001".to_string(),
            file_path: file_path.to_string(),
            file_name: file_path.rsplit('/').next().unwrap().to_string(),
            title: Some("Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: album_artist.map(str::to_string),
            genre: None,
            year: None,
            track_number: Some(3),
            disc_number: disc,
            duration: Some(200),
            file_size: 4,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: crate::models::ReplayGain::default(),
            sort_tags: Default::default(),
            is_missing: false,
        };
        let template = Template::parse(crate::settings::DEFAULT_MUSIC_FOLDER_TEMPLATE).unwrap();

        assert_eq!(
            template_examples(
                &template,
                &[
                    track("/m/a.flac", Some("Various"), Some(1)),
                    track("/m/b.mp3", None, None),
                ]
            ),
            ["Various/Album/1-03 Song.flac", "Artist/Album/03 Song.mp3"]
        );
    }
}
