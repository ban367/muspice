//! 転送先デバイス（SDカード・USBメモリなどのフォルダ）の記録
//!
//! デバイスごとに、転送先のフォルダと同期する対象（全曲か、選んだプレイリスト）を
//! `sync_devices`・`sync_device_playlists`に記録する。
//!
//! - デバイス上にコピーしたファイルの一覧は、DBではなくデバイス側の管理ファイルに記録する
//!   （`device_manifest`）。DBを作り直した場合や、別のPCから同期する場合も引き継げる
//! - 同期は一方向（ライブラリ → デバイス）。差分の計算は`device_sync`、コピーの実行は
//!   `device_transfer`が行う

use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};

/// デバイス名の長さの上限（バイト数）
const MAX_DEVICE_NAME_LENGTH: usize = 100;

/// 転送先デバイス（一覧表示用）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDevice {
    pub id: String,
    pub name: String,
    /// 転送先のフォルダ
    pub path: String,
    /// ライブラリの全曲を同期するか
    pub sync_all: bool,
    /// 同期するプレイリスト（`.m3u8`も書き出す。全曲を同期しない場合は、曲もここから決まる）
    pub playlist_ids: Vec<String>,
    /// 同期の対象から外れた曲を、デバイスから削除するか
    pub remove_unselected: bool,
    /// 接続されているか（転送先のフォルダに、このデバイスの管理ファイルがあるか）
    pub connected: bool,
    /// 空き容量・全体の容量（バイト）。接続されていない場合は値なし
    #[specta(type = Option<specta_typescript::Number>)]
    pub free_bytes: Option<i64>,
    #[specta(type = Option<specta_typescript::Number>)]
    pub total_bytes: Option<i64>,
    pub created_at: String,
    pub last_synced_at: Option<String>,
}

/// デバイスの設定（名前と同期する対象）
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDeviceConfig {
    pub name: String,
    pub sync_all: bool,
    pub playlist_ids: Vec<String>,
    pub remove_unselected: bool,
}

/// 同期で行う処理の件数と、必要な容量（同期の前の確認用）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncPlan {
    /// コピーする曲数とサイズの合計（バイト）
    pub copy_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub copy_bytes: i64,
    /// デバイスから削除する曲数とサイズの合計（バイト）
    pub delete_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub delete_bytes: i64,
    /// デバイス上で名前を変える曲数（タグが変わって配置が変わった曲）
    pub rename_count: u32,
    /// そのまま残す曲数
    pub unchanged_count: u32,
    /// 書き出すプレイリスト数
    pub playlist_count: u32,
    /// 元のファイルが見つからない曲数（コピーできない。コピー済みならデバイスに残す）
    pub missing_source_count: u32,
    /// デバイスの空き容量と、同期で増える容量（バイト）
    #[specta(type = specta_typescript::Number)]
    pub free_bytes: i64,
    #[specta(type = specta_typescript::Number)]
    pub required_bytes: i64,
    /// 空き容量が足りるか
    pub has_enough_space: bool,
}

/// 同期の結果
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncResult {
    pub copied_count: u32,
    pub deleted_count: u32,
    pub renamed_count: u32,
    /// 書き出したプレイリスト数
    pub playlist_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
    /// 途中で中止された
    pub cancelled: bool,
}

/// 記録しているデバイス（DBの行と、同期するプレイリスト）
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub sync_all: bool,
    pub remove_unselected: bool,
    /// 同期するプレイリスト（削除済みのプレイリストは含まない）
    pub playlist_ids: Vec<String>,
    pub created_at: String,
    pub last_synced_at: Option<String>,
}

impl DeviceRecord {
    /// 同期する対象が1つもないか（全曲でもなく、プレイリストも選んでいない）
    pub fn has_no_source(&self) -> bool {
        !self.sync_all && self.playlist_ids.is_empty()
    }
}

/// 同期の実行状態（同時に実行する同期は1つだけ）
#[derive(Default)]
pub struct DeviceSyncState {
    running: AtomicBool,
    cancel: AtomicBool,
}

impl DeviceSyncState {
    /// 同期を始める。他の同期が実行中ならNone
    ///
    /// 返したガードを破棄すると、実行中の印が外れる。
    pub fn begin(&self) -> Option<DeviceSyncGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        self.cancel.store(false, Ordering::SeqCst);
        Some(DeviceSyncGuard { state: self })
    }

    /// 実行中の同期を中止させる（実行中でなければ何もしない）
    pub fn request_cancel(&self) {
        if self.running.load(Ordering::SeqCst) {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// 中止の要求（コピーの途中で確認する）
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }
}

/// 同期の実行中の印（破棄すると外れる）
pub struct DeviceSyncGuard<'a> {
    state: &'a DeviceSyncState,
}

impl Drop for DeviceSyncGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}

/// デバイス名をバリデーションし、前後の空白を除いた名前を返す
pub fn validate_device_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "デバイス名を入力してください".to_string(),
        ));
    }
    if trimmed.len() > MAX_DEVICE_NAME_LENGTH {
        return Err(AppError::Validation(format!(
            "デバイス名は{}文字以内で入力してください",
            MAX_DEVICE_NAME_LENGTH
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "デバイス名に使用できない文字が含まれています".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

const DEVICE_COLUMNS: &str =
    "id, name, path, sync_all, remove_unselected, created_at, last_synced_at";

fn map_device_row(row: &rusqlite::Row) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        sync_all: row.get::<_, i32>(3)? != 0,
        remove_unselected: row.get::<_, i32>(4)? != 0,
        playlist_ids: Vec::new(),
        created_at: row.get(5)?,
        last_synced_at: row.get(6)?,
    })
}

/// デバイスに同期するプレイリストのIDを取得する（プレイリストの作成日時の新しい順）
///
/// プレイリストと結合し、削除済みのプレイリストを含めない。
fn find_device_playlist_ids(conn: &Connection, device_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id FROM sync_device_playlists sp
             JOIN playlists p ON p.id = sp.playlist_id
             WHERE sp.device_id = ?1
             ORDER BY p.created_at DESC, p.id",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([device_id], |row| row.get(0))
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// 記録しているデバイスを名前順に取得する
pub fn find_all_devices(conn: &Connection) -> AppResult<Vec<DeviceRecord>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM sync_devices ORDER BY name, created_at, id",
            DEVICE_COLUMNS
        ))
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    let mut devices = stmt
        .query_map([], map_device_row)
        .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))?;

    for device in &mut devices {
        device.playlist_ids = find_device_playlist_ids(conn, &device.id)?;
    }
    Ok(devices)
}

/// IDでデバイスを取得する（なければNone）
pub fn try_find_device(conn: &Connection, device_id: &str) -> AppResult<Option<DeviceRecord>> {
    let device = conn
        .query_row(
            &format!("SELECT {} FROM sync_devices WHERE id = ?1", DEVICE_COLUMNS),
            [device_id],
            map_device_row,
        )
        .optional()
        .map_err(|e| AppError::Database(format!("デバイスの取得に失敗しました: {}", e)))?;

    match device {
        Some(mut device) => {
            device.playlist_ids = find_device_playlist_ids(conn, &device.id)?;
            Ok(Some(device))
        }
        None => Ok(None),
    }
}

/// IDでデバイスを取得する
pub fn find_device(conn: &Connection, device_id: &str) -> AppResult<DeviceRecord> {
    try_find_device(conn, device_id)?
        .ok_or_else(|| AppError::NotFound("デバイスが見つかりません".to_string()))
}

/// デバイスを登録する（同期する対象は未選択。対象から外れた曲は削除する設定）
pub fn insert_device(conn: &Connection, device_id: &str, name: &str, path: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO sync_devices (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![device_id, name, path, Utc::now().to_rfc3339()],
    )
    .map_err(|e| AppError::Database(format!("デバイスの登録に失敗しました: {}", e)))?;
    Ok(())
}

/// デバイスの設定（名前と同期する対象）を更新する
///
/// 見つからないプレイリスト（選んだ後に削除された場合など）は無視する。
pub fn update_device_config(
    conn: &mut Connection,
    device_id: &str,
    config: &SyncDeviceConfig,
) -> AppResult<()> {
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    let updated = tx
        .execute(
            "UPDATE sync_devices SET name = ?2, sync_all = ?3, remove_unselected = ?4 WHERE id = ?1",
            rusqlite::params![
                device_id,
                config.name,
                config.sync_all,
                config.remove_unselected
            ],
        )
        .map_err(|e| AppError::Database(format!("デバイスの更新に失敗しました: {}", e)))?;
    if updated == 0 {
        return Err(AppError::NotFound("デバイスが見つかりません".to_string()));
    }

    tx.execute(
        "DELETE FROM sync_device_playlists WHERE device_id = ?1",
        [device_id],
    )
    .map_err(|e| AppError::Database(format!("デバイスの更新に失敗しました: {}", e)))?;
    for playlist_id in &config.playlist_ids {
        tx.execute(
            "INSERT OR IGNORE INTO sync_device_playlists (device_id, playlist_id)
             SELECT ?1, id FROM playlists WHERE id = ?2",
            [device_id, playlist_id.as_str()],
        )
        .map_err(|e| AppError::Database(format!("デバイスの更新に失敗しました: {}", e)))?;
    }

    tx.commit()
        .map_err(|e| AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e)))
}

/// 転送先のフォルダを記録する（マウント先が変わった場合など）
pub fn set_device_path(conn: &Connection, device_id: &str, path: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_devices SET path = ?2 WHERE id = ?1",
        [device_id, path],
    )
    .map_err(|e| AppError::Database(format!("転送先の記録に失敗しました: {}", e)))?;
    Ok(())
}

/// デバイスの記録を削除する（デバイス上のファイルは消さない）
pub fn delete_device(conn: &mut Connection, device_id: &str) -> AppResult<()> {
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;
    tx.execute(
        "DELETE FROM sync_device_playlists WHERE device_id = ?1",
        [device_id],
    )
    .map_err(|e| AppError::Database(format!("デバイスの削除に失敗しました: {}", e)))?;
    tx.execute("DELETE FROM sync_devices WHERE id = ?1", [device_id])
        .map_err(|e| AppError::Database(format!("デバイスの削除に失敗しました: {}", e)))?;
    tx.commit()
        .map_err(|e| AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e)))
}

/// 同期した日時を記録する
pub fn mark_synced(conn: &Connection, device_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_devices SET last_synced_at = ?2 WHERE id = ?1",
        rusqlite::params![device_id, Utc::now().to_rfc3339()],
    )
    .map_err(|e| AppError::Database(format!("同期日時の記録に失敗しました: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        crate::db::run_migrations(&conn).expect("マイグレーション実行に失敗");
        conn
    }

    fn config(name: &str, sync_all: bool, playlist_ids: &[&str]) -> SyncDeviceConfig {
        SyncDeviceConfig {
            name: name.to_string(),
            sync_all,
            playlist_ids: playlist_ids.iter().map(|id| id.to_string()).collect(),
            remove_unselected: true,
        }
    }

    #[test]
    fn test_insert_and_find_device() {
        let conn = setup_test_db();
        insert_device(&conn, "d1", "SDカード", "/Volumes/SD/Music").unwrap();

        let device = find_device(&conn, "d1").unwrap();
        assert_eq!(device.name, "SDカード");
        assert_eq!(device.path, "/Volumes/SD/Music");
        // 登録した直後は対象が未選択で、対象から外れた曲は削除する設定
        assert!(device.has_no_source());
        assert!(device.remove_unselected);
        assert_eq!(device.last_synced_at, None);

        assert!(matches!(
            find_device(&conn, "missing"),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_find_all_devices_orders_by_name() {
        let conn = setup_test_db();
        insert_device(&conn, "d1", "Walkman", "/Volumes/WALKMAN").unwrap();
        insert_device(&conn, "d2", "Car", "/Volumes/CAR").unwrap();

        let names: Vec<String> = find_all_devices(&conn)
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, ["Car", "Walkman"]);
    }

    #[test]
    fn test_update_device_config_replaces_playlists() {
        let mut conn = setup_test_db();
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();
        let p1 = crate::playlist::create_playlist(&conn, "通勤").unwrap();
        let p2 = crate::playlist::create_playlist(&conn, "ドライブ").unwrap();

        update_device_config(&mut conn, "d1", &config("SD", false, &[&p1.id, &p2.id])).unwrap();
        let mut ids = find_device(&conn, "d1").unwrap().playlist_ids;
        ids.sort();
        let mut expected = vec![p1.id.clone(), p2.id.clone()];
        expected.sort();
        assert_eq!(ids, expected);

        // 見つからないプレイリストは無視し、選び直した内容で置き換える
        update_device_config(
            &mut conn,
            "d1",
            &config("車", true, &[&p2.id, "missing-playlist"]),
        )
        .unwrap();
        let device = find_device(&conn, "d1").unwrap();
        assert_eq!(device.name, "車");
        assert!(device.sync_all);
        assert_eq!(device.playlist_ids, [p2.id]);
    }

    #[test]
    fn test_update_missing_device_is_not_found() {
        let mut conn = setup_test_db();
        assert!(matches!(
            update_device_config(&mut conn, "missing", &config("SD", true, &[])),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_deleted_playlist_is_not_listed() {
        let mut conn = setup_test_db();
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();
        let playlist = crate::playlist::create_playlist(&conn, "通勤").unwrap();
        update_device_config(&mut conn, "d1", &config("SD", false, &[&playlist.id])).unwrap();

        crate::playlist::delete_playlist(&conn, &playlist.id).unwrap();

        assert!(find_device(&conn, "d1").unwrap().has_no_source());
    }

    #[test]
    fn test_set_path_mark_synced_and_delete() {
        let mut conn = setup_test_db();
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();

        set_device_path(&conn, "d1", "/Volumes/SD 1").unwrap();
        mark_synced(&conn, "d1").unwrap();
        let device = find_device(&conn, "d1").unwrap();
        assert_eq!(device.path, "/Volumes/SD 1");
        assert!(device.last_synced_at.is_some());

        delete_device(&mut conn, "d1").unwrap();
        assert_eq!(try_find_device(&conn, "d1").unwrap(), None);
    }

    #[test]
    fn test_validate_device_name() {
        assert_eq!(validate_device_name("  SDカード ").unwrap(), "SDカード");
        assert!(validate_device_name("   ").is_err());
        assert!(validate_device_name(&"a".repeat(101)).is_err());
        assert!(validate_device_name("SD\nカード").is_err());
    }

    #[test]
    fn test_sync_state_allows_one_sync_at_a_time() {
        let state = DeviceSyncState::default();
        // 実行中でなければ、中止の要求は残らない
        state.request_cancel();
        assert!(!state.cancel_flag().load(Ordering::SeqCst));

        let guard = state.begin().expect("最初の同期は始められる");
        assert!(state.begin().is_none());
        state.request_cancel();
        assert!(state.cancel_flag().load(Ordering::SeqCst));

        // 終わったら次の同期を始められ、中止の要求は持ち越さない
        drop(guard);
        let _next = state.begin().expect("終わった後は始められる");
        assert!(!state.cancel_flag().load(Ordering::SeqCst));
    }
}
