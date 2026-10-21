//! ライブラリフォルダ（インポートしたフォルダ）の一覧・削除・再スキャンと、読み込まないフォルダ（除外）のコマンド

use super::import::create_track_from_file;
use super::{pick_folder, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::{LibraryChanged, LibraryScanProgress};
use crate::library::{modified_at_of, scan_directory, to_count};
use crate::library_folder::{
    DiskFile, ExclusionRecord, FolderExclusion, LibraryFolder, LibraryFolderList, RescanPlan,
    RescanResult, add_exclusion, delete_exclusion, excluded_path, find_all_folders, find_exclusion,
    find_exclusions, find_folder, plan_rescan, relative_to_folder, scan_exclusions,
    track_path_prefix,
};
use crate::library_sync::LibrarySync;
use crate::models::Track;
use crate::settings::Language;
use crate::state::AppState;
use crate::track_relink::MissingTrackIndex;
use crate::validation::validate_track_id;
use chrono::Utc;
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// 1回のトランザクションで書き込むトラック数（インポートと同じ）
const BATCH_SIZE: usize = 50;

/// ライブラリフォルダIDをバリデーション（UUID形式）
fn validate_folder_id(id: &str) -> AppResult<()> {
    validate_track_id(id)
        .map_err(|_| AppError::Validation("不正なライブラリフォルダID形式です".to_string()))
}

/// 読み込まないフォルダのIDをバリデーション（UUID形式）
fn validate_exclusion_id(id: &str) -> AppResult<()> {
    validate_track_id(id)
        .map_err(|_| AppError::Validation("不正な読み込まないフォルダのID形式です".to_string()))
}

/// ライブラリフォルダの一覧を取得
///
/// フォルダの存在確認は、外れたネットワークドライブなどで時間がかかることがあるため、
/// ブロッキング処理用スレッドで行う。
#[tauri::command]
#[specta::specta]
pub async fn get_library_folders(app_handle: AppHandle) -> AppResult<LibraryFolderList> {
    run_blocking(move || list_folders(app_handle.state::<AppState>().inner())).await
}

/// `get_library_folders`の本体（同期処理）
fn list_folders(state: &AppState) -> AppResult<LibraryFolderList> {
    let (records, counts, exclusions, total, missing_track_count) = state.with_db(|db| {
        let records = find_all_folders(db)?;
        let counts = records
            .iter()
            .map(|f| crate::repository::count_tracks_under(db, &track_path_prefix(&f.path)))
            .collect::<AppResult<Vec<_>>>()?;
        let exclusions = records
            .iter()
            .map(|f| {
                find_exclusions(db, &f.id)?
                    .into_iter()
                    .map(|exclusion| to_folder_exclusion(db, &f.path, exclusion))
                    .collect::<AppResult<Vec<_>>>()
            })
            .collect::<AppResult<Vec<_>>>()?;
        let total = crate::repository::count_all_tracks(db)?;
        let missing = crate::repository::count_missing_tracks(db)?;
        Ok((records, counts, exclusions, total, missing))
    })?;

    // フォルダの存在確認はファイルシステムへのアクセスのため、DBロックの外で行う
    let folders: Vec<LibraryFolder> = records
        .into_iter()
        .zip(counts.iter())
        .zip(exclusions)
        .map(|((record, &track_count), exclusions)| LibraryFolder {
            exists: Path::new(&record.path).is_dir(),
            id: record.id,
            path: record.path,
            track_count,
            added_at: record.added_at,
            last_scanned_at: record.last_scanned_at,
            exclusions,
            is_music_folder: record.is_music_folder,
        })
        .collect();

    // フォルダ同士は入れ子にしないため、各フォルダのトラック数の合計は重複しない
    let registered: u32 = counts.iter().sum();
    Ok(LibraryFolderList {
        folders,
        unregistered_track_count: total.saturating_sub(registered),
        missing_track_count,
    })
}

/// ライブラリフォルダの記録を削除する
///
/// `remove_tracks`がtrueなら、フォルダ内のトラックもライブラリから外す（ファイルは消さない）。
/// 外したトラック数を返す。音楽フォルダのライブラリフォルダなら、音楽フォルダは未設定に戻る
/// （印は記録と一緒に消える）。
#[tauri::command]
#[specta::specta]
pub async fn remove_library_folder(
    folder_id: String,
    remove_tracks: bool,
    app_handle: AppHandle,
) -> AppResult<u32> {
    validate_folder_id(&folder_id)?;

    // 再スキャン（自動のものを含む）が終わるのを待ち、監視の更新もするため、ブロッキング処理用スレッドで行う
    run_blocking(move || remove_folder_blocking(&folder_id, remove_tracks, &app_handle)).await
}

/// `remove_library_folder`の本体（同期処理）
fn remove_folder_blocking(
    folder_id: &str,
    remove_tracks: bool,
    app_handle: &AppHandle,
) -> AppResult<u32> {
    let state = app_handle.state::<AppState>();
    let (removed, was_music_folder) = {
        // 同じフォルダの再スキャンの途中で消すと、再スキャンが曲を書き戻してしまうため、終わるまで待つ
        let _scan = state.lock_library_scan();
        state.with_db(|db| {
            let folder = find_folder(db, folder_id)?;
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;

            let removed = if remove_tracks {
                crate::repository::delete_tracks_under(&tx, &track_path_prefix(&folder.path))?
            } else {
                0
            };
            crate::library_folder::delete_folder(&tx, &folder.id)?;

            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })?;
            Ok((to_count(removed)?, folder.is_music_folder))
        })?
    };

    // 削除したフォルダは監視しない
    app_handle
        .state::<LibrarySync>()
        .refresh_watches(app_handle);

    // 音楽フォルダを消した場合は、曲を外さなくても、音楽フォルダの設定が変わったことを知らせる
    if removed > 0 || was_music_folder {
        notify_library_changed(app_handle);
    }
    Ok(removed)
}

/// 読み込まないフォルダの中にある、ライブラリの曲のパスに共通する接頭辞
fn exclusion_track_prefix(folder_path: &str, exclusion: &ExclusionRecord) -> AppResult<String> {
    let excluded = excluded_path(folder_path, &exclusion.path);
    let excluded = excluded.to_str().ok_or_else(|| {
        AppError::Validation("読み込まないフォルダのパスを扱えません".to_string())
    })?;
    Ok(track_path_prefix(excluded))
}

/// 記録している読み込まないフォルダを、一覧表示用にする（中にある曲を数える）
fn to_folder_exclusion(
    db: &Connection,
    folder_path: &str,
    exclusion: ExclusionRecord,
) -> AppResult<FolderExclusion> {
    let track_count = crate::repository::count_tracks_under(
        db,
        &exclusion_track_prefix(folder_path, &exclusion)?,
    )?;
    Ok(FolderExclusion {
        id: exclusion.id,
        path: exclusion.path,
        track_count,
    })
}

/// 読み込まないフォルダを選ぶダイアログを開き、ライブラリフォルダの除外の一覧へ足す
///
/// ダイアログはRust側で、ライブラリフォルダを開いた状態で出す（WebViewからパスを受け取らない）。
/// 足せるのは、ライブラリフォルダの中のフォルダだけ。中の曲はライブラリに残す（外すかは、
/// 利用者が選び、`remove_excluded_tracks`で外す）。
///
/// @returns 足した除外（中にある曲の数を含む。選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn add_library_folder_exclusion(
    folder_id: String,
    window: WebviewWindow,
    app_handle: AppHandle,
) -> AppResult<Option<FolderExclusion>> {
    validate_folder_id(&folder_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let folder = state.with_db(|db| find_folder(db, &folder_id))?;
        if !Path::new(&folder.path).is_dir() {
            return Err(AppError::NotFound(format!(
                "フォルダが見つかりません: {}（外付けドライブなどが接続されているか確認してください）",
                folder.path
            )));
        }

        let title = match crate::menu::current_language(&app_handle) {
            Language::Ja => "読み込まないフォルダを選択",
            Language::En => "Select a Folder to Exclude",
        };
        let dialog = app_handle
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .set_directory(&folder.path);
        let Some(selected) = pick_folder(dialog)? else {
            return Ok(None);
        };
        let relative = relative_to_folder(&folder.path, &selected)?;

        // 再スキャンの途中で足すと、その再スキャンが除外の前の内容で曲を書き込むため、終わるまで待つ
        let _scan = state.lock_library_scan();
        let exclusion = state.with_db(|db| {
            let added = add_exclusion(db, &folder.id, &relative)?;
            to_folder_exclusion(db, &folder.path, added)
        })?;
        Ok(Some(exclusion))
    })
    .await
}

/// 読み込まないフォルダの中にある曲を、ライブラリから外す（ファイルは消さない）
///
/// 外したトラック数を返す。
#[tauri::command]
#[specta::specta]
pub async fn remove_excluded_tracks(exclusion_id: String, app_handle: AppHandle) -> AppResult<u32> {
    validate_exclusion_id(&exclusion_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let removed = {
            let _scan = state.lock_library_scan();
            state.with_db(|db| {
                let exclusion = find_exclusion(db, &exclusion_id)?;
                let folder = find_folder(db, &exclusion.folder_id)?;
                let prefix = exclusion_track_prefix(&folder.path, &exclusion)?;
                to_count(crate::repository::delete_tracks_under(db, &prefix)?)
            })?
        };
        if removed > 0 {
            notify_library_changed(&app_handle);
        }
        Ok(removed)
    })
    .await
}

/// 読み込まないフォルダを除外の一覧から外し、ライブラリフォルダを再スキャンする
///
/// 除外していたフォルダの曲を読み込み、除外の前から登録されていた曲も、再び比べる対象にする。
/// 再スキャンは`rescan_library_folder`と同じ（`LibraryScanProgress`を送る）。
///
/// @returns 再スキャンの結果（フォルダが見つからない場合は、再スキャンせずにnull）
#[tauri::command]
#[specta::specta]
pub async fn remove_library_folder_exclusion(
    exclusion_id: String,
    app_handle: AppHandle,
) -> AppResult<Option<RescanResult>> {
    validate_exclusion_id(&exclusion_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let folder = state.with_db(|db| {
            let exclusion = find_exclusion(db, &exclusion_id)?;
            delete_exclusion(db, &exclusion.id)?;
            find_folder(db, &exclusion.folder_id)
        })?;
        if !Path::new(&folder.path).is_dir() {
            return Ok(None);
        }
        rescan_folder(&folder.id, state.inner(), &app_handle, true).map(Some)
    })
    .await
}

/// 見つからない曲（ファイルが見つからなくなったトラック）を、すべてライブラリから外す
///
/// 外したトラック数を返す。ファイルには触れない。
#[tauri::command]
#[specta::specta]
pub async fn remove_missing_tracks(app_handle: AppHandle) -> AppResult<u32> {
    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let removed = {
            // 再スキャンの途中（見つからない曲にしてから、移動・改名の先のファイルに結び付けるまでの間）に
            // 外さないよう、終わるまで待つ
            let _scan = state.lock_library_scan();
            let removed = state.with_db(|db| crate::repository::delete_missing_tracks(db))?;
            to_count(removed)?
        };
        if removed > 0 {
            notify_library_changed(&app_handle);
        }
        Ok(removed)
    })
    .await
}

/// ライブラリフォルダを再スキャンし、追加・移動・削除・変更されたファイルをライブラリに反映する
///
/// フォルダが見つからない場合はエラーにする（トラックは見つからない曲にしない）。
#[tauri::command]
#[specta::specta]
pub async fn rescan_library_folder(
    folder_id: String,
    app_handle: AppHandle,
) -> AppResult<RescanResult> {
    validate_folder_id(&folder_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        rescan_folder(&folder_id, state.inner(), &app_handle, true)
    })
    .await
}

/// ライブラリフォルダを再スキャンする（同期処理。`rescan_library_folder`と自動の再スキャンが使う）
///
/// 他のインポート・再スキャンが終わるまで待ってから始める。
/// `report_progress`がtrueなら`LibraryScanProgress`を送る（設定ウィンドウの手動の再スキャン用）。
pub(crate) fn rescan_folder(
    folder_id: &str,
    state: &AppState,
    app_handle: &AppHandle,
    report_progress: bool,
) -> AppResult<RescanResult> {
    let _scan = state.lock_library_scan();
    let folder = state.with_db(|db| find_folder(db, folder_id))?;
    let folder_path = Path::new(&folder.path);
    if !folder_path.is_dir() {
        return Err(AppError::NotFound(format!(
            "フォルダが見つかりません: {}（外付けドライブなどが接続されているか確認してください）",
            folder.path
        )));
    }

    // 1. ディスク上のファイルとライブラリのトラックを比べる（ファイルの走査はロックの外）
    let exclusions = state.with_db(|db| scan_exclusions(db, &folder))?;
    let scanned = scan_directory(folder_path, &exclusions)?;
    let disk_files = scanned
        .audio_files
        .iter()
        .filter_map(|path| {
            let path_str = path.to_str()?.to_string();
            let metadata = fs::metadata(path).ok();
            Some(DiskFile {
                size: metadata.as_ref().map(|m| m.len() as i64),
                modified_at: metadata.as_ref().and_then(modified_at_of),
                path: path_str,
            })
        })
        .collect();
    // 読めなかったフォルダ（権限がないなど）の中のトラックは、比べる対象から外す
    // （ファイルがあるかを確かめられないため、見つからない曲にしない）。読み飛ばす場所
    // （隠しフォルダ・ゴミ箱・パッケージ・読み込まないフォルダ）の中のトラックも、走査していない
    // ため比べず、そのまま残す（外すかは利用者が選ぶ。ADR-062）
    let tracks = state
        .with_db(|db| {
            crate::repository::find_track_file_states_under(db, &track_path_prefix(&folder.path))
        })?
        .into_iter()
        .filter(|track| {
            let path = Path::new(&track.file_path);
            !scanned.is_in_unreadable_dir(path) && !exclusions.skips(folder_path, path)
        })
        .collect();
    let plan = plan_rescan(disk_files, tracks);

    let unreadable = scanned.unreadable_dir_errors();
    let mut result = RescanResult {
        missing_skipped: plan.missing_skipped,
        error_count: to_count(unreadable.len())?,
        errors: unreadable,
        ..RescanResult::default()
    };

    // 2. 見つからなくなったトラックを見つからない曲にし、同じ場所に戻ったトラックを元に戻す。
    //    ファイルの読み込みの前に行い、見つからない曲（これまでの分を含む）を、新しく見つかった
    //    ファイルの対応付けの候補にする。途中で失敗しても、見つからない曲は次の再スキャンで対応付けられる
    let progress = report_progress.then_some(app_handle);
    let outcome = record_file_states(state, &plan, &mut result)
        // 3. 追加・変更のあったファイルを読み込み、バッチごとに書き込む
        .and_then(|newly_missing| {
            read_and_write_tracks(&plan, state, progress, newly_missing, &mut result)
        })
        // 4. スキャン日時を記録する（途中で失敗した場合は記録しない）
        .and_then(|()| state.with_db(|db| crate::library_folder::mark_scanned(db, &folder.id)));

    // 失敗した場合も、それまでに書き込んだバッチはライブラリに反映されているため、通知する
    log::info!(
        "ライブラリフォルダを再スキャンしました: {} 追加={}, 更新={}, 移動={}, 見つからない={}, 見つかった={}, エラー={}",
        folder.path,
        result.added_count,
        result.updated_count,
        result.relinked_count,
        result.missing_count,
        result.restored_count,
        result.error_count
    );

    if result.updated_count > 0 || result.relinked_count > 0 || result.restored_count > 0 {
        // 同じトラックのファイルが差し替わった・見つかったため、アルバムアートを読み直させる
        if let Ok(mut cache) = state.album_art_cache.lock() {
            cache.clear();
        }
    }
    if result.added_count > 0
        || result.updated_count > 0
        || result.relinked_count > 0
        || result.missing_count > 0
        || result.restored_count > 0
    {
        notify_library_changed(app_handle);
    }

    outcome.map(|()| result)
}

/// 更新日時を記録し、見つからなくなったトラックを見つからない曲に、同じ場所に戻ったトラックを元に戻す
///
/// @returns 今回の再スキャンで見つからない曲にしたトラックのID
fn record_file_states(
    state: &AppState,
    plan: &RescanPlan,
    result: &mut RescanResult,
) -> AppResult<HashSet<String>> {
    let (newly_missing, restored) = state.with_db(|db| {
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        for (track_id, modified_at) in &plan.to_record_modified_at {
            crate::repository::set_track_file_modified_at(&tx, track_id, *modified_at)?;
        }
        let now = Utc::now().to_rfc3339();
        let mut newly_missing = HashSet::new();
        for track_id in &plan.to_mark_missing {
            if crate::repository::mark_track_missing(&tx, track_id, &now)? > 0 {
                newly_missing.insert(track_id.clone());
            }
        }
        let mut restored = 0;
        for track_id in &plan.to_restore {
            restored += crate::repository::restore_missing_track(&tx, track_id)?;
        }
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })?;
        Ok((newly_missing, restored))
    })?;
    result.missing_count = to_count(newly_missing.len())?;
    result.restored_count = to_count(restored)?;
    Ok(newly_missing)
}

/// 読み込んだファイルの書き込み方
enum WriteAction {
    /// 新しいトラックとして追加する
    Insert,
    /// 同じパスのトラックを、読み直した内容で更新する
    Update,
    /// 見つからない曲（このID）を、移動・改名の先のこのファイルに結び付ける
    Relink(String),
}

/// 追加・変更のあったファイルを読み込んでライブラリに書き込む
///
/// ファイルの読み込み（メタデータの抽出）はDBロックの外で行い、ロックはバッチ単位の
/// 書き込みの間だけ保持する（インポートと同じ）。
/// 新しいファイルが、見つからない曲（ほかのライブラリフォルダの分を含む）の移動・改名の先と
/// 判定できた場合は、新しいトラックとして追加せず、その曲に結び付ける。
/// `progress`を渡すと、ファイルごとに`LibraryScanProgress`を送る。
/// `newly_missing`は、今回の再スキャンで見つからない曲にしたトラックのID
/// （結び付けた分を、結果の「見つからない曲にした数」から除くために使う）。
fn read_and_write_tracks(
    plan: &RescanPlan,
    state: &AppState,
    progress: Option<&AppHandle>,
    mut newly_missing: HashSet<String>,
    result: &mut RescanResult,
) -> AppResult<()> {
    // (ファイル, 新規か)
    let files: Vec<(&DiskFile, bool)> = plan
        .to_add
        .iter()
        .map(|f| (f, true))
        .chain(plan.to_update.iter().map(|f| (f, false)))
        .collect();
    let total = to_count(files.len())?;
    let mut processed: u32 = 0;

    // 新しいファイルがある場合だけ、見つからない曲を対応付けの候補として読み込む
    let mut missing_tracks = if plan.to_add.is_empty() {
        MissingTrackIndex::default()
    } else {
        MissingTrackIndex::new(state.with_db(|db| crate::repository::find_missing_tracks(db))?)
    };

    for chunk in files.chunks(BATCH_SIZE) {
        let mut pending: Vec<(Track, WriteAction, Option<i64>)> = Vec::new();

        for (file, is_new) in chunk {
            let path = Path::new(&file.path);
            processed += 1;
            if let Some(app_handle) = progress {
                let event = LibraryScanProgress {
                    current: processed,
                    total,
                    current_file: path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("不明なファイル")
                        .to_string(),
                };
                if let Err(e) = event.emit(app_handle) {
                    log::warn!("再スキャンの進捗イベントの送信に失敗しました: {}", e);
                }
            }

            match create_track_from_file(path) {
                Ok(track) => {
                    let action = if !*is_new {
                        WriteAction::Update
                    } else if let Some(moved) = missing_tracks.take_match(&track, file.modified_at)
                    {
                        WriteAction::Relink(moved.id)
                    } else {
                        WriteAction::Insert
                    };
                    pending.push((track, action, file.modified_at));
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", file.path, e));
                    result.error_count += 1;
                }
            }
        }

        if pending.is_empty() {
            continue;
        }

        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;
            for (track, action, modified_at) in &pending {
                let written = match action {
                    WriteAction::Insert => {
                        crate::repository::insert_track(&tx, track, *modified_at)
                    }
                    WriteAction::Update => {
                        crate::repository::update_track_by_file_path(&tx, track, *modified_at)
                    }
                    WriteAction::Relink(track_id) => crate::repository::relink_track(
                        &tx,
                        track_id,
                        &track.file_path,
                        &track.file_name,
                        *modified_at,
                    ),
                };
                match (written, action) {
                    (Ok(()), WriteAction::Insert) => result.added_count += 1,
                    (Ok(()), WriteAction::Update) => result.updated_count += 1,
                    (Ok(()), WriteAction::Relink(track_id)) => {
                        result.relinked_count += 1;
                        // 今回見つからなくなった曲の移動・改名だった（見つからない曲のままではない）
                        if newly_missing.remove(track_id) {
                            result.missing_count = result.missing_count.saturating_sub(1);
                        }
                    }
                    (Err(e), _) => {
                        result.errors.push(format!("{}: {}", track.file_path, e));
                        result.error_count += 1;
                    }
                }
            }
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })?;
    }

    Ok(())
}

/// ライブラリが変わったことを他のウィンドウに知らせる
pub(super) fn notify_library_changed(app_handle: &AppHandle) {
    if let Err(e) = LibraryChanged.emit(app_handle) {
        log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
    }
}
