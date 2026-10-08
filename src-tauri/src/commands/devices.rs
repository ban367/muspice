//! 転送先デバイス（SDカードなどのフォルダ）の登録・設定・同期のコマンド
//!
//! フォルダのパスを受け取るのは登録（`register_sync_device`）と転送先の変更
//! （`relink_sync_device`）だけで、それ以外はデバイスのIDからDBで解決する。

use super::run_blocking;
use crate::device::{
    DeviceRecord, DeviceSyncPlan, DeviceSyncResult, DeviceSyncState, SyncDevice, SyncDeviceConfig,
    validate_device_name,
};
use crate::device_manifest::{
    Manifest, read_device_id, read_manifest, to_device_path, write_manifest,
};
use crate::device_sync::{SourceFile, SourcePlaylist, SourceTrack, SyncPlan, plan_sync};
use crate::device_transfer::{TransferProgress, execute_plan};
use crate::error::{AppError, AppResult};
use crate::events::DeviceSyncProgress;
use crate::library::{modified_at_of, to_count};
use crate::library_folder::{find_all_folders, normalize_folder_path};
use crate::state::AppState;
use crate::validation::{validate_file_path, validate_playlist_id, validate_track_id};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use uuid::Uuid;

/// 空き容量の判定で、管理ファイルとプレイリストのために残しておく容量（バイト）
const SPACE_MARGIN_BYTES: i64 = 1024 * 1024;

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

    SyncDevice {
        id: record.id,
        name: record.name,
        path: record.path,
        sync_all: record.sync_all,
        playlist_ids: record.playlist_ids,
        remove_unselected: record.remove_unselected,
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
fn validate_device_folder(state: &AppState, folder_path: &str) -> AppResult<String> {
    validate_file_path(folder_path)?;
    let path = normalize_folder_path(folder_path);
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err(AppError::NotFound(format!(
            "フォルダが見つかりません: {}",
            path
        )));
    }

    let library_folders = state.with_db(|db| find_all_folders(db))?;
    let overlaps = library_folders.iter().any(|folder| {
        let library = Path::new(&folder.path);
        root.starts_with(library) || library.starts_with(root)
    });
    if overlaps {
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

/// フォルダを転送先デバイスとして登録する
///
/// フォルダに管理ファイルがある場合は、そのデバイスとして扱う（登録済みなら転送先を
/// 更新し、未登録なら管理ファイルを引き継いで登録する）。なければ新しく作る。
#[tauri::command]
#[specta::specta]
pub async fn register_sync_device(
    folder_path: String,
    name: String,
    app_handle: AppHandle,
) -> AppResult<SyncDevice> {
    let name = validate_device_name(&name)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let device_id = register_device(state.inner(), &folder_path, &name)?;
        load_sync_device(state.inner(), &device_id)
    })
    .await
}

/// `register_sync_device`の本体（同期処理）。登録したデバイスのIDを返す
fn register_device(state: &AppState, folder_path: &str, name: &str) -> AppResult<String> {
    let path = validate_device_folder(state, folder_path)?;
    let root = Path::new(&path);

    let device_id = match read_manifest(root)? {
        Some(manifest) => {
            validate_device_id(&manifest.device_id).map_err(|_| {
                AppError::Validation("デバイスの管理ファイルの内容が正しくありません".to_string())
            })?;
            state.with_db(
                |db| match crate::device::try_find_device(db, &manifest.device_id)? {
                    Some(_) => crate::device::set_device_path(db, &manifest.device_id, &path),
                    None => crate::device::insert_device(db, &manifest.device_id, name, &path),
                },
            )?;
            manifest.device_id
        }
        None => {
            let device_id = Uuid::new_v4().to_string();
            write_manifest(root, &Manifest::new(&device_id))?;
            state.with_db(|db| crate::device::insert_device(db, &device_id, name, &path))?;
            device_id
        }
    };

    log::info!("転送先デバイスを登録しました: {}", path);
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
    let config = SyncDeviceConfig {
        name: validate_device_name(&config.name)?,
        ..config
    };

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        state.with_db(|db| crate::device::update_device_config(db, &device_id, &config))?;
        load_sync_device(state.inner(), &device_id)
    })
    .await
}

/// デバイスの転送先のフォルダを変える（マウント先が変わった場合や、カードを初期化した場合）
///
/// フォルダに別のデバイスの管理ファイルがある場合はエラーにする。管理ファイルがなければ、
/// このデバイスの管理ファイルを新しく作る。
#[tauri::command]
#[specta::specta]
pub async fn relink_sync_device(
    device_id: String,
    folder_path: String,
    app_handle: AppHandle,
) -> AppResult<SyncDevice> {
    validate_device_id(&device_id)?;

    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        relink_device(state.inner(), &device_id, &folder_path)?;
        load_sync_device(state.inner(), &device_id)
    })
    .await
}

/// `relink_sync_device`の本体（同期処理）
fn relink_device(state: &AppState, device_id: &str, folder_path: &str) -> AppResult<()> {
    state.with_db(|db| crate::device::find_device(db, device_id))?;
    let path = validate_device_folder(state, folder_path)?;
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

/// デバイスの接続を確認し、同期する曲・プレイリストと管理ファイルから差分を計算する
fn prepare_sync(state: &AppState, device_id: &str) -> AppResult<PreparedSync> {
    let (device, all_tracks, all_playlists) = state.with_db(|db| {
        let device = crate::device::find_device(db, device_id)?;
        let tracks = crate::repository::find_transfer_tracks(db)?;
        let playlists = crate::playlist::get_all_playlists(db)
            .map_err(|e| AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)))?;
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

    let playlists: Vec<SourcePlaylist> = all_playlists
        .into_iter()
        .filter(|playlist| device.playlist_ids.contains(&playlist.id))
        .map(|playlist| SourcePlaylist {
            id: playlist.id,
            name: playlist.name,
            track_ids: playlist.tracks.into_iter().map(|t| t.track_id).collect(),
        })
        .collect();
    // 全曲を同期しない場合は、選んだプレイリストの曲だけにする
    let selected_ids: Option<HashSet<&str>> = (!device.sync_all).then(|| {
        playlists
            .iter()
            .flat_map(|playlist| playlist.track_ids.iter().map(String::as_str))
            .collect()
    });
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
            SourceTrack { track, file }
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

/// 差分から、件数と必要な容量をまとめる
fn summarize_plan(plan: &SyncPlan, free_bytes: i64) -> AppResult<DeviceSyncPlan> {
    let copy_bytes = plan.copy_bytes();
    // 削除を先に行うため、削除で空く容量を差し引く
    let required_bytes = (copy_bytes - plan.delete_bytes).max(0);
    Ok(DeviceSyncPlan {
        copy_count: to_count(plan.to_copy.len())?,
        copy_bytes,
        delete_count: to_count(plan.to_delete.len())?,
        delete_bytes: plan.delete_bytes,
        rename_count: to_count(plan.to_rename.len())?,
        unchanged_count: to_count(plan.kept.len())?,
        playlist_count: to_count(plan.playlists.len())?,
        missing_source_count: plan.missing_source_count,
        free_bytes,
        required_bytes,
        has_enough_space: required_bytes == 0
            || required_bytes.saturating_add(SPACE_MARGIN_BYTES) <= free_bytes,
    })
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
                };
                if let Err(e) = event.emit(&app_handle) {
                    log::warn!("同期の進捗イベントの送信に失敗しました: {}", e);
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
        "デバイスへ同期しました: {} コピー={}, 削除={}, リネーム={}, プレイリスト={}, エラー={}, 中止={}",
        prepared.device.path,
        outcome.copied_count,
        outcome.deleted_count,
        outcome.renamed_count,
        outcome.playlist_count,
        outcome.errors.len(),
        outcome.cancelled
    );

    Ok(DeviceSyncResult {
        copied_count: outcome.copied_count,
        deleted_count: outcome.deleted_count,
        renamed_count: outcome.renamed_count,
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
    use crate::device::{find_device, update_device_config};
    use crate::device_manifest::test_support::TempDir;
    use crate::models::{ReplayGain, Track};
    use rusqlite::Connection;

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

            let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
            crate::db::run_migrations(&conn).expect("マイグレーション実行に失敗");
            Self {
                _dir: dir,
                state: AppState::new(conn),
                library,
                device_root,
            }
        }

        fn device_path(&self) -> &str {
            self.device_root.to_str().unwrap()
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
                id: id.to_string(),
                file_path: path.to_str().unwrap().to_string(),
                file_name,
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: None,
                is_missing: false,
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
                last_played_at: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                replay_gain: ReplayGain::default(),
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
            let device_id = register_device(&self.state, self.device_path(), "SD").unwrap();
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
                remove_unselected,
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
            fixture.on_device("Artist/Album/01 One.mp3").as_deref(),
            Some("audio data of One")
        );
        // ファイル名に使えない文字は置き換える
        assert_eq!(
            fixture.on_device("AC_DC/Live_ 1992/02 Two_.mp3").as_deref(),
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
        assert_eq!(fixture.on_device("Artist/Album/03 Not Selected.mp3"), None);
        // プレイリストは曲順どおりに、デバイスのフォルダからの相対パスで書く
        assert_eq!(
            fixture.on_device("通勤.m3u8").as_deref(),
            Some(
                "#EXTM3U\r\n\
                 #EXTINF:200,Artist - Two\r\nArtist/Album/02 Two.mp3\r\n\
                 #EXTINF:200,Artist - One\r\nArtist/Album/01 One.mp3\r\n"
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
        assert_eq!(fixture.on_device("Artist/Album/02 Two.mp3"), None);
        assert!(fixture.on_device("Artist/Album/01 One.mp3").is_some());
        assert_eq!(fixture.on_device("通勤.m3u8"), None);
        assert!(fixture.on_device("ドライブ.m3u8").is_some());
        // 管理外のファイルには触れない
        assert_eq!(fixture.on_device("my notes.txt").as_deref(), Some("mine"));
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
            fixture.on_device("Artist/Album/01 One.mp3").as_deref(),
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
            register_device(&fixture.state, fixture.device_path(), "別の名前").unwrap(),
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
            register_device(&fixture.state, fixture.device_path(), "SD").unwrap(),
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
                crate::library_folder::register_folder(db, fixture.library.to_str().unwrap())
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
                register_device(&fixture.state, path.to_str().unwrap(), "SD"),
                Err(AppError::Validation(_))
            ));
        }
        assert!(matches!(
            register_device(
                &fixture.state,
                inside.join("missing").to_str().unwrap(),
                "SD"
            ),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_relink_moves_device_and_rejects_other_device_folder() {
        let fixture = Fixture::new();
        let device_id = fixture.register(true, &[]);

        // 管理ファイルのないフォルダ（初期化したカードなど）: このデバイスの管理ファイルを作る
        let fresh = fixture.device_root.parent().unwrap().join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        relink_device(&fixture.state, &device_id, fresh.to_str().unwrap()).unwrap();
        let device = fixture
            .state
            .with_db(|db| find_device(db, &device_id))
            .unwrap();
        assert_eq!(device.path, fresh.to_str().unwrap());
        assert!(to_sync_device(device).connected);

        // 別のデバイスの管理ファイルがあるフォルダは選べない
        let other = fixture.device_root.parent().unwrap().join("other");
        fs::create_dir_all(&other).unwrap();
        let other_id = register_device(&fixture.state, other.to_str().unwrap(), "Other").unwrap();
        assert_ne!(other_id, device_id);
        assert!(matches!(
            relink_device(&fixture.state, &device_id, other.to_str().unwrap()),
            Err(AppError::Validation(_))
        ));
    }
}
