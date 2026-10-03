//! ライブラリフォルダ（インポートしたフォルダ）の一覧・削除・再スキャンのコマンド

use super::import::create_track_from_file;
use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::{LibraryChanged, LibraryScanProgress};
use crate::library::{get_file_modified_at, scan_directory};
use crate::library_folder::{
    DiskFile, LibraryFolder, LibraryFolderList, RescanPlan, RescanResult, find_all_folders,
    find_folder, plan_rescan, track_path_prefix,
};
use crate::library_sync::LibrarySync;
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 1回のトランザクションで書き込むトラック数（インポートと同じ）
const BATCH_SIZE: usize = 50;

/// ライブラリフォルダIDをバリデーション（UUID形式）
fn validate_folder_id(id: &str) -> AppResult<()> {
    validate_track_id(id)
        .map_err(|_| AppError::Validation("不正なライブラリフォルダID形式です".to_string()))
}

/// 件数（usize）をフロントエンドへ返すu32へ変換する
fn to_count(value: usize) -> AppResult<u32> {
    u32::try_from(value)
        .map_err(|_| AppError::Validation(format!("件数が扱える範囲を超えました: {}", value)))
}

/// ライブラリフォルダの一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_library_folders(state: State<'_, AppState>) -> AppResult<LibraryFolderList> {
    let (records, counts, total) = state.with_db(|db| {
        let records = find_all_folders(db)?;
        let counts = records
            .iter()
            .map(|f| crate::repository::count_tracks_under(db, &track_path_prefix(&f.path)))
            .collect::<AppResult<Vec<_>>>()?;
        let total = crate::repository::count_all_tracks(db)?;
        Ok((records, counts, total))
    })?;

    // フォルダの存在確認はファイルシステムへのアクセスのため、DBロックの外で行う
    let folders: Vec<LibraryFolder> = records
        .into_iter()
        .zip(counts.iter())
        .map(|(record, &track_count)| LibraryFolder {
            exists: Path::new(&record.path).is_dir(),
            id: record.id,
            path: record.path,
            track_count,
            added_at: record.added_at,
            last_scanned_at: record.last_scanned_at,
        })
        .collect();

    // フォルダ同士は入れ子にしないため、各フォルダのトラック数の合計は重複しない
    let registered: u32 = counts.iter().sum();
    Ok(LibraryFolderList {
        folders,
        unregistered_track_count: total.saturating_sub(registered),
    })
}

/// ライブラリフォルダの記録を削除する
///
/// `remove_tracks`がtrueなら、フォルダ内のトラックもライブラリから外す（ファイルは消さない）。
/// 外したトラック数を返す。
#[tauri::command]
#[specta::specta]
pub async fn remove_library_folder(
    folder_id: String,
    remove_tracks: bool,
    app_handle: AppHandle,
) -> AppResult<u32> {
    validate_folder_id(&folder_id)?;

    let state = app_handle.state::<AppState>();
    let removed = state.with_db(|db| {
        let folder = find_folder(db, &folder_id)?;
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
        to_count(removed)
    })?;

    // 削除したフォルダは監視しない
    app_handle
        .state::<LibrarySync>()
        .refresh_watches(&app_handle);

    if removed > 0 {
        notify_library_changed(&app_handle);
    }
    Ok(removed)
}

/// ライブラリフォルダを再スキャンし、追加・削除・変更されたファイルをライブラリに反映する
///
/// フォルダが見つからない場合はエラーにする（トラックは外さない）。
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
    let disk_files = scan_directory(folder_path)?
        .into_iter()
        .filter_map(|path| {
            let path_str = path.to_str()?.to_string();
            let metadata = fs::metadata(&path).ok();
            Some(DiskFile {
                size: metadata.as_ref().map(|m| m.len() as i64),
                modified_at: metadata.and(get_file_modified_at(&path)),
                path: path_str,
            })
        })
        .collect();
    let tracks = state.with_db(|db| {
        crate::repository::find_track_file_states_under(db, &track_path_prefix(&folder.path))
    })?;
    let plan = plan_rescan(disk_files, tracks);

    let mut result = RescanResult {
        removal_skipped: plan.removal_skipped,
        ..RescanResult::default()
    };

    // 2. 追加・変更のあったファイルを読み込み、バッチごとに書き込む
    let progress = report_progress.then_some(app_handle);
    read_and_write_tracks(&plan, state, progress, &mut result)?;

    // 3. 更新日時の記録と、見つからなくなったトラックの削除
    let removed = state.with_db(|db| {
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        for (track_id, modified_at) in &plan.to_record_modified_at {
            crate::repository::set_track_file_modified_at(&tx, track_id, *modified_at)?;
        }
        let mut removed = 0;
        for track_id in &plan.to_remove {
            removed += crate::repository::delete_track(&tx, track_id)?;
        }
        crate::library_folder::mark_scanned(&tx, &folder.id)?;
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })?;
        Ok(removed)
    })?;
    result.removed_count = to_count(removed)?;

    log::info!(
        "ライブラリフォルダを再スキャンしました: {} 追加={}, 更新={}, 削除={}, エラー={}",
        folder.path,
        result.added_count,
        result.updated_count,
        result.removed_count,
        result.error_count
    );

    if result.updated_count > 0 || result.removed_count > 0 {
        // 同じパスのファイルが差し替わったため、アルバムアートを読み直させる
        if let Ok(mut cache) = state.album_art_cache.lock() {
            cache.clear();
        }
    }
    if result.added_count > 0 || result.updated_count > 0 || result.removed_count > 0 {
        notify_library_changed(app_handle);
    }

    Ok(result)
}

/// 追加・変更のあったファイルを読み込んでライブラリに書き込む
///
/// ファイルの読み込み（メタデータの抽出）はDBロックの外で行い、ロックはバッチ単位の
/// 書き込みの間だけ保持する（インポートと同じ）。
/// `progress`を渡すと、ファイルごとに`LibraryScanProgress`を送る。
fn read_and_write_tracks(
    plan: &RescanPlan,
    state: &AppState,
    progress: Option<&AppHandle>,
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

    for chunk in files.chunks(BATCH_SIZE) {
        let mut pending: Vec<(Track, bool, Option<i64>)> = Vec::new();

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
                Ok(track) => pending.push((track, *is_new, file.modified_at)),
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
            for (track, is_new, modified_at) in &pending {
                let written = if *is_new {
                    crate::repository::insert_track(&tx, track, *modified_at)
                } else {
                    crate::repository::update_track_by_file_path(&tx, track, *modified_at)
                };
                match written {
                    Ok(()) if *is_new => result.added_count += 1,
                    Ok(()) => result.updated_count += 1,
                    Err(e) => {
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
fn notify_library_changed(app_handle: &AppHandle) {
    if let Err(e) = LibraryChanged.emit(app_handle) {
        log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
    }
}
