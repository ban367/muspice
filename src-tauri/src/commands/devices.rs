//! 転送先デバイス（SDカードなどのフォルダ）の登録・設定・同期のコマンド
//!
//! 転送先のフォルダは、登録（`register_sync_device`）と転送先の変更（`relink_sync_device`）で
//! Rust側のダイアログから選ぶ（WebViewからパスを受け取らない。ADR-065）。それ以外の操作は、
//! デバイスのIDからDBで解決する。

use super::{file_dialog, pick_folder, run_blocking};
use crate::device::{
    DeviceRecord, DeviceSyncPlan, DeviceSyncResult, DeviceSyncState, DeviceTranscode,
    PLAN_ITEMS_LIMIT, SyncDevice, SyncDeviceConfig, SyncRename, SyncSelectionSummary,
    TranscodeMode, suggest_device_name, validate_device_name,
};
use crate::device_manifest::{
    Manifest, read_device_id, read_manifest, to_device_path, write_manifest,
};
use crate::device_sync::{
    ExtraKind, SourceExtra, SourceFile, SourcePlaylist, SourceTrack, SyncPlan, Transcode,
    plan_sync, track_relative_path,
};
use crate::device_transfer::{TransferProgress, execute_plan};
use crate::error::{AppError, AppResult};
use crate::events::DeviceSyncProgress;
use crate::file_organize::{DirCache, Template};
use crate::library::{modified_at_of, to_count};
use crate::library_folder::{find_all_folders, folders_overlap, normalize_folder_path};
use crate::metadata::MAX_ALBUM_ART_BYTES;
use crate::repository::TransferTrack;
use crate::settings::Language;
use crate::smart_playlist::EvalContext;
use crate::state::AppState;
use crate::validation::{validate_playlist_id, validate_smart_playlist_id, validate_track_id};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use uuid::Uuid;

/// 空き容量の判定で、管理ファイルとプレイリストのために残しておく容量（バイト）
const SPACE_MARGIN_BYTES: i64 = 1024 * 1024;

/// ダイアログの文言と、フォルダから名前を決められない時のデバイス名（設定の言語に合わせる）
struct Labels {
    pick_title: &'static str,
    default_name: &'static str,
}

impl Labels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                pick_title: "転送先のフォルダを選択",
                default_name: "デバイス",
            },
            Language::En => Self {
                pick_title: "Select the destination folder",
                default_name: "Device",
            },
        }
    }
}

/// デバイスIDをバリデーション（UUID形式）
fn validate_device_id(id: &str) -> AppResult<()> {
    validate_track_id(id).map_err(|_| AppError::Validation("不正なデバイスID形式です".to_string()))
}

/// 容量（u64）を、フロントエンドへ返すi64へ変換する
fn to_bytes(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// 一覧表示用のデバイスにする（接続の確認と容量の取得のため、ファイルシステムにアクセスする）
fn to_sync_device(record: DeviceRecord) -> SyncDevice {
    let root = Path::new(&record.path);
    let connected = read_device_id(root).is_some_and(|id| id == record.id);
    let (free_bytes, total_bytes) = if connected {
        (
            fs4::available_space(root).ok().map(to_bytes),
            fs4::total_space(root).ok().map(to_bytes),
        )
    } else {
        (None, None)
    };

    // 取り出せるのは、macOSで、`/Volumes`の下のドライブにあり、接続されているデバイス
    let ejectable =
        cfg!(target_os = "macos") && connected && crate::device::volume_mount_point(root).is_some();

    SyncDevice {
        id: record.id,
        name: record.name,
        path: record.path,
        sync_all: record.sync_all,
        playlist_ids: record.playlist_ids,
        smart_playlist_ids: record.smart_playlist_ids,
        remove_unselected: record.remove_unselected,
        path_template: record.path_template,
        copy_extras: record.copy_extras,
        transcode: record.transcode,
        playlist_format: record.playlist_format,
        ejectable,
        connected,
        free_bytes,
        total_bytes,
        created_at: record.created_at,
        last_synced_at: record.last_synced_at,
    }
}

/// デバイスを読み直して、一覧表示用の形で返す
fn load_sync_device(state: &AppState, device_id: &str) -> AppResult<SyncDevice> {
    let record = state.with_db(|db| crate::device::find_device(db, device_id))?;
    Ok(to_sync_device(record))
}

/// 転送先にするフォルダを検証し、記録用にそろえたパスを返す
///
/// ライブラリフォルダと重なるフォルダは転送先にできない（コピーした曲を再スキャンが
/// ライブラリに取り込み、その曲をまたコピーしてしまうため）。
fn validate_device_folder(state: &AppState, folder: &Path) -> AppResult<String> {
    let path = folder.to_str().map(normalize_folder_path).ok_or_else(|| {
        AppError::Validation("フォルダのパスに扱えない文字が含まれています".to_string())
    })?;
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err(AppError::NotFound(format!(
            "フォルダが見つかりません: {}",
            path
        )));
    }

    let library_folders = state.with_db(|db| find_all_folders(db))?;
    if library_folders
        .iter()
        .any(|folder| folders_overlap(root, Path::new(&folder.path)))
    {
        return Err(AppError::Validation(
            "ライブラリフォルダと重なるフォルダは、転送先にできません".to_string(),
        ));
    }
    Ok(path)
}

/// 登録済みのデバイスの一覧を取得する
///
/// 接続の確認は、外れたネットワークドライブなどで時間がかかることがあるため、
/// ブロッキング処理用スレッドで行う。
#[tauri::command]
#[specta::specta]
pub async fn get_sync_devices(app_handle: AppHandle) -> AppResult<Vec<SyncDevice>> {
    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let records = state.with_db(|db| crate::device::find_all_devices(db))?;
        // 接続の確認はファイルシステムへのアクセスのため、DBロックの外で行う
        Ok(records.into_iter().map(to_sync_device).collect())
    })
    .await
}

/// フォルダを選ぶダイアログを開き、選んだフォルダを転送先デバイスとして登録する
///
/// ダイアログはRust側で開く（WebViewからパスを受け取らない。ADR-065）。名前は、ドライブ・
/// フォルダの名前にする（サイドバーの右クリックのメニューで変えられる）。フォルダに管理ファイルがある場合は、
/// そのデバイスとして扱う（登録済みなら転送先を更新し、未登録なら管理ファイルを引き継いで
/// 登録する）。なければ新しく作る。
///
/// @returns 登録したデバイス（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn register_sync_device(app_handle: AppHandle) -> AppResult<Option<SyncDevice>> {
    run_blocking(move || {
        let labels = Labels::for_language(crate::menu::current_language(&app_handle));
        let Some(folder) = pick_folder(file_dialog(&app_handle).set_title(labels.pick_title))?
        else {
            return Ok(None);
        };
        let name = suggest_device_name(&folder).unwrap_or_else(|| labels.default_name.to_string());

        let state = app_handle.state::<AppState>();
        let device_id = register_device(state.inner(), &folder, &name)?;
        load_sync_device(state.inner(), &device_id).map(Some)
    })
    .await
}

/// `register_sync_device`のダイアログの後の処理（同期処理）。登録したデバイスのIDを返す
fn register_device(state: &AppState, folder: &Path, name: &str) -> AppResult<String> {
    let name = validate_device_name(name)?;
    let path = validate_device_folder(state, folder)?;
    let root = Path::new(&path);

    let device_id = match read_manifest(root)? {
        Some(manifest) => {
            validate_device_id(&manifest.device_id).map_err(|_| {
                AppError::Validation("デバイスの管理ファイルの内容が正しくありません".to_string())
            })?;
            state.with_db(
                |db| match crate::device::try_find_device(db, &manifest.device_id)? {
                    Some(_) => crate::device::set_device_path(db, &manifest.device_id, &path),
                    None => crate::device::insert_device(db, &manifest.device_id, &name, &path),
                },
            )?;
            manifest.device_id
        }
        None => {
            let device_id = Uuid::new_v4().to_string();
            write_manifest(root, &Manifest::new(&device_id))?;
            state.with_db(|db| crate::device::insert_device(db, &device_id, &name, &path))?;
            device_id
        }
    };

    log::info!("Registered sync device: {}", path);
    Ok(device_id)
}

/// デバイスの設定（名前と同期する対象）を更新する
#[tauri::command]
#[specta::specta]
pub async fn update_sync_device(
    device_id: String,
    config: SyncDeviceConfig,
    app_handle: AppHandle,
) -> AppResult<SyncDevice> {
    validate_device_id(&device_id)?;
    for playlist_id in &config.playlist_ids {
        validate_playlist_id(playlist_id)?;
    }
    for smart_playlist_id in &config.smart_playlist_ids {
        validate_smart_playlist_id(smart_playlist_id)?;
    }
    // 配置の書式は、読めることを確かめてから保存する（前後の空白は除く）
    let path_template = config.path_template.trim().to_string();
    Template::parse(&path_template)?;
    crate::device::validate_transcode(&config.transcode)?;
    let config = SyncDeviceConfig {
        name: validate_device_name(&config.name)?,
        path_template,
        ..config
    };

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        state.with_db(|db| crate::device::update_device_config(db, &device_id, &config))?;
        load_sync_device(state.inner(), &device_id)
    })
    .await
}

/// フォルダを選ぶダイアログを開き、デバイスの転送先をそのフォルダに変える（マウント先が
/// 変わった場合や、カードを初期化した場合）
///
/// ダイアログはRust側で開く（WebViewからパスを受け取らない。ADR-065）。フォルダに別のデバイスの
/// 管理ファイルがある場合はエラーにする。管理ファイルがなければ、このデバイスの管理ファイルを
/// 新しく作る。
///
/// @returns 転送先を変えたデバイス（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn relink_sync_device(
    device_id: String,
    app_handle: AppHandle,
) -> AppResult<Option<SyncDevice>> {
    validate_device_id(&device_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        // 見つからないデバイスなら、ダイアログを開かずにエラーにする
        state.with_db(|db| crate::device::find_device(db, &device_id))?;
        let labels = Labels::for_language(crate::menu::current_language(&app_handle));
        let Some(folder) = pick_folder(file_dialog(&app_handle).set_title(labels.pick_title))?
        else {
            return Ok(None);
        };

        relink_device(state.inner(), &device_id, &folder)?;
        load_sync_device(state.inner(), &device_id).map(Some)
    })
    .await
}

/// `relink_sync_device`のダイアログの後の処理（同期処理）
fn relink_device(state: &AppState, device_id: &str, folder: &Path) -> AppResult<()> {
    state.with_db(|db| crate::device::find_device(db, device_id))?;
    let path = validate_device_folder(state, folder)?;
    let root = Path::new(&path);

    match read_manifest(root)? {
        Some(manifest) if manifest.device_id == device_id => {}
        Some(_) => {
            return Err(AppError::Validation(
                "このフォルダは別のデバイスの転送先として使われています".to_string(),
            ));
        }
        None => write_manifest(root, &Manifest::new(device_id))?,
    }
    state.with_db(|db| crate::device::set_device_path(db, device_id, &path))
}

/// デバイスの登録を解除する
///
/// デバイス上の曲と管理ファイルは消さない（同じフォルダをもう一度登録すると、
/// コピー済みの曲を引き継ぐ）。
#[tauri::command]
#[specta::specta]
pub async fn remove_sync_device(device_id: String, state: State<'_, AppState>) -> AppResult<()> {
    validate_device_id(&device_id)?;
    state.with_db(|db| {
        crate::device::find_device(db, &device_id)?;
        crate::device::delete_device(db, &device_id)
    })
}

/// 同期の準備ができたデバイス（接続を確認し、差分を計算した状態）
struct PreparedSync {
    device: DeviceRecord,
    root: PathBuf,
    manifest: Manifest,
    plan: SyncPlan,
}

/// デバイスに同期するプレイリストとオートプレイリスト（どちらも`.m3u8`を書き出す）
///
/// 全曲を同期するデバイスでは、すべてのプレイリスト・オートプレイリストを書き出す（選んだプレイリストの
/// 記録は、選んだプレイリストで同期する設定に戻した時のために残してあるが、使わない。ADR-135）。
/// オートプレイリストは、今の条件に合う曲を転送する（ランダムな並びは、選び直すまで同じ曲になる）。
/// プレイリストとオートプレイリストのIDは重ならないため、1つの一覧にして扱う。
fn source_playlists(
    db: &Connection,
    device: &DeviceRecord,
    context: EvalContext,
) -> AppResult<Vec<SourcePlaylist>> {
    let mut playlists: Vec<SourcePlaylist> = crate::playlist::get_all_playlists(db)?
        .into_iter()
        .filter(|playlist| device.sync_all || device.playlist_ids.contains(&playlist.id))
        .map(|playlist| SourcePlaylist {
            track_ids: crate::playlist::playlist_track_ids(&playlist),
            id: playlist.id,
            name: playlist.name,
            format: device.playlist_format,
        })
        .collect();
    for smart_playlist in crate::smart_playlist::get_all_smart_playlists(db)?
        .into_iter()
        .filter(|playlist| device.sync_all || device.smart_playlist_ids.contains(&playlist.id))
    {
        playlists.push(SourcePlaylist {
            track_ids: crate::smart_playlist::smart_playlist_track_ids(
                db,
                &smart_playlist,
                context,
            )?,
            id: smart_playlist.id,
            name: smart_playlist.name,
            format: device.playlist_format,
        });
    }
    Ok(playlists)
}

/// デバイスの接続を確認し、同期する曲・プレイリストと管理ファイルから差分を計算する
fn prepare_sync(state: &AppState, device_id: &str) -> AppResult<PreparedSync> {
    let context = state.smart_playlist_context();
    let (device, all_tracks, playlists) = state.with_db(|db| {
        let device = crate::device::find_device(db, device_id)?;
        let tracks = crate::repository::find_transfer_tracks(db)?;
        let playlists = source_playlists(db, &device, context)?;
        Ok((device, tracks, playlists))
    })?;

    // ここからはファイルシステムへのアクセスのため、DBロックの外で行う
    let root = PathBuf::from(&device.path);
    let manifest = read_manifest(&root)?
        .filter(|manifest| manifest.device_id == device.id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "デバイスが接続されていません: {}（接続されているか確認してください）",
                device.path
            ))
        })?;

    // 全曲を同期しない場合は、選んだプレイリストの曲だけにする
    let selected_ids: Option<HashSet<&str>> = (!device.sync_all).then(|| {
        playlists
            .iter()
            .flat_map(|playlist| playlist.track_ids.iter().map(String::as_str))
            .collect()
    });
    // デバイス上の配置の書式（保存する時に確かめているため、読めないのは記録が壊れた場合だけ）
    let template = Template::parse(&device.path_template).map_err(|_| {
        AppError::Validation(
            "デバイスの配置の書式を読めません。デバイスのページで、書式を直してください"
                .to_string(),
        )
    })?;
    let mut dir_cache = DirCache::default();
    // 変換できない環境（macOSでない・`afconvert`がない）では、設定によらず変換しない
    let transcode_setting = device.transcode;
    if transcode_setting.mode != TranscodeMode::Off && !crate::transcode::is_available() {
        log::warn!("Transcoding is not available; copying the original files");
    }
    let tracks: Vec<SourceTrack> = all_tracks
        .into_iter()
        .filter(|track| {
            selected_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(track.id.as_str()))
        })
        .map(|track| {
            let file = fs::metadata(&track.file_path)
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| SourceFile {
                    size: to_bytes(metadata.len()),
                    modified_at: modified_at_of(&metadata),
                });
            let transcode = file
                .is_some()
                .then(|| transcode_for(&transcode_setting, &track))
                .flatten();
            let path = track_relative_path(&track, &template, transcode);
            // 歌詞ファイル・フォルダの画像（元のファイルがある曲だけ。フォルダは1回だけ読む）
            let extras = if device.copy_extras && file.is_some() {
                source_extras(&mut dir_cache, Path::new(&track.file_path))
            } else {
                Vec::new()
            };
            SourceTrack {
                track,
                file,
                transcode,
                path,
                extras,
            }
        })
        .collect();

    let plan = plan_sync(
        &tracks,
        &playlists,
        &manifest,
        device.remove_unselected,
        |relative_path| {
            let metadata = fs::symlink_metadata(to_device_path(&root, relative_path)).ok()?;
            // フォルダなど、ファイル以外のものがある場所も使えない（どのサイズとも一致させない）
            Some(if metadata.is_file() {
                to_bytes(metadata.len())
            } else {
                -1
            })
        },
    );

    Ok(PreparedSync {
        device,
        root,
        manifest,
        plan,
    })
}

/// 曲を、転送する時にAACに変換するか（変換するなら、変換の設定。ADR-118）
///
/// 可逆の形式かは、拡張子（`.m4a`・`.ogg`は中のコーデック）で決める。ビットレートの条件では、
/// 可逆の形式の曲と、ライブラリに記録したビットレートが変換先より高い曲を変換する。変換できない
/// 環境では、変換しない。
fn transcode_for(setting: &DeviceTranscode, track: &TransferTrack) -> Option<Transcode> {
    if setting.mode == TranscodeMode::Off || !crate::transcode::is_available() {
        return None;
    }
    let is_lossless =
        || crate::transcode::is_lossless_source(Path::new(&track.file_path)) == Some(true);
    let converts = match setting.mode {
        TranscodeMode::Off => false,
        TranscodeMode::Lossless => is_lossless(),
        TranscodeMode::AboveBitrate => {
            track
                .bitrate
                .is_some_and(|bitrate| i64::from(bitrate) > i64::from(setting.bitrate_kbps))
                || is_lossless()
        }
    };
    converts.then_some(Transcode {
        bitrate_kbps: setting.bitrate_kbps,
    })
}

/// 曲に添えてコピーするファイル（同じ名前の歌詞ファイルと、フォルダの画像。ADR-112）
///
/// フォルダの画像は、アルバムアートの表示と同じ名前の優先順で、いちばん先の1つ（大きすぎる画像は使わない）。
fn source_extras(dir_cache: &mut DirCache, track_path: &Path) -> Vec<SourceExtra> {
    let lyrics = dir_cache
        .lyrics_file(track_path)
        .map(|path| (ExtraKind::Lyrics, path));
    let image = track_path
        .parent()
        .and_then(|dir| dir_cache.folder_image(dir))
        .map(|path| (ExtraKind::FolderImage, path));
    lyrics
        .into_iter()
        .chain(image)
        .filter_map(|(kind, path)| {
            let metadata = fs::metadata(&path).ok().filter(|m| m.is_file())?;
            if kind == ExtraKind::FolderImage && metadata.len() > MAX_ALBUM_ART_BYTES {
                return None;
            }
            Some(SourceExtra {
                kind,
                source: path.to_str()?.to_string(),
                file: SourceFile {
                    size: to_bytes(metadata.len()),
                    modified_at: modified_at_of(&metadata),
                },
            })
        })
        .collect()
}

/// 差分から、件数と必要な容量をまとめる
fn summarize_plan(plan: &SyncPlan, free_bytes: i64) -> AppResult<DeviceSyncPlan> {
    let copy_bytes = plan.copy_bytes();
    let extra_copy_bytes = plan.extra_copy_bytes();
    // 削除を先に行うため、削除で空く容量を差し引く
    let required_bytes =
        (copy_bytes + extra_copy_bytes - plan.delete_bytes - plan.extra_delete_bytes).max(0);
    Ok(DeviceSyncPlan {
        copy_count: to_count(plan.to_copy.len())?,
        copy_bytes,
        delete_count: to_count(plan.deleted_files().count())?,
        delete_bytes: plan.delete_bytes,
        rename_count: to_count(plan.to_rename.len())?,
        unchanged_count: to_count(plan.kept.len())?,
        playlist_count: to_count(plan.playlists.len())?,
        transcode_count: to_count(plan.transcode_count())?,
        missing_source_count: plan.missing_source_count,
        extra_copy_count: to_count(plan.extras_to_copy.len())?,
        extra_copy_bytes,
        extra_delete_count: to_count(plan.extras_to_delete.len())?,
        free_bytes,
        required_bytes,
        has_enough_space: required_bytes == 0
            || required_bytes.saturating_add(SPACE_MARGIN_BYTES) <= free_bytes,
        copy_paths: plan
            .to_copy
            .iter()
            .take(PLAN_ITEMS_LIMIT)
            .map(|op| op.dest.clone())
            .collect(),
        delete_paths: plan
            .deleted_files()
            .take(PLAN_ITEMS_LIMIT)
            .map(|entry| entry.path.clone())
            .collect(),
        renames: plan
            .to_rename
            .iter()
            .take(PLAN_ITEMS_LIMIT)
            .map(|op| SyncRename {
                from: op.entry.path.clone(),
                to: op.to.clone(),
            })
            .collect(),
    })
}

/// 同期する内容の曲数と、ライブラリでのサイズの合計（デバイスには触れない。ADR-113）
fn selection_summary(state: &AppState, device_id: &str) -> AppResult<SyncSelectionSummary> {
    let context = state.smart_playlist_context();
    state.with_db(|db| {
        let device = crate::device::find_device(db, device_id)?;
        let tracks = crate::repository::find_transfer_tracks(db)?;
        let selected: Option<HashSet<String>> = if device.sync_all {
            None
        } else {
            Some(
                source_playlists(db, &device, context)?
                    .into_iter()
                    .flat_map(|playlist| playlist.track_ids)
                    .collect(),
            )
        };
        let (count, bytes) = tracks
            .iter()
            .filter(|track| selected.as_ref().is_none_or(|ids| ids.contains(&track.id)))
            .fold((0usize, 0i64), |(count, bytes), track| {
                (count + 1, bytes.saturating_add(track.file_size))
            });
        Ok(SyncSelectionSummary {
            track_count: to_count(count)?,
            total_bytes: bytes,
        })
    })
}

/// 同期する内容の曲数と、ライブラリでのサイズの合計を返す（デバイスのページの見通し。ADR-113）
///
/// デバイスに触れず、ライブラリの記録から求める（接続されていなくても求められる）。
#[tauri::command]
#[specta::specta]
pub async fn get_sync_selection_summary(
    device_id: String,
    app_handle: AppHandle,
) -> AppResult<SyncSelectionSummary> {
    validate_device_id(&device_id)?;
    run_blocking(move || selection_summary(app_handle.state::<AppState>().inner(), &device_id))
        .await
}

/// デバイスのドライブを取り出す（macOS。同期の後に使う。ADR-113）
///
/// 転送先のフォルダが`/Volumes`の下のドライブにある時だけ使える（ほかは`VALIDATION`）。同期の
/// 実行中は`LOCK`。ほかのアプリがファイルを開いているなどで取り出せない時は`IO`。
#[tauri::command]
#[specta::specta]
pub async fn eject_sync_device(device_id: String, app_handle: AppHandle) -> AppResult<()> {
    validate_device_id(&device_id)?;
    run_blocking(move || {
        if app_handle.state::<DeviceSyncState>().is_running() {
            return Err(AppError::Lock(
                "同期の実行中は、デバイスを取り出せません".to_string(),
            ));
        }
        let device = app_handle
            .state::<AppState>()
            .with_db(|db| crate::device::find_device(db, &device_id))?;
        let mount_point = crate::device::volume_mount_point(Path::new(&device.path))
            .filter(|_| cfg!(target_os = "macos"))
            .ok_or_else(|| {
                AppError::Validation("このデバイスは、取り出せる場所にありません".to_string())
            })?;
        eject_volume(&mount_point)
    })
    .await
}

/// ドライブを取り出す（`diskutil eject`。macOSに付いているコマンド）
fn eject_volume(mount_point: &Path) -> AppResult<()> {
    if !mount_point.is_dir() {
        return Err(AppError::NotFound(
            "デバイスが見つかりません（すでに取り出されている可能性があります）".to_string(),
        ));
    }
    let output = std::process::Command::new("/usr/sbin/diskutil")
        .arg("eject")
        .arg(mount_point)
        .output()
        .map_err(|e| AppError::Io(format!("デバイスを取り出せませんでした: {}", e)))?;
    if output.status.success() {
        log::info!("Ejected the device volume: {}", mount_point.display());
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        log::warn!(
            "Failed to eject the device volume {}: {}",
            mount_point.display(),
            detail
        );
        Err(AppError::Io(
            "デバイスを取り出せませんでした（ほかのアプリがデバイスのファイルを開いている可能性があります）"
                .to_string(),
        ))
    }
}

/// デバイスの空き容量（バイト）
fn free_space(root: &Path) -> AppResult<i64> {
    fs4::available_space(root)
        .map(to_bytes)
        .map_err(|e| AppError::Io(format!("デバイスの空き容量を取得できません: {}", e)))
}

/// 同期で行う処理の件数と必要な容量を調べる（デバイスには書き込まない）
///
/// デバイスが接続されていない場合は`NOT_FOUND`。
#[tauri::command]
#[specta::specta]
pub async fn plan_device_sync(
    device_id: String,
    app_handle: AppHandle,
) -> AppResult<DeviceSyncPlan> {
    validate_device_id(&device_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let prepared = prepare_sync(state.inner(), &device_id)?;
        summarize_plan(&prepared.plan, free_space(&prepared.root)?)
    })
    .await
}

/// デバイスへ同期する（削除 → リネーム → コピー → プレイリストの書き出し）
///
/// `DeviceSyncProgress`を送る。同時に実行できる同期は1つで、実行中は`LOCK`。
/// 同期する対象を選んでいない場合と、空き容量が足りない場合は`VALIDATION`。
/// ファイル単位の失敗は結果の`errors`に入れて続ける。
#[tauri::command]
#[specta::specta]
pub async fn run_device_sync(
    device_id: String,
    app_handle: AppHandle,
) -> AppResult<DeviceSyncResult> {
    validate_device_id(&device_id)?;

    run_blocking(move || {
        let sync_state = app_handle.state::<DeviceSyncState>();
        let _running = sync_state
            .begin()
            .ok_or_else(|| AppError::Lock("他のデバイスへの同期が実行中です".to_string()))?;
        let state = app_handle.state::<AppState>();

        sync_device(
            state.inner(),
            &device_id,
            sync_state.cancel_flag(),
            &mut |progress| {
                let event = DeviceSyncProgress {
                    device_id: device_id.clone(),
                    current: progress.current,
                    total: progress.total,
                    bytes_done: progress.bytes_done,
                    bytes_total: progress.bytes_total,
                    current_file: progress.current_file,
                    converting: progress.converting,
                };
                if let Err(e) = event.emit(&app_handle) {
                    log::warn!("Failed to emit sync progress event: {}", e);
                }
            },
        )
    })
    .await
}

/// `run_device_sync`の本体（同期処理）
fn sync_device(
    state: &AppState,
    device_id: &str,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(TransferProgress),
) -> AppResult<DeviceSyncResult> {
    let prepared = prepare_sync(state, device_id)?;
    if prepared.device.has_no_source() {
        return Err(AppError::Validation(
            "同期する対象（全曲またはプレイリスト）を選択してください".to_string(),
        ));
    }
    let summary = summarize_plan(&prepared.plan, free_space(&prepared.root)?)?;
    if !summary.has_enough_space {
        return Err(AppError::Validation(
            "デバイスの空き容量が足りません".to_string(),
        ));
    }

    let outcome = execute_plan(
        &prepared.root,
        prepared.manifest,
        &prepared.plan,
        cancel,
        on_progress,
    )?;

    if !outcome.cancelled {
        state.with_db(|db| crate::device::mark_synced(db, device_id))?;
    }
    log::info!(
        "Synced to device: {} copied={}, deleted={}, renamed={}, extras copied={}, extras deleted={}, playlists={}, errors={}, cancelled={}",
        prepared.device.path,
        outcome.copied_count,
        outcome.deleted_count,
        outcome.renamed_count,
        outcome.extra_copied_count,
        outcome.extra_deleted_count,
        outcome.playlist_count,
        outcome.errors.len(),
        outcome.cancelled
    );

    Ok(DeviceSyncResult {
        copied_count: outcome.copied_count,
        deleted_count: outcome.deleted_count,
        renamed_count: outcome.renamed_count,
        extra_copied_count: outcome.extra_copied_count,
        extra_deleted_count: outcome.extra_deleted_count,
        playlist_count: outcome.playlist_count,
        error_count: to_count(outcome.errors.len())?,
        errors: outcome.errors,
        cancelled: outcome.cancelled,
    })
}

/// 実行中の同期を中止する（コピー中のファイルの途中でも止める。実行中でなければ何もしない）
#[tauri::command]
#[specta::specta]
pub async fn cancel_device_sync(sync_state: State<'_, DeviceSyncState>) -> AppResult<()> {
    sync_state.request_cancel();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::PlaylistFormat;
    use crate::device::{find_device, update_device_config};

    #[test]
    fn test_transcode_for_follows_the_setting() {
        let mut track = TransferTrack {
            id: "t1".to_string(),
            file_path: "/music/a.flac".to_string(),
            title: None,
            artist: None,
            album: None,
            album_artist: None,
            compilation: false,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration: Some(200),
            file_size: 1,
            bitrate: Some(900),
        };
        let setting = |mode| DeviceTranscode {
            mode,
            bitrate_kbps: 256,
        };
        let aac = Some(Transcode { bitrate_kbps: 256 });
        assert_eq!(transcode_for(&setting(TranscodeMode::Off), &track), None);
        // 変換できない環境では、設定によらず変換しない
        if !crate::transcode::is_available() {
            assert_eq!(
                transcode_for(&setting(TranscodeMode::Lossless), &track),
                None
            );
            return;
        }
        assert_eq!(
            transcode_for(&setting(TranscodeMode::Lossless), &track),
            aac
        );
        assert_eq!(
            transcode_for(&setting(TranscodeMode::AboveBitrate), &track),
            aac
        );

        // ビットレートの高い非可逆の曲は、ビットレートの条件の時だけ変換する
        track.file_path = "/music/a.mp3".to_string();
        track.bitrate = Some(320);
        assert_eq!(
            transcode_for(&setting(TranscodeMode::Lossless), &track),
            None
        );
        assert_eq!(
            transcode_for(&setting(TranscodeMode::AboveBitrate), &track),
            aac
        );
        track.bitrate = Some(256);
        assert_eq!(
            transcode_for(&setting(TranscodeMode::AboveBitrate), &track),
            None
        );
    }
    use crate::device_manifest::test_support::TempDir;
    use crate::models::{ReplayGain, Track};

    /// ライブラリのフォルダ（実ファイルを置く）と、デバイスのフォルダ
    struct Fixture {
        _dir: TempDir,
        state: AppState,
        library: PathBuf,
        device_root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = TempDir::new();
            let library = dir.path().join("library");
            let device_root = dir.path().join("device");
            fs::create_dir_all(&library).unwrap();
            fs::create_dir_all(&device_root).unwrap();

            let conn = crate::db::open_in_memory();
            Self {
                _dir: dir,
                state: AppState::new(conn),
                library,
                device_root,
            }
        }

        /// ライブラリにファイルを置き、トラックとして登録する
        fn add_track(
            &self,
            id: &str,
            artist: &str,
            album: &str,
            number: i32,
            title: &str,
        ) -> String {
            let file_name = format!("{}.mp3", id);
            let path = self.library.join(&file_name);
            fs::write(&path, format!("audio data of {}", title)).unwrap();
            let track = Track {
                cue: None,
                id: id.to_string(),
                file_path: path.to_str().unwrap().to_string(),
                file_name,
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: None,
                is_missing: false,
                compilation: false,
                genre: None,
                year: None,
                track_number: Some(number),
                disc_number: None,
                duration: Some(200),
                file_size: 0,
                format: "MP3".to_string(),
                bitrate: None,
                sample_rate: None,
                is_favorite: false,
                rating: 0,
                play_count: 0,
                skip_count: 0,
                last_played_at: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                replay_gain: ReplayGain::default(),
                sort_tags: Default::default(),
                details: Default::default(),
            };
            self.state
                .with_db(|db| crate::repository::insert_track(db, &track, None))
                .unwrap();
            id.to_string()
        }

        fn add_playlist(&self, name: &str, track_ids: &[&str]) -> String {
            self.state
                .with_db(|db| {
                    let playlist = crate::playlist::create_playlist(db, name).unwrap();
                    let track_ids: Vec<String> =
                        track_ids.iter().map(|id| id.to_string()).collect();
                    crate::playlist::add_tracks_to_playlist(db, &playlist.id, &track_ids).unwrap();
                    Ok(playlist.id)
                })
                .unwrap()
        }

        /// デバイスを登録し、同期する対象を設定する
        fn register(&self, sync_all: bool, playlist_ids: &[&str]) -> String {
            let device_id = register_device(&self.state, &self.device_root, "SD").unwrap();
            self.configure(&device_id, sync_all, playlist_ids, true);
            device_id
        }

        fn configure(
            &self,
            device_id: &str,
            sync_all: bool,
            playlist_ids: &[&str],
            remove_unselected: bool,
        ) {
            let config = SyncDeviceConfig {
                name: "SD".to_string(),
                sync_all,
                playlist_ids: playlist_ids.iter().map(|id| id.to_string()).collect(),
                smart_playlist_ids: Vec::new(),
                remove_unselected,
                path_template: crate::device::DEFAULT_DEVICE_TEMPLATE.to_string(),
                copy_extras: true,
                transcode: DeviceTranscode::default(),
                playlist_format: PlaylistFormat::default(),
            };
            self.state
                .with_db(|db| update_device_config(db, device_id, &config))
                .unwrap();
        }

        fn sync(&self, device_id: &str) -> DeviceSyncResult {
            sync_device(&self.state, device_id, &AtomicBool::new(false), &mut |_| {}).unwrap()
        }

        fn plan(&self, device_id: &str) -> DeviceSyncPlan {
            let prepared = prepare_sync(&self.state, device_id).unwrap();
            summarize_plan(&prepared.plan, free_space(&prepared.root).unwrap()).unwrap()
        }

        fn on_device(&self, relative_path: &str) -> Option<String> {
            fs::read_to_string(to_device_path(&self.device_root, relative_path)).ok()
        }
    }

    #[test]
    fn test_sync_all_copies_tracks_and_second_sync_changes_nothing() {
        let fixture = Fixture::new();
        fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        fixture.add_track(
            "00000000-0000-0000-0000-000000000002",
            "AC/DC",
            "Live: 1992",
            2,
            "Two?",
        );
        let device_id = fixture.register(true, &[]);

        let plan = fixture.plan(&device_id);
        assert_eq!((plan.copy_count, plan.delete_count), (2, 0));
        assert!(plan.has_enough_space);

        let result = fixture.sync(&device_id);
        assert_eq!((result.copied_count, result.error_count), (2, 0));
        assert!(!result.cancelled);
        assert_eq!(
            fixture
                .on_device("Music/Artist/Album/01 One.mp3")
                .as_deref(),
            Some("audio data of One")
        );
        // ファイル名に使えない文字は置き換える
        assert_eq!(
            fixture
                .on_device("Music/AC_DC/Live_ 1992/02 Two_.mp3")
                .as_deref(),
            Some("audio data of Two?")
        );
        let device = fixture
            .state
            .with_db(|db| find_device(db, &device_id))
            .unwrap();
        assert!(device.last_synced_at.is_some());

        // もう一度同期しても、コピーし直さない
        let plan = fixture.plan(&device_id);
        assert_eq!(
            (plan.copy_count, plan.delete_count, plan.unchanged_count),
            (0, 0, 2)
        );
        assert_eq!(fixture.sync(&device_id).copied_count, 0);
    }

    #[test]
    fn test_plan_lists_paths_and_selection_summary_uses_the_library() {
        let fixture = Fixture::new();
        let one = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        fixture.add_track(
            "00000000-0000-0000-0000-000000000002",
            "Artist",
            "Album",
            2,
            "Two",
        );
        fixture
            .state
            .with_db(|db| {
                db.execute("UPDATE tracks SET file_size = 300", [])
                    .map_err(|e| AppError::Database(e.to_string()))
            })
            .unwrap();
        let playlist = fixture.add_playlist("Car", &[&one]);
        let device_id = fixture.register(false, &[&playlist]);

        // 確認の画面の一覧は、コピーする曲のデバイス上の場所
        let plan = fixture.plan(&device_id);
        assert_eq!(plan.copy_paths, ["Music/Artist/Album/01 One.mp3"]);
        assert!(plan.delete_paths.is_empty() && plan.renames.is_empty());

        // 同期する内容の見通しは、ライブラリの記録のサイズ（デバイスに触れない）
        let summary = selection_summary(&fixture.state, &device_id).unwrap();
        assert_eq!((summary.track_count, summary.total_bytes), (1, 300));
        fixture.configure(&device_id, true, &[], true);
        let summary = selection_summary(&fixture.state, &device_id).unwrap();
        assert_eq!((summary.track_count, summary.total_bytes), (2, 600));
    }

    #[test]
    fn test_sync_copies_lyrics_and_folder_image_and_removes_them_when_turned_off() {
        let fixture = Fixture::new();
        let id = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        fs::write(fixture.library.join(format!("{}.LRC", id)), "[00:01.00]la").unwrap();
        // フォルダの画像は、中身がJPEGかPNGのものだけを使う（表示と同じ。先頭がJPEGの印）
        fs::write(fixture.library.join("cover.jpg"), b"\xFF\xD8\xFFjpeg").unwrap();
        let device_id = fixture.register(true, &[]);

        let plan = fixture.plan(&device_id);
        assert_eq!((plan.copy_count, plan.extra_copy_count), (1, 2));
        assert_eq!(plan.extra_copy_bytes, 12 + 7);
        let result = fixture.sync(&device_id);
        assert_eq!((result.extra_copied_count, result.error_count), (2, 0));
        assert_eq!(
            fixture
                .on_device("Music/Artist/Album/01 One.lrc")
                .as_deref(),
            Some("[00:01.00]la")
        );
        assert_eq!(
            fs::read(to_device_path(
                &fixture.device_root,
                "Music/Artist/Album/cover.jpg"
            ))
            .unwrap(),
            b"\xFF\xD8\xFFjpeg"
        );
        // もう一度同期しても、コピーし直さない
        assert_eq!(fixture.plan(&device_id).extra_copy_count, 0);

        // コピーしない設定にすると、コピーした歌詞ファイル・画像を消す（曲は残す）
        let config = SyncDeviceConfig {
            name: "SD".to_string(),
            sync_all: true,
            playlist_ids: Vec::new(),
            smart_playlist_ids: Vec::new(),
            remove_unselected: true,
            path_template: crate::device::DEFAULT_DEVICE_TEMPLATE.to_string(),
            copy_extras: false,
            transcode: DeviceTranscode::default(),
            playlist_format: PlaylistFormat::default(),
        };
        fixture
            .state
            .with_db(|db| update_device_config(db, &device_id, &config))
            .unwrap();
        let plan = fixture.plan(&device_id);
        assert_eq!((plan.extra_delete_count, plan.unchanged_count), (2, 1));
        assert_eq!(fixture.sync(&device_id).extra_deleted_count, 2);
        assert_eq!(fixture.on_device("Music/Artist/Album/01 One.lrc"), None);
        assert_eq!(fixture.on_device("Music/Artist/Album/cover.jpg"), None);
        assert!(fixture.on_device("Music/Artist/Album/01 One.mp3").is_some());
    }

    #[test]
    fn test_sync_playlists_writes_m3u8_and_removes_unselected_tracks() {
        let fixture = Fixture::new();
        let one = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let two = fixture.add_track(
            "00000000-0000-0000-0000-000000000002",
            "Artist",
            "Album",
            2,
            "Two",
        );
        fixture.add_track(
            "00000000-0000-0000-0000-000000000003",
            "Artist",
            "Album",
            3,
            "Not Selected",
        );
        let commute = fixture.add_playlist("通勤", &[&two, &one]);
        let drive = fixture.add_playlist("ドライブ", &[&one]);
        let device_id = fixture.register(false, &[&commute, &drive]);
        // ユーザーが自分で置いたファイル
        fs::write(fixture.device_root.join("my notes.txt"), "mine").unwrap();

        let result = fixture.sync(&device_id);
        assert_eq!((result.copied_count, result.playlist_count), (2, 2));
        assert_eq!(
            fixture.on_device("Music/Artist/Album/03 Not Selected.mp3"),
            None
        );
        // プレイリストは`Playlists`フォルダに、曲順どおりに、そのフォルダからの相対パスで書く
        assert_eq!(
            fixture.on_device("Playlists/通勤.m3u8").as_deref(),
            Some(
                "#EXTM3U\r\n\
                 #EXTINF:200,Artist - Two\r\n../Music/Artist/Album/02 Two.mp3\r\n\
                 #EXTINF:200,Artist - One\r\n../Music/Artist/Album/01 One.mp3\r\n"
            )
        );

        // 「通勤」を対象から外すと、そのプレイリストだけに入っていた曲とファイルを削除する
        fixture.configure(&device_id, false, &[&drive], true);
        let plan = fixture.plan(&device_id);
        assert_eq!(
            (plan.copy_count, plan.delete_count, plan.unchanged_count),
            (0, 1, 1)
        );
        let result = fixture.sync(&device_id);
        assert_eq!(result.deleted_count, 1);
        assert_eq!(fixture.on_device("Music/Artist/Album/02 Two.mp3"), None);
        assert!(fixture.on_device("Music/Artist/Album/01 One.mp3").is_some());
        assert_eq!(fixture.on_device("Playlists/通勤.m3u8"), None);
        assert!(fixture.on_device("Playlists/ドライブ.m3u8").is_some());
        // 管理外のファイルには触れない
        assert_eq!(fixture.on_device("my notes.txt").as_deref(), Some("mine"));
    }

    #[test]
    fn test_sync_all_writes_every_playlist() {
        let fixture = Fixture::new();
        let one = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let commute = fixture.add_playlist("通勤", &[&one]);
        fixture.add_playlist("ドライブ", &[&one]);
        // 全曲を同期する時は、選んだプレイリストの記録によらず、すべてのプレイリストを書き出す
        let device_id = fixture.register(true, &[&commute]);

        assert_eq!(fixture.plan(&device_id).playlist_count, 2);
        let result = fixture.sync(&device_id);
        assert_eq!((result.copied_count, result.playlist_count), (1, 2));
        for name in ["Playlists/通勤.m3u8", "Playlists/ドライブ.m3u8"] {
            assert_eq!(
                fixture.on_device(name).as_deref(),
                Some("#EXTM3U\r\n#EXTINF:200,Artist - One\r\n../Music/Artist/Album/01 One.mp3\r\n")
            );
        }
    }

    /// オートプレイリストは、同期する時の条件に合う曲を転送する
    #[test]
    fn test_sync_smart_playlist_copies_the_tracks_matching_the_rules() {
        use crate::smart_playlist::{
            MatchMode, NumberField, NumberOp, SmartOrder, SmartOrderField, SmartRule, SmartRules,
        };

        let fixture = Fixture::new();
        let one = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let two = fixture.add_track(
            "00000000-0000-0000-0000-000000000002",
            "Artist",
            "Album",
            2,
            "Two",
        );
        let set_rating = |track_id: &str, rating: i32| {
            fixture
                .state
                .with_db(|db| crate::repository::set_track_rating(db, track_id, rating))
                .unwrap();
        };
        set_rating(&one, 5);
        let rules = SmartRules {
            match_mode: MatchMode::All,
            rules: vec![SmartRule::Number {
                field: NumberField::Rating,
                op: NumberOp::AtLeast,
                value: 4,
                value_to: None,
            }],
            order: SmartOrder {
                field: SmartOrderField::Title,
                descending: false,
            },
            limit: None,
        };
        let smart_playlist_id = fixture
            .state
            .with_db(|db| {
                Ok(
                    crate::smart_playlist::create_smart_playlist(db, "高評価", &rules)
                        .unwrap()
                        .id,
                )
            })
            .unwrap();
        let device_id = fixture.register(false, &[]);
        let config = SyncDeviceConfig {
            name: "SD".to_string(),
            sync_all: false,
            playlist_ids: Vec::new(),
            smart_playlist_ids: vec![smart_playlist_id],
            remove_unselected: true,
            path_template: crate::device::DEFAULT_DEVICE_TEMPLATE.to_string(),
            copy_extras: true,
            transcode: DeviceTranscode::default(),
            playlist_format: PlaylistFormat::default(),
        };
        fixture
            .state
            .with_db(|db| update_device_config(db, &device_id, &config))
            .unwrap();
        assert_eq!(
            selection_summary(&fixture.state, &device_id)
                .unwrap()
                .track_count,
            1
        );

        let result = fixture.sync(&device_id);
        assert_eq!((result.copied_count, result.playlist_count), (1, 1));
        assert_eq!(
            fixture.on_device("Playlists/高評価.m3u8").as_deref(),
            Some("#EXTM3U\r\n#EXTINF:200,Artist - One\r\n../Music/Artist/Album/01 One.mp3\r\n")
        );

        // 評価を変えると、次の同期で曲が入れ替わる
        set_rating(&one, 2);
        set_rating(&two, 4);
        let result = fixture.sync(&device_id);
        assert_eq!((result.copied_count, result.deleted_count), (1, 1));
        assert_eq!(fixture.on_device("Music/Artist/Album/01 One.mp3"), None);
        assert_eq!(
            fixture.on_device("Playlists/高評価.m3u8").as_deref(),
            Some("#EXTM3U\r\n#EXTINF:200,Artist - Two\r\n../Music/Artist/Album/02 Two.mp3\r\n")
        );
    }

    #[test]
    fn test_sync_recopies_changed_source_file() {
        let fixture = Fixture::new();
        let id = fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let device_id = fixture.register(true, &[]);
        fixture.sync(&device_id);

        // 元のファイルを書き換える（サイズが変わる）
        fs::write(
            fixture.library.join(format!("{}.mp3", id)),
            "edited audio data (longer than before)",
        )
        .unwrap();

        assert_eq!(fixture.plan(&device_id).copy_count, 1);
        fixture.sync(&device_id);
        assert_eq!(
            fixture
                .on_device("Music/Artist/Album/01 One.mp3")
                .as_deref(),
            Some("edited audio data (longer than before)")
        );
    }

    #[test]
    fn test_sync_requires_source_and_connection() {
        let fixture = Fixture::new();
        fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let device_id = fixture.register(false, &[]);
        let run = |fixture: &Fixture| {
            sync_device(
                &fixture.state,
                &device_id,
                &AtomicBool::new(false),
                &mut |_| {},
            )
        };

        // 同期する対象を選んでいない
        assert!(matches!(run(&fixture), Err(AppError::Validation(_))));

        // デバイスが外れている（転送先のフォルダがない）
        fixture.configure(&device_id, true, &[], true);
        fs::remove_dir_all(&fixture.device_root).unwrap();
        assert!(matches!(run(&fixture), Err(AppError::NotFound(_))));
        assert!(
            !to_sync_device(
                fixture
                    .state
                    .with_db(|db| find_device(db, &device_id))
                    .unwrap()
            )
            .connected
        );
    }

    #[test]
    fn test_register_adopts_existing_manifest() {
        let fixture = Fixture::new();
        fixture.add_track(
            "00000000-0000-0000-0000-000000000001",
            "Artist",
            "Album",
            1,
            "One",
        );
        let device_id = fixture.register(true, &[]);
        fixture.sync(&device_id);

        // 同じフォルダをもう一度登録しても、同じデバイスのまま
        assert_eq!(
            register_device(&fixture.state, &fixture.device_root, "別の名前").unwrap(),
            device_id
        );
        assert_eq!(
            fixture
                .state
                .with_db(|db| crate::device::find_all_devices(db))
                .unwrap()
                .len(),
            1
        );

        // 登録を解除してから登録し直すと、管理ファイルを引き継ぐ（コピー済みの曲をコピーし直さない）
        fixture
            .state
            .with_db(|db| crate::device::delete_device(db, &device_id))
            .unwrap();
        assert_eq!(
            register_device(&fixture.state, &fixture.device_root, "SD").unwrap(),
            device_id
        );
        fixture.configure(&device_id, true, &[], true);
        let plan = fixture.plan(&device_id);
        assert_eq!((plan.copy_count, plan.unchanged_count), (0, 1));
    }

    #[test]
    fn test_register_rejects_folder_overlapping_library() {
        let fixture = Fixture::new();
        fixture
            .state
            .with_db(|db| {
                crate::library_folder::register_folder(db, fixture.library.to_str().unwrap(), &[])
            })
            .unwrap();
        let inside = fixture.library.join("device");
        fs::create_dir_all(&inside).unwrap();

        // ライブラリフォルダ自身・その中・それを含むフォルダは転送先にできない
        for path in [
            fixture.library.as_path(),
            inside.as_path(),
            fixture.library.parent().unwrap(),
        ] {
            assert!(matches!(
                register_device(&fixture.state, path, "SD"),
                Err(AppError::Validation(_))
            ));
        }
        assert!(matches!(
            register_device(&fixture.state, &inside.join("missing"), "SD"),
            Err(AppError::NotFound(_))
        ));
    }

    /// シンボリックリンクを解決すると重なるフォルダも、転送先にできない
    #[cfg(unix)]
    #[test]
    fn test_register_rejects_folder_overlapping_library_through_symlinks() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let base = fixture.device_root.parent().unwrap().to_path_buf();
        let inside = fixture.library.join("device");
        fs::create_dir_all(&inside).unwrap();
        // ライブラリフォルダの中を指すリンク
        let link_to_inside = base.join("link-to-inside");
        symlink(&inside, &link_to_inside).unwrap();
        // リンクのパスで記録したライブラリフォルダ（リンクの先は`real/library`）
        let real = base.join("real");
        fs::create_dir_all(real.join("library")).unwrap();
        let link_to_real = base.join("link-to-real");
        symlink(&real, &link_to_real).unwrap();
        fixture
            .state
            .with_db(|db| {
                crate::library_folder::register_folder(db, fixture.library.to_str().unwrap(), &[])?;
                crate::library_folder::register_folder(
                    db,
                    link_to_real.join("library").to_str().unwrap(),
                    &[],
                )
            })
            .unwrap();

        for path in [&link_to_inside, &real] {
            assert!(matches!(
                register_device(&fixture.state, path, "SD"),
                Err(AppError::Validation(_))
            ));
        }
        // 重ならないフォルダは、転送先にできる
        assert!(register_device(&fixture.state, &fixture.device_root, "SD").is_ok());
    }

    #[test]
    fn test_relink_moves_device_and_rejects_other_device_folder() {
        let fixture = Fixture::new();
        let device_id = fixture.register(true, &[]);

        // 管理ファイルのないフォルダ（初期化したカードなど）: このデバイスの管理ファイルを作る
        let fresh = fixture.device_root.parent().unwrap().join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        relink_device(&fixture.state, &device_id, &fresh).unwrap();
        let device = fixture
            .state
            .with_db(|db| find_device(db, &device_id))
            .unwrap();
        assert_eq!(device.path, fresh.to_str().unwrap());
        assert!(to_sync_device(device).connected);

        // 別のデバイスの管理ファイルがあるフォルダは選べない
        let other = fixture.device_root.parent().unwrap().join("other");
        fs::create_dir_all(&other).unwrap();
        let other_id = register_device(&fixture.state, &other, "Other").unwrap();
        assert_ne!(other_id, device_id);
        assert!(matches!(
            relink_device(&fixture.state, &device_id, &other),
            Err(AppError::Validation(_))
        ));
    }
}
