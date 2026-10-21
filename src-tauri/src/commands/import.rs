//! 音楽ファイルのインポート関連コマンド
//!
//! 取り込み方は4つあり、どれも同じ処理（`import_audio_files`）で曲を登録する（ADR-050）。
//!
//! - フォルダを選ぶ（`import_folder`）: 中の曲をすべて登録し、フォルダをライブラリフォルダとして記録する
//! - ファイルを選ぶ（`import_files`）: 選んだ曲だけを登録する（フォルダは記録しない）
//! - ウィンドウへドロップする（`import_dropped_files`。macOS）: ファイルは曲だけ、フォルダはフォルダとして
//! - 「このアプリで開く」・Dockのアイコンへのドロップ（`opened_files`）: 曲だけを登録して、再生する
//!
//! 音楽フォルダを設定し、設定「取り込む曲を音楽フォルダへコピーする」がオンの時は、取り込む曲を
//! 音楽フォルダへ書式どおりにコピーしてから登録する（ADR-069。`music_folder_copy`）。ファイルは設定に
//! 従ってコピーし、ライブラリフォルダの外のフォルダを取り込む時は、取り込まずに預かって、参照するか
//! コピーするかを画面に選ばせる（`ImportOutcome::ChooseFolderMode` → `import_pending_folders`）。

use super::{file_dialog, pick_folder, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::{ImportPhase, ImportProgress, LibraryChanged};
use crate::file_organize::Template;
use crate::library::{
    DuplicateAction, ImportResult, ScannedFolder, get_default_title, get_file_format,
    get_file_modified_at, get_file_size, is_supported_audio_file, scan_directory,
    supported_extensions, to_count,
};
use crate::library_folder::{ImportExclusions, find_all_folders};
use crate::library_sync::LibrarySync;
use crate::metadata::extract_all_file_info;
use crate::models::Track;
use crate::music_folder_copy::{
    COPY_BUFFER_SIZE, CopySource, PlannedCopy, copy_files, copy_folder_images, remove_copy,
};
use crate::settings::{Language, SettingsState};
use crate::state::AppState;
use crate::track_relink::MissingTrackIndex;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use uuid::Uuid;

/// コピーの後に残す、音楽フォルダのドライブの空き容量（バイト）
const SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

/// 取り込みを始めた結果
#[derive(Debug, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportOutcome {
    /// 取り込んだ
    Imported { result: ImportResult },
    /// フォルダを、参照するか音楽フォルダへコピーするかを選ぶ（選ぶまで取り込まない。選んだら
    /// `import_pending_folders`、やめたら`cancel_pending_import`）
    ChooseFolderMode { pending: PendingFolderImport },
    /// 音楽フォルダが見つからないため、取り込まなかった（外付けドライブが外れているなど）
    MusicFolderMissing { path: String },
    /// 音楽フォルダのドライブの空き容量が足りないため、取り込まなかった
    NotEnoughSpace {
        #[serde(rename = "requiredBytes")]
        #[specta(type = specta_typescript::Number)]
        required_bytes: i64,
        #[serde(rename = "freeBytes")]
        #[specta(type = specta_typescript::Number)]
        free_bytes: i64,
    },
}

/// 参照するかコピーするかを選ぶ、取り込むフォルダ（画面に出す）
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingFolderImport {
    /// 取り込むフォルダ
    pub folders: Vec<String>,
    /// 取り込む曲の数（フォルダの中の曲と、一緒にドロップしたファイル）と、大きさの合計（バイト）
    pub track_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub total_bytes: i64,
    /// 音楽フォルダへコピーする曲の数と、大きさの合計（ライブラリフォルダの中の曲・登録済みの曲は、
    /// コピーしない）
    pub copy_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub copy_bytes: i64,
    /// 音楽フォルダ
    pub music_folder: String,
    /// 音楽フォルダが今もあるか（外付けドライブが外れている場合などはfalse）
    pub music_folder_exists: bool,
    /// 音楽フォルダのドライブの空き容量（バイト。取得できなければnull）
    #[specta(type = Option<specta_typescript::Number>)]
    pub free_bytes: Option<i64>,
}

/// 預かったフォルダの取り込み方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderImportMode {
    /// その場から読む（フォルダをライブラリフォルダとして記録する）
    Reference,
    /// 音楽フォルダへコピーする（フォルダは記録しない）
    Copy,
}

/// 参照かコピーかを選ぶまで預かっている取り込み（Tauriの管理状態。多くても1つ）
#[derive(Default)]
pub struct PendingImport(Mutex<Option<PendingTargets>>);

/// 預かっている取り込みの対象
struct PendingTargets {
    files: Vec<PathBuf>,
    folders: Vec<ScannedImportFolder>,
}

impl PendingImport {
    fn hold(&self, targets: PendingTargets) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = Some(targets);
        }
    }

    fn take(&self) -> Option<PendingTargets> {
        self.0.lock().ok().and_then(|mut pending| pending.take())
    }
}

/// フォルダを選ぶダイアログを開き、選んだフォルダの音楽ファイルをインポートする
///
/// ダイアログはRust側で開く（WebViewからパスを受け取らない。ADR-065）。中の曲をすべて登録し、
/// フォルダをライブラリフォルダとして記録する。取り込む曲を音楽フォルダへコピーする設定で、
/// ライブラリフォルダの外のフォルダを選んだ時は、取り込まずに預かる（`ChooseFolderMode`）。
///
/// @returns 結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn import_folder(
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<Option<ImportOutcome>> {
    // ダイアログ・スキャン・メタデータ抽出・バッチ書き込みはブロッキング処理用スレッドで行う
    run_blocking(move || {
        let title = match crate::menu::current_language(&app_handle) {
            Language::Ja => "インポートするフォルダを選択",
            Language::En => "Select a folder to import",
        };
        let Some(folder) = pick_folder(file_dialog(&app_handle).set_title(title))? else {
            return Ok(None);
        };
        let targets = ImportTargets {
            files: Vec::new(),
            folders: vec![folder],
        };
        start_import(&app_handle, targets, duplicate_action).map(Some)
    })
    .await
}

/// ファイルを選ぶダイアログを開き、選んだ音楽ファイルをインポートする
///
/// ダイアログはRust側で開く（WebViewへパスを渡さない）。選んだ曲だけを登録し、ファイルのある
/// フォルダは、ライブラリフォルダとして記録しない（再スキャンの対象にならない）。取り込む曲を
/// 音楽フォルダへコピーする設定なら、コピーしてから登録する。
///
/// @returns 結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn import_files(
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<Option<ImportOutcome>> {
    run_blocking(move || {
        let english = crate::menu::current_language(&app_handle) == crate::settings::Language::En;
        let (title, filter) = if english {
            ("Select the music files to import", "Music files")
        } else {
            ("インポートする音楽ファイルを選択", "音楽ファイル")
        };
        let Some(files) = file_dialog(&app_handle)
            .set_title(title)
            .add_filter(filter, supported_extensions())
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        let paths: Vec<PathBuf> = files
            .into_iter()
            .filter_map(|file| file.into_path().ok())
            .collect();
        start_import(
            &app_handle,
            ImportTargets::from_paths(&paths),
            duplicate_action,
        )
        .map(Some)
    })
    .await
}

/// ウィンドウへドロップされたファイル・フォルダをインポートする（macOS）
///
/// WebViewは、ドロップされたファイルのパスを知らない（HTML5のドロップでは得られず、Tauriの
/// ファイルドロップは曲のドラッグ&ドロップのために無効にしている。ADR-018）。WebViewからは
/// 「ドロップされた」ことと名前だけを受け取り、パスは、OSのドラッグ用のペーストボードから
/// Rust側で読む（`dropped_files`）。ファイルは曲だけを登録し、フォルダは`import_folder`と同じく
/// ライブラリフォルダとして記録する。登録済みのファイルはスキップする。取り込む曲を音楽フォルダへ
/// コピーする設定で、ライブラリフォルダの外のフォルダがあれば、取り込まずに預かる
/// （`ChooseFolderMode`。選んだ方法で、一緒にドロップしたファイルも取り込む）。
#[tauri::command]
#[specta::specta]
pub async fn import_dropped_files(
    names: Vec<String>,
    app_handle: AppHandle,
) -> AppResult<ImportOutcome> {
    run_blocking(move || {
        let paths = crate::dropped_files::take_dropped_paths(
            &app_handle,
            &app_handle.state::<crate::dropped_files::DroppedFiles>(),
            &names,
        )?;
        start_import(
            &app_handle,
            ImportTargets::from_paths(&paths),
            DuplicateAction::Skip,
        )
    })
    .await
}

/// 預かっているフォルダ（`ChooseFolderMode`）を、選んだ方法で取り込む
///
/// 音楽フォルダへコピーできなかった場合（`MusicFolderMissing`・`NotEnoughSpace`）は、預かったままにする
/// （参照して取り込むこともできる）。預かっているものがなければ`NOT_FOUND`。
#[tauri::command]
#[specta::specta]
pub async fn import_pending_folders(
    mode: FolderImportMode,
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<ImportOutcome> {
    run_blocking(move || {
        let pending = app_handle.state::<PendingImport>();
        let targets = pending
            .take()
            .ok_or_else(|| AppError::NotFound("取り込むフォルダがありません".to_string()))?;
        let copy = match mode {
            FolderImportMode::Reference => None,
            // 選んだ時に設定がオフになっていても、音楽フォルダがあればコピーする
            FolderImportMode::Copy => Some(
                music_folder_destination(&app_handle.state::<AppState>(), &app_handle)?
                    .ok_or_else(|| {
                        AppError::NotFound("音楽フォルダが設定されていません".to_string())
                    })?,
            ),
        };
        let outcome = run_import(
            &app_handle,
            &targets.files,
            &targets.folders,
            duplicate_action,
            copy.as_ref(),
        );
        // 失敗した場合は手放す（画面も、預かったものはなくなったとして扱う）
        if matches!(
            outcome,
            Ok(ImportOutcome::MusicFolderMissing { .. } | ImportOutcome::NotEnoughSpace { .. })
        ) {
            pending.hold(targets);
        }
        outcome
    })
    .await
}

/// 預かっているフォルダの取り込みをやめる（預かっていなければ、何もしない）
#[tauri::command]
#[specta::specta]
pub fn cancel_pending_import(pending: tauri::State<'_, PendingImport>) {
    pending.take();
}

/// 「このアプリで開く」などで開かれ、インポートが済んだ曲を受け取る（受け取った分は、手放す）
///
/// フロントエンドは、起動した時と、`TracksOpened`が届いた時に呼び、受け取った曲を再生する。
#[tauri::command]
#[specta::specta]
pub fn take_opened_files(
    opened: tauri::State<'_, crate::opened_files::OpenedFilesState>,
) -> crate::opened_files::OpenedFiles {
    opened.take()
}

/// 取り込みを始める（ダイアログ・ドロップで選んだものの、同期処理）
///
/// 取り込む曲を音楽フォルダへコピーする設定で、ライブラリフォルダの外のフォルダがあれば、取り込まずに
/// 預かる（`ChooseFolderMode`）。それ以外は、ファイルは設定に従い、フォルダは参照して取り込む
/// （ライブラリフォルダの中のフォルダは、どの場合も参照する）。取り込めるものが1つもなければエラー。
fn start_import(
    app_handle: &AppHandle,
    targets: ImportTargets,
    duplicate_action: DuplicateAction,
) -> AppResult<ImportOutcome> {
    if targets.is_empty() {
        return Err(AppError::Validation(
            "インポートできる音楽ファイルがありません".to_string(),
        ));
    }
    let state = app_handle.state::<AppState>();
    let copy = copy_destination(&state, app_handle)?;
    let folders = targets
        .folders
        .iter()
        .map(|folder| ScannedImportFolder::scan(&state, folder))
        .collect::<AppResult<Vec<_>>>()?;

    if let Some(copy) = &copy {
        let library_folders = state.with_db(|db| find_all_folders(db))?;
        let is_outside_library = |folder: &PathBuf| {
            !library_folders
                .iter()
                .any(|library| folder.starts_with(&library.path))
        };
        if targets.folders.iter().any(is_outside_library) {
            let summary = summarize_pending(&state, &targets.files, &folders, copy)?;
            // コピーする曲がなければ（空のフォルダ・すでにライブラリにある曲だけ）、選ばせずに参照する
            if summary.copy_count == 0 {
                return run_import(app_handle, &targets.files, &folders, duplicate_action, None);
            }
            app_handle.state::<PendingImport>().hold(PendingTargets {
                files: targets.files,
                folders,
            });
            return Ok(ImportOutcome::ChooseFolderMode { pending: summary });
        }
    }
    run_import(
        app_handle,
        &targets.files,
        &folders,
        duplicate_action,
        copy.as_ref(),
    )
}

/// 取り込み、コマンドの結果にする
fn run_import(
    app_handle: &AppHandle,
    files: &[PathBuf],
    folders: &[ScannedImportFolder],
    duplicate_action: DuplicateAction,
    copy: Option<&CopyDestination>,
) -> AppResult<ImportOutcome> {
    Ok(
        match import_and_notify(app_handle, files, folders, duplicate_action, copy)? {
            ImportRun::Imported(report) => ImportOutcome::Imported {
                result: report.result,
            },
            ImportRun::MusicFolderMissing(path) => ImportOutcome::MusicFolderMissing { path },
            ImportRun::NotEnoughSpace {
                required_bytes,
                free_bytes,
            } => ImportOutcome::NotEnoughSpace {
                required_bytes: to_bytes(required_bytes),
                free_bytes: to_bytes(free_bytes),
            },
        },
    )
}

/// 取り込み、ライブラリの変更を知らせる（同期処理。「このアプリで開く」でも使う）
///
/// 音楽ファイルは曲だけを登録し、フォルダは中の曲を登録してライブラリフォルダとして記録する。
/// `copy`を渡した場合は、音楽フォルダへコピーしてから登録する（フォルダは記録しない）。
pub(crate) fn import_and_notify(
    app_handle: &AppHandle,
    files: &[PathBuf],
    folders: &[ScannedImportFolder],
    duplicate_action: DuplicateAction,
    copy: Option<&CopyDestination>,
) -> AppResult<ImportRun> {
    let state = app_handle.state::<AppState>();
    let run = import_scanned(
        &state,
        files,
        folders,
        duplicate_action,
        copy,
        &|progress| {
            if let Err(e) = progress.emit(app_handle) {
                log::warn!("進捗イベントの送信に失敗しました: {}", e);
            }
        },
    )?;

    if matches!(run, ImportRun::Imported(_)) {
        // 記録したフォルダを監視する（監視が有効な場合）
        app_handle
            .state::<LibrarySync>()
            .refresh_watches(app_handle);
        // 曲の一覧と、設定ウィンドウのライブラリフォルダの一覧に反映させる
        if let Err(e) = LibraryChanged.emit(app_handle) {
            log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
        }
    }
    Ok(run)
}

/// 取り込む曲を、音楽フォルダへコピーする時の宛先
pub(crate) struct CopyDestination {
    /// 音楽フォルダ
    pub folder: PathBuf,
    /// 音楽フォルダの書式
    pub template: Template,
}

/// 取り込む曲のコピーの宛先（設定「取り込む曲を音楽フォルダへコピーする」がオフか、音楽フォルダが
/// 未設定ならNone）
pub(crate) fn copy_destination(
    state: &AppState,
    app_handle: &AppHandle,
) -> AppResult<Option<CopyDestination>> {
    if !app_handle
        .state::<SettingsState>()
        .get()?
        .copy_to_music_folder
    {
        return Ok(None);
    }
    music_folder_destination(state, app_handle)
}

/// 音楽フォルダと、その書式（音楽フォルダが未設定ならNone）
fn music_folder_destination(
    state: &AppState,
    app_handle: &AppHandle,
) -> AppResult<Option<CopyDestination>> {
    let Some(folder) = state.with_db(|db| crate::library_folder::find_music_folder(db))? else {
        return Ok(None);
    };
    let settings = app_handle.state::<SettingsState>().get()?;
    Ok(Some(CopyDestination {
        folder: PathBuf::from(folder.path),
        template: Template::parse(&settings.music_folder_template)?,
    }))
}

/// 預かるフォルダの、画面に出す内容（曲の数・大きさと、音楽フォルダの空き容量）
fn summarize_pending(
    state: &AppState,
    files: &[PathBuf],
    folders: &[ScannedImportFolder],
    copy: &CopyDestination,
) -> AppResult<PendingFolderImport> {
    let (known_paths, library_folders) = state.with_db(|db| {
        Ok((
            crate::repository::find_all_file_paths(db)?,
            find_all_folders(db)?,
        ))
    })?;
    let mut summary = PendingFolderImport {
        folders: folders
            .iter()
            .map(|folder| folder.path.to_string_lossy().into_owned())
            .collect(),
        track_count: 0,
        total_bytes: 0,
        copy_count: 0,
        copy_bytes: 0,
        music_folder: copy.folder.to_string_lossy().into_owned(),
        music_folder_exists: copy.folder.is_dir(),
        free_bytes: fs4::available_space(&copy.folder).ok().map(to_bytes),
    };
    for path in unique_sources(files, folders) {
        let size = get_file_size(path).unwrap_or(0);
        summary.track_count += 1;
        summary.total_bytes += size;
        if !is_in_library(path, &known_paths, &library_folders) {
            summary.copy_count += 1;
            summary.copy_bytes += size;
        }
    }
    Ok(summary)
}

/// 取り込む曲（ファイルと、フォルダの中の曲。重複は除く）
fn unique_sources<'a>(
    files: &'a [PathBuf],
    folders: &'a [ScannedImportFolder],
) -> Vec<&'a PathBuf> {
    let mut seen: HashSet<&PathBuf> = HashSet::new();
    files
        .iter()
        .chain(
            folders
                .iter()
                .flat_map(|folder| folder.scanned.audio_files.iter()),
        )
        .filter(|path| seen.insert(path))
        .collect()
}

/// 曲が、すでにライブラリにあるか（登録済みか、ライブラリフォルダの中のファイル）
fn is_in_library(
    path: &Path,
    known_paths: &HashSet<String>,
    library_folders: &[crate::library_folder::FolderRecord],
) -> bool {
    path.to_str().is_some_and(|path| known_paths.contains(path))
        || library_folders
            .iter()
            .any(|folder| path.starts_with(&folder.path))
}

/// バイト数を、画面に渡す数にする
fn to_bytes(bytes: u64) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
}

/// インポートする対象（音楽ファイルと、フォルダ）
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ImportTargets {
    /// 音楽ファイル（曲だけを登録する）
    pub files: Vec<PathBuf>,
    /// フォルダ（中の曲を登録し、ライブラリフォルダとして記録する）
    pub folders: Vec<PathBuf>,
}

impl ImportTargets {
    /// パスを、音楽ファイルとフォルダに分ける（どちらでもないもの・重複は除く）
    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let mut targets = Self::default();
        for path in paths {
            if path.is_dir() {
                if !targets.folders.contains(path) {
                    targets.folders.push(path.clone());
                }
            } else if path.is_file()
                && is_supported_audio_file(path)
                && !targets.files.contains(path)
            {
                targets.files.push(path.clone());
            }
        }
        targets
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }
}

/// 取り込むフォルダ（走査した結果）
pub(crate) struct ScannedImportFolder {
    pub path: PathBuf,
    exclusions: ImportExclusions,
    scanned: ScannedFolder,
}

impl ScannedImportFolder {
    /// フォルダを走査する
    ///
    /// 読み込まないフォルダ（登録済みのフォルダの除外、新しく登録するフォルダなら直下の既知のアプリの
    /// フォルダ）は、中を読まない（ADR-062）。
    pub fn scan(state: &AppState, path: &Path) -> AppResult<Self> {
        // 既定の除外はファイルシステムを読むため、DBロックの外で求める
        let exclusions = match path.to_str() {
            Some(folder_path) => {
                let defaults = crate::library_folder::default_exclusions(path);
                state.with_db(|db| {
                    crate::library_folder::import_exclusions(db, folder_path, &defaults)
                })?
            }
            None => ImportExclusions::default(),
        };
        let scanned = scan_directory(path, &exclusions.scan)?;
        Ok(Self {
            path: path.to_path_buf(),
            exclusions,
            scanned,
        })
    }
}

/// 取り込みの結果
pub(crate) enum ImportRun {
    Imported(ImportReport),
    /// 音楽フォルダが見つからないため、取り込まなかった
    MusicFolderMissing(String),
    /// 音楽フォルダのドライブの空き容量が足りないため、取り込まなかった
    NotEnoughSpace {
        required_bytes: u64,
        free_bytes: u64,
    },
}

/// 取り込んだ結果
#[derive(Debug, Default)]
pub(crate) struct ImportReport {
    pub result: ImportResult,
    /// 音楽フォルダへコピーした曲・コピー済みだった曲の、元のパス → ライブラリに登録したパス
    /// （ほかの曲は、元のパスのまま登録する）
    pub placed: HashMap<PathBuf, PathBuf>,
}

/// 音楽ファイルとフォルダをインポートする（同期処理。参照して取り込む）
///
/// `on_progress`には、ファイルを1つ読むたびに進み具合を渡す（ファイルの分と、フォルダごとに、
/// 件数を数え直す）。
pub(super) fn import_targets(
    state: &AppState,
    targets: &ImportTargets,
    duplicate_action: DuplicateAction,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportResult> {
    let folders = targets
        .folders
        .iter()
        .map(|folder| ScannedImportFolder::scan(state, folder))
        .collect::<AppResult<Vec<_>>>()?;
    match import_scanned(
        state,
        &targets.files,
        &folders,
        duplicate_action,
        None,
        on_progress,
    )? {
        ImportRun::Imported(report) => Ok(report.result),
        // コピーしない取り込みでは、起きない
        _ => Err(AppError::Io("取り込めませんでした".to_string())),
    }
}

/// 音楽ファイルと、走査したフォルダをインポートする（同期処理）
///
/// `copy`を渡した場合は、音楽フォルダへコピーしてから登録する（`import_with_copy`）。
fn import_scanned(
    state: &AppState,
    files: &[PathBuf],
    folders: &[ScannedImportFolder],
    duplicate_action: DuplicateAction,
    copy: Option<&CopyDestination>,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportRun> {
    // 自動の再スキャンと同じファイルを同時に登録しないよう、終わるまで待ってから始める
    let _scan = state.lock_library_scan();
    if let Some(copy) = copy {
        return import_with_copy(state, files, folders, duplicate_action, copy, on_progress);
    }
    let mut result = ImportResult::default();
    if !files.is_empty() {
        result.merge(import_audio_files(
            files,
            duplicate_action,
            state,
            on_progress,
        )?);
    }
    for folder in folders {
        let (folder_result, _) = import_scanned_folder(
            folder,
            duplicate_action,
            state,
            on_progress,
            FolderRegistration::IfTracksFound,
        )?;
        result.merge(folder_result);
    }
    Ok(ImportRun::Imported(ImportReport {
        result,
        placed: HashMap::new(),
    }))
}

/// 取り込む曲を音楽フォルダへコピーしてから登録する（同期処理。呼び出し側が`lock_library_scan`を持つ）
///
/// - ライブラリフォルダの中の曲・登録済みの曲は、コピーせず、参照して取り込む（登録済みの曲は、
///   `duplicate_action`に従う）。フォルダは、ライブラリフォルダとして記録しない
/// - コピーする前に、音楽フォルダがあることと、空き容量を確かめる（どちらかがだめなら、何も取り込まない。
///   黙って参照に切り替えない）
/// - コピー先に同じ名前のファイルがある曲・同じコピー先になる曲は、取り込まずに結果で知らせる
///   （`not_copied`）。コピー先に同じ内容のファイルがある曲は、コピー先を取り込む
fn import_with_copy(
    state: &AppState,
    files: &[PathBuf],
    folders: &[ScannedImportFolder],
    duplicate_action: DuplicateAction,
    copy: &CopyDestination,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportRun> {
    if !copy.folder.is_dir() {
        return Ok(ImportRun::MusicFolderMissing(
            copy.folder.to_string_lossy().into_owned(),
        ));
    }
    let mut result = ImportResult::default();
    for folder in folders {
        let unreadable = folder.scanned.unreadable_dir_errors();
        result.error_count += to_count(unreadable.len())?;
        result.errors.extend(unreadable);
    }

    let (known_paths, library_folders) = state.with_db(|db| {
        Ok((
            crate::repository::find_all_file_paths(db)?,
            find_all_folders(db)?,
        ))
    })?;
    let (mut referenced, to_copy): (Vec<PathBuf>, Vec<PathBuf>) = unique_sources(files, folders)
        .into_iter()
        .cloned()
        .partition(|path| is_in_library(path, &known_paths, &library_folders));

    // 1. コピー先を決めるために、曲を読む
    let total = u32::try_from(to_copy.len())
        .map_err(|_| AppError::Validation("ファイル数が多すぎます".to_string()))?;
    let mut sources = Vec::with_capacity(to_copy.len());
    for (index, path) in (1..).zip(&to_copy) {
        on_progress(ImportProgress {
            phase: ImportPhase::Reading,
            current: index,
            total,
            current_file: display_file_name(path),
        });
        match create_track_from_file(path) {
            Ok(track) => sources.push(CopySource {
                path: path.clone(),
                track,
            }),
            Err(e) => {
                result.errors.push(format!("{}: {}", path.display(), e));
                result.error_count += 1;
            }
        }
    }
    let plan = crate::music_folder_copy::plan(sources, &copy.folder, &copy.template, &known_paths);

    // 2. 空き容量を確かめる（足りなければ、何も取り込まない）
    let required_bytes = plan.required_bytes();
    if required_bytes > 0 {
        let free_bytes = fs4::available_space(&copy.folder).map_err(|e| {
            AppError::Io(format!(
                "音楽フォルダのドライブの空き容量を取得できません: {}",
                e
            ))
        })?;
        if required_bytes.saturating_add(SPACE_MARGIN_BYTES) > free_bytes {
            return Ok(ImportRun::NotEnoughSpace {
                required_bytes,
                free_bytes,
            });
        }
    }

    // 3. ライブラリにある曲と、コピー先に同じ内容のファイルがある曲は、参照して取り込む
    let mut placed = HashMap::new();
    for (source, target) in &plan.already_copied {
        referenced.push(target.clone());
        placed.insert(source.clone(), target.clone());
    }
    if !referenced.is_empty() {
        result.merge(import_audio_files(
            &referenced,
            duplicate_action,
            state,
            on_progress,
        )?);
    }

    // 4. コピーして登録する
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let copied = copy_and_register(&plan.copies, &copy.folder, state, on_progress, &mut result)?;
    for planned in &copied {
        placed.insert(planned.source.clone(), planned.target.clone());
    }
    copy_folder_images(&copied, &mut buffer);
    // 見つからない曲に結び付けた曲は、アルバムアートを読み直させる
    if result.relinked_count > 0
        && let Ok(mut cache) = state.album_art_cache.lock()
    {
        cache.clear();
    }
    result.copied_count = to_count(copied.len())?;
    result.not_copied = plan.not_copied;
    Ok(ImportRun::Imported(ImportReport { result, placed }))
}

/// 計画どおりに曲をコピーし、コピー先の曲を登録する（同期処理）
///
/// コピーはDBロックの外で行い、登録は50件ずつ、1つのトランザクションで行う。登録できなかった曲の
/// コピーは消す（取り込まなかった曲のファイルを、音楽フォルダに残さない）。コピー先の曲が、見つからない
/// 曲と同じ曲なら、新しい曲として登録せず、その曲に結び付ける（`relinked_count`）。
/// @returns コピーして登録した曲
fn copy_and_register<'a>(
    copies: &'a [PlannedCopy],
    music_folder: &Path,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
    result: &mut ImportResult,
) -> AppResult<Vec<&'a PlannedCopy>> {
    const BATCH_SIZE: usize = 50;
    let total = u32::try_from(copies.len())
        .map_err(|_| AppError::Validation("ファイル数が多すぎます".to_string()))?;
    let mut missing_tracks = state.with_db(|db| {
        Ok(MissingTrackIndex::new(
            crate::repository::find_missing_tracks(db)?,
        ))
    })?;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let mut registered = Vec::with_capacity(copies.len());
    let mut processed: u32 = 0;

    for chunk in copies.chunks(BATCH_SIZE) {
        // 1. ロック外: コピーする（コピーした曲, 歌詞ファイルもコピーしたか, 登録する曲, 書き込み方,
        //    ファイルの更新日時）
        let mut pending: Vec<(&PlannedCopy, bool, Track, ImportAction, Option<i64>)> = Vec::new();
        for planned in chunk {
            processed += 1;
            on_progress(ImportProgress {
                phase: ImportPhase::Copying,
                current: processed,
                total,
                current_file: display_file_name(&planned.source),
            });
            let source = planned.source.display();
            let (copied, target_path) = match copy_files(planned, &mut buffer)
                .and_then(|copied| copied_track_path(planned).map(|path| (copied, path)))
            {
                Ok(copied) => copied,
                Err(e) => {
                    result.errors.push(format!("{}: {}", source, e));
                    result.error_count += 1;
                    continue;
                }
            };
            if let Some(e) = &copied.lyrics_error {
                result.errors.push(format!("{}: {}", source, e));
                result.error_count += 1;
            }

            let mut track = planned.track.clone();
            track.file_name = planned
                .target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            track.file_path = target_path;
            track.file_size = to_bytes(copied.size);
            let modified_at = get_file_modified_at(&planned.target);
            let action = match missing_tracks.take_match(&track, modified_at) {
                Some(moved) => ImportAction::Relink(moved.id),
                None => ImportAction::Insert,
            };
            pending.push((planned, copied.lyrics_copied, track, action, modified_at));
        }
        if pending.is_empty() {
            continue;
        }

        // 2. ロック内: バッチをまとめて登録する
        let written = state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;
            let outcomes: Vec<AppResult<()>> = pending
                .iter()
                .map(|(_, _, track, action, modified_at)| match action {
                    ImportAction::Relink(track_id) => crate::repository::relink_track(
                        &tx,
                        track_id,
                        &track.file_path,
                        &track.file_name,
                        *modified_at,
                    ),
                    _ => crate::repository::insert_track(&tx, track, *modified_at),
                })
                .collect();
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })?;
            Ok(outcomes)
        });
        let outcomes = match written {
            Ok(outcomes) => outcomes,
            Err(e) => {
                for (planned, lyrics_copied, ..) in &pending {
                    remove_copy(planned, *lyrics_copied, music_folder);
                }
                return Err(e);
            }
        };
        for ((planned, lyrics_copied, _, action, _), outcome) in pending.into_iter().zip(outcomes) {
            match (outcome, action) {
                (Ok(()), ImportAction::Relink(_)) => result.relinked_count += 1,
                (Ok(()), _) => result.imported_count += 1,
                (Err(e), _) => {
                    result
                        .errors
                        .push(format!("{}: {}", planned.source.display(), e));
                    result.error_count += 1;
                    remove_copy(planned, lyrics_copied, music_folder);
                    continue;
                }
            }
            registered.push(planned);
        }
        log::info!(
            "音楽フォルダへのコピー: {}/{} ファイル処理完了",
            processed,
            total
        );
    }
    Ok(registered)
}

/// コピー先の、登録するパス
fn copied_track_path(planned: &PlannedCopy) -> AppResult<String> {
    planned
        .target
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Io("ファイルパスの変換に失敗しました".to_string()))
}

/// 進み具合に出す、ファイルの名前
fn display_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("不明なファイル")
        .to_string()
}

/// フォルダのインポートで、フォルダをライブラリフォルダとして記録する場合
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FolderRegistration {
    /// 音楽ファイルが見つかった場合だけ記録する（中のライブラリフォルダの記録を、空のフォルダで
    /// まとめて消さないため）。記録に失敗しても、インポートの結果は返す
    IfTracksFound,
    /// 空でも記録する（音楽フォルダ）。記録に失敗したら、エラーにする
    Always,
}

/// フォルダの中の音楽ファイルをインポートし、フォルダをライブラリフォルダとして記録する（同期処理）
///
/// 呼び出し側が`lock_library_scan`を持って呼ぶ。
///
/// @returns インポートの結果と、記録したライブラリフォルダのID（記録しなかった・登録済みのフォルダの
///   中だった場合はNone）
pub(super) fn import_folder_blocking(
    path: &Path,
    duplicate_action: DuplicateAction,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
    registration: FolderRegistration,
) -> AppResult<(ImportResult, Option<String>)> {
    let folder = ScannedImportFolder::scan(state, path)?;
    import_scanned_folder(&folder, duplicate_action, state, on_progress, registration)
}

/// 走査したフォルダの音楽ファイルをインポートし、フォルダをライブラリフォルダとして記録する（同期処理）
fn import_scanned_folder(
    folder: &ScannedImportFolder,
    duplicate_action: DuplicateAction,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
    registration: FolderRegistration,
) -> AppResult<(ImportResult, Option<String>)> {
    let audio_files = &folder.scanned.audio_files;
    let mut result = import_audio_files(audio_files, duplicate_action, state, on_progress)?;
    // 読めなかったフォルダは、取り込めなかったものとして結果に出す
    let unreadable = folder.scanned.unreadable_dir_errors();
    result.error_count += to_count(unreadable.len())?;
    result.errors.extend(unreadable);

    // インポートしたフォルダをライブラリフォルダとして記録する（再スキャンの対象になる）
    let register = || {
        let folder_path = folder
            .path
            .to_str()
            .ok_or_else(|| AppError::Validation("フォルダの名前を扱えません".to_string()))?;
        state.with_db(|db| {
            crate::library_folder::register_folder(db, folder_path, &folder.exclusions.to_register)
        })
    };
    let folder_id = match registration {
        FolderRegistration::Always => register()?,
        // 記録に失敗してもインポート自体は成功しているため、結果は返す
        FolderRegistration::IfTracksFound if !audio_files.is_empty() => {
            register().unwrap_or_else(|e| {
                log::error!("ライブラリフォルダの記録に失敗しました: {}", e);
                None
            })
        }
        FolderRegistration::IfTracksFound => None,
    };

    Ok((result, folder_id))
}

/// 音楽ファイルの一覧を、ライブラリへ登録する（同期処理。フォルダ・ファイルのインポートで共通）
fn import_audio_files(
    audio_files: &[PathBuf],
    duplicate_action: DuplicateAction,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportResult> {
    let mut imported_count = 0;
    let mut skipped_count = 0;
    let mut relinked_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    // バッチサイズ（一度にコミットするファイル数）
    const BATCH_SIZE: usize = 50;
    // 進捗イベントの件数はu32で送る（TypeScript側でnumberとして扱うため）
    let total_files = u32::try_from(audio_files.len())
        .map_err(|_| AppError::Validation("ファイル数が多すぎます".to_string()))?;
    let mut processed_count: u32 = 0;

    // 登録済みファイルパスを一度だけ取得する（ファイルごとの重複クエリを避ける）
    //
    // 処理済みのパスも随時追加していくため、同一インポート内で同じパスが
    // 複数回現れた場合も2回目以降は重複として扱われる（file_pathはUNIQUE）。
    // 見つからない曲は、新しいファイルの対応付けの候補にする（ライブラリフォルダの外へ移動した
    // ファイルをインポートした場合などに、新しい曲として登録せず、同じ曲として引き継ぐ）
    let (mut known_paths, mut missing_tracks) = state.with_db(|db| {
        Ok((
            crate::repository::find_all_file_paths(db)?,
            MissingTrackIndex::new(crate::repository::find_missing_tracks(db)?),
        ))
    })?;

    // バッチ処理でインポート
    //
    // ファイル読み取り（メタデータ抽出）はDBロックの外で行い、
    // ロックはバッチ単位の書き込みの間だけ保持する。
    // これにより大量インポート中も再生などの他操作がブロックされない。
    for chunk in audio_files.chunks(BATCH_SIZE) {
        // 1. ロック外: ファイルからトラック情報を抽出する
        // （トラック, 書き込み方, ファイルの更新日時）
        let mut pending: Vec<(Track, ImportAction, Option<i64>)> = Vec::new();
        // スキップした登録済みのファイルのパス（見つからない曲にしていた場合は、見つかる曲に戻す）
        let mut found_paths: Vec<String> = Vec::new();

        for file_path in chunk {
            let file_path_str = file_path
                .to_str()
                .ok_or_else(|| AppError::Io("ファイルパスの変換に失敗しました".to_string()))?;

            // 進捗イベントを送信
            processed_count += 1;
            on_progress(ImportProgress {
                phase: ImportPhase::Reading,
                current: processed_count,
                total: total_files,
                current_file: display_file_name(file_path),
            });

            // 重複チェック
            // insertは未登録のパスの場合にtrueを返すため、falseなら重複
            // （DBに存在するか、このインポートで既に処理済みのいずれか）
            let is_duplicate = !known_paths.insert(file_path_str.to_string());
            if is_duplicate && matches!(duplicate_action, DuplicateAction::Skip) {
                skipped_count += 1;
                found_paths.push(file_path_str.to_string());
                continue;
            }

            match create_track_from_file(file_path) {
                Ok(track) => {
                    let modified_at = get_file_modified_at(file_path);
                    let action = if is_duplicate {
                        ImportAction::Replace
                    } else if let Some(moved) = missing_tracks.take_match(&track, modified_at) {
                        ImportAction::Relink(moved.id)
                    } else {
                        ImportAction::Insert
                    };
                    pending.push((track, action, modified_at));
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path_str, e));
                    error_count += 1;
                }
            }
        }

        if pending.is_empty() && found_paths.is_empty() {
            continue;
        }

        // 2. ロック内: バッチをまとめて書き込む
        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;

            for path in &found_paths {
                crate::repository::restore_missing_track_by_file_path(&tx, path)?;
            }

            for (track, action, modified_at) in &pending {
                let result = match action {
                    // 既存のトラックを更新
                    ImportAction::Replace => {
                        crate::repository::update_track_by_file_path(&tx, track, *modified_at)
                    }
                    // 新しいトラックを追加
                    ImportAction::Insert => {
                        crate::repository::insert_track(&tx, track, *modified_at)
                    }
                    // 見つからない曲を、このファイルに結び付ける
                    ImportAction::Relink(track_id) => crate::repository::relink_track(
                        &tx,
                        track_id,
                        &track.file_path,
                        &track.file_name,
                        *modified_at,
                    ),
                };

                match (result, action) {
                    (Ok(()), ImportAction::Relink(_)) => relinked_count += 1,
                    (Ok(()), _) => imported_count += 1,
                    (Err(e), _) => {
                        errors.push(format!("{}: {}", track.file_path, e));
                        error_count += 1;
                    }
                }
            }

            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })?;

        // 進行状況をログ出力
        log::info!(
            "インポート進行状況: {}/{} ファイル処理完了",
            processed_count,
            total_files
        );
    }

    // 同じパスのファイルが差し替わった可能性があるため、アルバムアートを読み直させる
    if let Ok(mut cache) = state.album_art_cache.lock() {
        cache.clear();
    }

    Ok(ImportResult {
        imported_count,
        skipped_count,
        relinked_count,
        error_count,
        errors,
        ..ImportResult::default()
    })
}

/// 読み込んだファイルの書き込み方
enum ImportAction {
    /// 新しいトラックとして追加する
    Insert,
    /// 登録済みの同じパスのトラックを、読み直した内容で置き換える
    Replace,
    /// 見つからない曲（このID）を、移動・改名の先のこのファイルに結び付ける
    Relink(String),
}

/// ファイルからトラック情報を作成（再スキャン・「メタデータを更新」でも使う）
///
/// 評価はファイルのタグから読む。お気に入り・再生回数は初期値（更新時は使われない）。
pub(super) fn create_track_from_file(file_path: &Path) -> AppResult<Track> {
    let file_name = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Io("ファイル名の取得に失敗しました".to_string()))?
        .to_string();

    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| AppError::Io("ファイルパスの変換に失敗しました".to_string()))?
        .to_string();

    // 1回のファイルオープンで全情報を一括抽出
    let file_info = extract_all_file_info(file_path)?;
    let sort_tags = file_info.metadata.sort_tags();

    // タイトルがない場合はファイル名をデフォルトとして使用
    let title = file_info
        .metadata
        .title
        .or_else(|| Some(get_default_title(file_path)));

    let file_size = get_file_size(file_path)?;
    let format = get_file_format(file_path);

    let now = Utc::now().to_rfc3339();

    Ok(Track {
        id: Uuid::new_v4().to_string(),
        file_path: file_path_str,
        file_name,
        title,
        artist: file_info.metadata.artist,
        album: file_info.metadata.album,
        album_artist: file_info.metadata.album_artist,
        genre: file_info.metadata.genre,
        year: file_info.metadata.year,
        track_number: file_info.metadata.track_number,
        disc_number: file_info.metadata.disc_number,
        duration: file_info.duration,
        file_size,
        format,
        bitrate: file_info.bitrate,
        sample_rate: file_info.sample_rate,
        is_favorite: false,
        rating: file_info.rating,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        created_at: now.clone(),
        updated_at: now,
        replay_gain: file_info.replay_gain,
        sort_tags,
        is_missing: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::write_wav;
    use std::cell::RefCell;

    fn test_state() -> AppState {
        let conn = crate::db::open_in_memory();
        AppState::new(conn)
    }

    /// 音楽ファイル（WAV）を2つ、フォルダ（中にWAVが1つ）を1つ、音楽ではないファイルを1つ作る
    fn fixture() -> (PathBuf, [PathBuf; 4]) {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", Uuid::new_v4()));
        let album = dir.join("album");
        std::fs::create_dir_all(&album).unwrap();
        let first = dir.join("first.wav");
        let second = dir.join("second.WAV");
        let cover = dir.join("cover.jpg");
        write_wav(&first);
        write_wav(&second);
        write_wav(&album.join("inside.wav"));
        std::fs::write(&cover, b"not music").unwrap();
        (dir, [first, second, album, cover])
    }

    fn track_paths(state: &AppState) -> Vec<String> {
        let mut paths: Vec<String> = state
            .with_db(|db| crate::repository::find_all_tracks(db))
            .unwrap()
            .into_iter()
            .map(|track| track.file_path)
            .collect();
        paths.sort();
        paths
    }

    fn folder_paths(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| crate::library_folder::find_all_folders(db))
            .unwrap()
            .into_iter()
            .map(|folder| folder.path)
            .collect()
    }

    #[test]
    fn test_targets_are_split_into_audio_files_and_folders() {
        let (dir, [first, second, album, cover]) = fixture();

        let targets = ImportTargets::from_paths(&[
            first.clone(),
            cover.clone(),
            album.clone(),
            second.clone(),
            first.clone(),
            dir.join("missing.wav"),
        ]);
        // 音楽ではないファイル・ないファイル・重複は除く（拡張子の大文字と小文字は区別しない）
        assert_eq!(targets.files, vec![first, second]);
        assert_eq!(targets.folders, vec![album]);
        assert!(!targets.is_empty());
        assert!(ImportTargets::from_paths(&[cover, dir.join("missing.wav")]).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ファイルを選んだインポートは、曲だけを登録する（フォルダは、ライブラリフォルダにしない）
    #[test]
    fn test_importing_files_registers_only_the_tracks() {
        let (dir, [first, second, _album, cover]) = fixture();
        let state = test_state();
        let progress = RefCell::new(Vec::new());
        let targets = ImportTargets::from_paths(&[first.clone(), cover, second.clone()]);

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|event| {
            progress
                .borrow_mut()
                .push((event.current, event.total, event.current_file));
        })
        .unwrap();

        assert_eq!((result.imported_count, result.skipped_count), (2, 0));
        assert_eq!(
            track_paths(&state),
            [first.to_str().unwrap(), second.to_str().unwrap()]
        );
        // 同じフォルダにある、選んでいない曲（album/inside.wav）は登録しない
        assert!(folder_paths(&state).is_empty());
        assert_eq!(
            *progress.borrow(),
            [
                (1, 2, "first.wav".to_string()),
                (2, 2, "second.WAV".to_string())
            ]
        );

        // もう一度インポートすると、登録済みとしてスキップする
        let again = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();
        assert_eq!((again.imported_count, again.skipped_count), (0, 2));
        // 「このアプリで開く」では、開いた順に曲を受け取る
        let opened = state
            .with_db(|db| {
                crate::repository::find_tracks_by_file_paths(
                    db,
                    &[
                        second.to_str().unwrap().to_string(),
                        "/no/such/file.wav".to_string(),
                        first.to_str().unwrap().to_string(),
                    ],
                )
            })
            .unwrap();
        assert_eq!(
            opened
                .iter()
                .map(|track| track.file_name.as_str())
                .collect::<Vec<_>>(),
            ["second.WAV", "first.wav"]
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// フォルダを選んだインポート（`import_folder`）は、中の曲を登録し、フォルダを記録する
    #[test]
    fn test_importing_a_folder_registers_its_tracks_and_the_folder() {
        let (dir, [_first, _second, album, _cover]) = fixture();
        let state = test_state();
        let targets = ImportTargets::from_paths(std::slice::from_ref(&album));

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!(result.imported_count, 1);
        assert_eq!(
            track_paths(&state),
            [album.join("inside.wav").to_str().unwrap()]
        );
        assert_eq!(folder_paths(&state), [album.to_str().unwrap()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ファイルとフォルダを一緒に渡すと、フォルダの方はライブラリフォルダとして記録する
    #[test]
    fn test_importing_a_folder_with_files_registers_the_folder() {
        let (dir, [first, _second, album, _cover]) = fixture();
        let state = test_state();
        let targets = ImportTargets::from_paths(&[first, album.clone()]);

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!(result.imported_count, 2);
        assert_eq!(track_paths(&state).len(), 2);
        assert_eq!(folder_paths(&state), [album.to_str().unwrap()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// フォルダのインポートは、読み飛ばす場所を読まず、直下の既知のアプリのフォルダを、
    /// 読み込まないフォルダとして記録する
    #[test]
    fn test_importing_a_folder_skips_hidden_package_and_app_folders() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", Uuid::new_v4()));
        let music = dir.join("Music");
        for path in [
            "Artist/Album/01.wav",
            "Artist/Album/._01.wav",
            ".Trashes/501/a.wav",
            "Projects/Song.band/Media/b.wav",
            "Music/Media.localized/Music/c.wav",
            "MusicBee/Podcasts/Show/d.wav",
        ] {
            let path = music.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            write_wav(&path);
        }
        let state = test_state();
        let targets = ImportTargets::from_paths(std::slice::from_ref(&music));

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!((result.imported_count, result.error_count), (1, 0));
        assert_eq!(
            track_paths(&state),
            [music.join("Artist/Album/01.wav").to_str().unwrap()]
        );
        assert_eq!(folder_paths(&state), [music.to_str().unwrap()]);
        let exclusions: Vec<String> = state
            .with_db(|db| {
                let folder = crate::library_folder::find_all_folders(db)?.remove(0);
                crate::library_folder::find_exclusions(db, &folder.id)
            })
            .unwrap()
            .into_iter()
            .map(|exclusion| exclusion.path)
            .collect();
        assert_eq!(exclusions, ["Music", "MusicBee"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 音楽フォルダ（ライブラリフォルダとして登録する）と、コピーの宛先
    fn music_folder(state: &AppState, dir: &Path) -> CopyDestination {
        let folder = dir.join("Music");
        std::fs::create_dir_all(&folder).unwrap();
        state
            .with_db(|db| crate::library_folder::register_folder(db, folder.to_str().unwrap(), &[]))
            .unwrap();
        CopyDestination {
            folder,
            template: Template::parse(crate::settings::DEFAULT_MUSIC_FOLDER_TEMPLATE).unwrap(),
        }
    }

    /// 音楽フォルダへコピーして取り込む（取り込んだ結果でなければ、テストを失敗させる）
    fn import_copying(
        state: &AppState,
        files: &[PathBuf],
        folders: &[PathBuf],
        copy: &CopyDestination,
    ) -> ImportReport {
        let folders: Vec<ScannedImportFolder> = folders
            .iter()
            .map(|folder| ScannedImportFolder::scan(state, folder).unwrap())
            .collect();
        match import_scanned(
            state,
            files,
            &folders,
            DuplicateAction::Skip,
            Some(copy),
            &|_| {},
        )
        .unwrap()
        {
            ImportRun::Imported(report) => report,
            _ => panic!("取り込んでいない"),
        }
    }

    /// ファイルは、音楽フォルダへ書式どおりにコピーし、コピー先の曲を登録する（元のファイルは残す）
    #[test]
    fn test_copying_files_places_them_in_the_music_folder() {
        let (dir, [first, _second, _album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        let progress = RefCell::new(Vec::new());

        let folders: [ScannedImportFolder; 0] = [];
        let ImportRun::Imported(report) = import_scanned(
            &state,
            std::slice::from_ref(&first),
            &folders,
            DuplicateAction::Skip,
            Some(&copy),
            &|event| {
                progress
                    .borrow_mut()
                    .push((event.phase, event.current, event.total));
            },
        )
        .unwrap() else {
            panic!("取り込んでいない");
        };

        // タグのない曲は、アーティスト・アルバムが既定の名前で、タイトルはファイル名
        let target = copy.folder.join("Unknown Artist/Unknown Album/first.wav");
        assert!(target.is_file());
        assert!(first.is_file());
        assert_eq!(
            (report.result.imported_count, report.result.copied_count),
            (1, 1)
        );
        assert_eq!(track_paths(&state), [target.to_str().unwrap()]);
        assert_eq!(report.placed, HashMap::from([(first, target)]));
        assert_eq!(
            *progress.borrow(),
            [(ImportPhase::Reading, 1, 1), (ImportPhase::Copying, 1, 1)]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 同じファイルをもう一度取り込むと、コピーせずに、コピー済みの曲として扱う
    #[test]
    fn test_copying_the_same_file_again_skips_it() {
        let (dir, [first, _second, _album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        import_copying(&state, std::slice::from_ref(&first), &[], &copy);

        let again = import_copying(&state, std::slice::from_ref(&first), &[], &copy);

        let target = copy.folder.join("Unknown Artist/Unknown Album/first.wav");
        assert_eq!(
            (
                again.result.imported_count,
                again.result.skipped_count,
                again.result.copied_count
            ),
            (0, 1, 0)
        );
        assert!(again.result.not_copied.is_empty());
        assert_eq!(again.placed, HashMap::from([(first, target.clone())]));
        assert_eq!(track_paths(&state), [target.to_str().unwrap()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// コピー先に別のファイルがある曲は、上書きせず、取り込まずに知らせる
    #[test]
    fn test_copying_never_overwrites_and_reports_skipped_tracks() {
        let (dir, [first, second, _album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        let taken = copy.folder.join("Unknown Artist/Unknown Album/first.wav");
        std::fs::create_dir_all(taken.parent().unwrap()).unwrap();
        std::fs::write(&taken, b"another song").unwrap();

        let report = import_copying(&state, &[first.clone(), second.clone()], &[], &copy);

        assert_eq!(std::fs::read(&taken).unwrap(), b"another song");
        assert_eq!(
            report.result.not_copied,
            [crate::music_folder_copy::NotCopied {
                source: first.to_str().unwrap().to_string(),
                target: Path::new("Unknown Artist/Unknown Album/first.wav")
                    .to_string_lossy()
                    .into_owned(),
                reason: crate::music_folder_copy::NotCopiedReason::TargetExists,
            }]
        );
        // もう一方の曲は、コピーして取り込む
        assert_eq!(
            track_paths(&state),
            [copy
                .folder
                .join("Unknown Artist/Unknown Album/second.WAV")
                .to_str()
                .unwrap()]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ライブラリフォルダの中の曲・登録済みの曲は、コピーせずに参照する
    #[test]
    fn test_tracks_already_in_the_library_are_not_copied() {
        let (dir, [first, second, album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        // ライブラリフォルダの中の、まだ登録していない曲
        state
            .with_db(|db| crate::library_folder::register_folder(db, album.to_str().unwrap(), &[]))
            .unwrap();
        let inside = album.join("inside.wav");
        // 参照して登録済みの曲
        import_targets(
            &state,
            &ImportTargets::from_paths(std::slice::from_ref(&second)),
            DuplicateAction::Skip,
            &|_| {},
        )
        .unwrap();

        let report = import_copying(&state, &[first, second.clone(), inside.clone()], &[], &copy);

        assert_eq!(
            (
                report.result.imported_count,
                report.result.skipped_count,
                report.result.copied_count
            ),
            (2, 1, 1)
        );
        let mut expected = vec![
            copy.folder
                .join("Unknown Artist/Unknown Album/first.wav")
                .to_str()
                .unwrap()
                .to_string(),
            second.to_str().unwrap().to_string(),
            inside.to_str().unwrap().to_string(),
        ];
        expected.sort();
        assert_eq!(track_paths(&state), expected);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// フォルダをコピーして取り込む時は、フォルダをライブラリフォルダとして記録しない
    #[test]
    fn test_copying_a_folder_does_not_register_it() {
        let (dir, [_first, _second, album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);

        let report = import_copying(&state, &[], std::slice::from_ref(&album), &copy);

        assert_eq!(report.result.copied_count, 1);
        assert_eq!(
            track_paths(&state),
            [copy
                .folder
                .join("Unknown Artist/Unknown Album/inside.wav")
                .to_str()
                .unwrap()]
        );
        assert_eq!(folder_paths(&state), [copy.folder.to_str().unwrap()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 音楽フォルダが見つからない時は、何も取り込まない（黙って参照に切り替えない）
    #[test]
    fn test_missing_music_folder_imports_nothing() {
        let (dir, [first, _second, _album, _cover]) = fixture();
        let state = test_state();
        let copy = CopyDestination {
            folder: dir.join("Unplugged/Music"),
            template: Template::parse(crate::settings::DEFAULT_MUSIC_FOLDER_TEMPLATE).unwrap(),
        };

        let run = import_scanned(
            &state,
            &[first],
            &[],
            DuplicateAction::Skip,
            Some(&copy),
            &|_| {},
        )
        .unwrap();

        assert!(
            matches!(&run, ImportRun::MusicFolderMissing(path) if Path::new(path) == copy.folder)
        );
        assert!(track_paths(&state).is_empty());
        assert!(!copy.folder.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// コピーした曲が見つからない曲と同じ曲なら、新しい曲として登録せず、その曲に結び付ける
    #[test]
    fn test_copied_track_relinks_a_missing_track() {
        let (dir, [first, _second, _album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        // 外付けドライブにあった同じ曲（今は見つからない）
        let old = dir.join("Unplugged/first.wav");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::copy(&first, &old).unwrap();
        import_targets(
            &state,
            &ImportTargets::from_paths(std::slice::from_ref(&old)),
            DuplicateAction::Skip,
            &|_| {},
        )
        .unwrap();
        let id = state
            .with_db(|db| crate::repository::find_all_tracks(db))
            .unwrap()[0]
            .id
            .clone();
        std::fs::remove_file(&old).unwrap();
        state
            .with_db(|db| {
                db.execute(
                    "UPDATE tracks SET missing_since = '2026-01-01T00:00:00Z'",
                    [],
                )
                .map_err(|e| AppError::Database(e.to_string()))
            })
            .unwrap();

        let report = import_copying(&state, std::slice::from_ref(&first), &[], &copy);

        assert_eq!(
            (
                report.result.imported_count,
                report.result.relinked_count,
                report.result.copied_count
            ),
            (0, 1, 1)
        );
        let tracks = state
            .with_db(|db| crate::repository::find_all_tracks(db))
            .unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, id);
        assert_eq!(
            Path::new(&tracks[0].file_path),
            copy.folder.join("Unknown Artist/Unknown Album/first.wav")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 預かるフォルダの内容: 曲の数・大きさと、そのうちコピーする曲（ライブラリにある曲は除く）
    #[test]
    fn test_pending_summary_counts_tracks_to_copy() {
        let (dir, [first, second, album, _cover]) = fixture();
        let state = test_state();
        let copy = music_folder(&state, &dir);
        // 登録済みの曲（ドロップしたファイルの1つ）
        import_targets(
            &state,
            &ImportTargets::from_paths(std::slice::from_ref(&second)),
            DuplicateAction::Skip,
            &|_| {},
        )
        .unwrap();
        let folders = [ScannedImportFolder::scan(&state, &album).unwrap()];
        let size = std::fs::metadata(&first).unwrap().len() as i64;

        let summary = summarize_pending(&state, &[first, second], &folders, &copy).unwrap();

        assert_eq!(
            summary,
            PendingFolderImport {
                folders: vec![album.to_str().unwrap().to_string()],
                track_count: 3,
                total_bytes: size * 3,
                copy_count: 2,
                copy_bytes: size * 2,
                music_folder: copy.folder.to_str().unwrap().to_string(),
                music_folder_exists: true,
                free_bytes: summary.free_bytes,
            }
        );
        assert!(summary.free_bytes.is_some_and(|free| free > 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
