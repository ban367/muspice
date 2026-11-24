//! 転送先デバイス（SDカード・USBメモリなどのフォルダ）の記録
//!
//! デバイスごとに、転送先のフォルダと同期する対象（全曲か、選んだプレイリスト）を
//! `sync_devices`・`sync_device_playlists`・`sync_device_smart_playlists`に記録する。
//!
//! - デバイス上にコピーしたファイルの一覧は、DBではなくデバイス側の管理ファイルに記録する
//!   （`device_manifest`）。DBを作り直した場合や、別のPCから同期する場合も引き継げる
//! - 同期は一方向（ライブラリ → デバイス）。差分の計算は`device_sync`、コピーの実行は
//!   `device_transfer`が行う

use crate::error::{AppError, AppResult};
use crate::settings::DEFAULT_MUSIC_FOLDER_TEMPLATE;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};

/// デバイス名の長さの上限（文字数）
const MAX_DEVICE_NAME_LENGTH: usize = 100;

/// 新しく登録するデバイスの配置の書式（音楽フォルダの既定の書式と同じ。アルバムアーティストで分ける。ADR-111）
pub const DEFAULT_DEVICE_TEMPLATE: &str = DEFAULT_MUSIC_FOLDER_TEMPLATE;

/// デバイスへ転送する時に、曲をAACに変換する条件（ADR-118）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TranscodeMode {
    /// 変換しない（元のファイルをコピーする）
    Off,
    /// 可逆の形式（FLAC・ALAC・WAV・AIFF・WavPack・APE）の曲を変換する
    Lossless,
    /// 可逆の形式の曲と、ビットレートが変換先より高い曲を変換する
    AboveBitrate,
}

impl TranscodeMode {
    /// DBに保存する値
    fn as_db(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lossless => "lossless",
            Self::AboveBitrate => "aboveBitrate",
        }
    }

    /// DBの値から読む（知らない値は、変換しない）
    fn from_db(value: &str) -> Self {
        match value {
            "lossless" => Self::Lossless,
            "aboveBitrate" => Self::AboveBitrate,
            _ => Self::Off,
        }
    }
}

/// デバイスへ転送する時の変換の設定（ADR-118）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTranscode {
    pub mode: TranscodeMode,
    /// 変換先のAACのビットレート（kbps。`transcode::AAC_BITRATES_KBPS`のどれか）
    pub bitrate_kbps: u32,
}

impl Default for DeviceTranscode {
    fn default() -> Self {
        Self {
            mode: TranscodeMode::Off,
            bitrate_kbps: crate::transcode::DEFAULT_AAC_BITRATE_KBPS,
        }
    }
}

/// 変換の設定を確かめる（ビットレートは選べる値のどれか。変換できない環境では、変換する設定にできない）
pub fn validate_transcode(transcode: &DeviceTranscode) -> AppResult<()> {
    if !crate::transcode::AAC_BITRATES_KBPS.contains(&transcode.bitrate_kbps) {
        return Err(AppError::Validation(
            "変換のビットレートは、128・192・256・320 kbpsのどれかにしてください".to_string(),
        ));
    }
    if transcode.mode != TranscodeMode::Off && !crate::transcode::is_available() {
        return Err(AppError::Validation(
            "この環境では、転送する時に曲を変換できません".to_string(),
        ));
    }
    Ok(())
}

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
    /// 同期するオートプレイリスト（プレイリストと同じく扱う。曲は、同期する時の条件に合う曲。ADR-127）
    pub smart_playlist_ids: Vec<String>,
    /// 同期の対象から外れた曲を、デバイスから削除するか
    pub remove_unselected: bool,
    /// デバイス上の配置の書式（ファイルの整理と同じ書き方。ADR-111）
    pub path_template: String,
    /// 曲に添えて、歌詞ファイル・フォルダの画像もコピーするか（ADR-112）
    pub copy_extras: bool,
    /// 転送する時の変換（ADR-118）
    pub transcode: DeviceTranscode,
    /// 取り出せるか（macOSで、転送先のフォルダが`/Volumes`の下のドライブにあり、接続されている。ADR-113）
    pub ejectable: bool,
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

/// デバイスの設定（名前と同期する対象と、デバイス上の配置の書式）
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDeviceConfig {
    pub name: String,
    pub sync_all: bool,
    pub playlist_ids: Vec<String>,
    pub smart_playlist_ids: Vec<String>,
    pub remove_unselected: bool,
    pub path_template: String,
    pub copy_extras: bool,
    pub transcode: DeviceTranscode,
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
    /// コピーする曲のうち、AACに変換する曲数（変換する曲のサイズは、ビットレートと長さからの見積もり。ADR-118）
    pub transcode_count: u32,
    /// 元のファイルが見つからない曲数（コピーできない。コピー済みならデバイスに残す）
    pub missing_source_count: u32,
    /// コピー・削除する、曲に添えるファイル（歌詞ファイル・フォルダの画像）の数と、コピーするサイズ（ADR-112）
    pub extra_copy_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub extra_copy_bytes: i64,
    pub extra_delete_count: u32,
    /// デバイスの空き容量と、同期で増える容量（バイト。曲に添えるファイルを含む）
    #[specta(type = specta_typescript::Number)]
    pub free_bytes: i64,
    #[specta(type = specta_typescript::Number)]
    pub required_bytes: i64,
    /// 空き容量が足りるか
    pub has_enough_space: bool,
    /// コピー・削除・名前を変える曲のデバイス上の場所（確認の画面で見せる。それぞれ先頭の
    /// `PLAN_ITEMS_LIMIT`件まで。件数は`copyCount`などを使う。ADR-113）
    pub copy_paths: Vec<String>,
    pub delete_paths: Vec<String>,
    pub renames: Vec<SyncRename>,
}

/// 確認の画面で見せる一覧の、それぞれの件数の上限
pub const PLAN_ITEMS_LIMIT: usize = 1000;

/// デバイス上で名前を変える曲（変更前と変更後の場所）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncRename {
    pub from: String,
    pub to: String,
}

/// 同期する内容の曲数と、ライブラリでのサイズの合計（デバイスのページで見せる見通し。ADR-113）
///
/// デバイスに触れずに、ライブラリの記録から求める（コピー済みかどうか・曲に添えるファイルは含めない）。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSelectionSummary {
    pub track_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub total_bytes: i64,
}

/// 同期の結果
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncResult {
    pub copied_count: u32,
    pub deleted_count: u32,
    pub renamed_count: u32,
    /// コピー・削除した、曲に添えるファイルの数
    pub extra_copied_count: u32,
    pub extra_deleted_count: u32,
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
    /// デバイス上の配置の書式
    pub path_template: String,
    /// 歌詞ファイル・フォルダの画像もコピーするか
    pub copy_extras: bool,
    /// 転送する時の変換
    pub transcode: DeviceTranscode,
    /// 同期するプレイリスト（削除済みのプレイリストは含まない）
    pub playlist_ids: Vec<String>,
    /// 同期するオートプレイリスト（削除済みのものは含まない）
    pub smart_playlist_ids: Vec<String>,
    pub created_at: String,
    pub last_synced_at: Option<String>,
}

impl DeviceRecord {
    /// 同期する対象が1つもないか（全曲でもなく、プレイリスト・オートプレイリストも選んでいない）
    pub fn has_no_source(&self) -> bool {
        !self.sync_all && self.playlist_ids.is_empty() && self.smart_playlist_ids.is_empty()
    }
}

/// 転送先のフォルダのあるドライブのマウント先（`/Volumes/<名前>`。macOSの外付けのドライブ）
///
/// `/Volumes`の下でないフォルダ（Macの中のフォルダ）はNone。
pub fn volume_mount_point(path: &Path) -> Option<std::path::PathBuf> {
    let mut components = path.components();
    match (components.next(), components.next(), components.next()) {
        (
            Some(Component::RootDir),
            Some(Component::Normal(volumes)),
            Some(Component::Normal(name)),
        ) if volumes == "Volumes" => Some(Path::new("/Volumes").join(name)),
        _ => None,
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

    /// 同期を実行中か
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
    if trimmed.chars().count() > MAX_DEVICE_NAME_LENGTH {
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

/// 転送先のフォルダから、デバイス名を決める（決められなければNone）
///
/// 取り外せるドライブ（macOSの`/Volumes/<名前>`、Linuxの`/media/<ユーザー>/<名前>`・
/// `/run/media/<ユーザー>/<名前>`）の中ならドライブの名前、それ以外はフォルダの名前にする。
/// ルート（`/`・Windowsのドライブ`E:\`）は名前にしない。
pub fn suggest_device_name(folder: &Path) -> Option<String> {
    let segments: Vec<&str> = folder
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let name = match segments.as_slice() {
        ["Volumes", name, ..] | ["media", _, name, ..] | ["run", "media", _, name, ..] => name,
        [.., last] => last,
        [] => return None,
    };
    // 登録できる名前にそろえる（制御文字を除き、長い名前は切る）
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_LENGTH)
        .collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

const DEVICE_COLUMNS: &str = "id, name, path, sync_all, remove_unselected, created_at, last_synced_at, path_template, copy_extras, transcode_mode, transcode_bitrate";

fn map_device_row(row: &rusqlite::Row) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        sync_all: row.get::<_, i32>(3)? != 0,
        remove_unselected: row.get::<_, i32>(4)? != 0,
        path_template: row.get(7)?,
        copy_extras: row.get::<_, i32>(8)? != 0,
        transcode: DeviceTranscode {
            mode: TranscodeMode::from_db(&row.get::<_, String>(9)?),
            bitrate_kbps: row.get(10)?,
        },
        playlist_ids: Vec::new(),
        smart_playlist_ids: Vec::new(),
        created_at: row.get(5)?,
        last_synced_at: row.get(6)?,
    })
}

/// デバイスに同期するプレイリスト・オートプレイリストのIDを、デバイスに入れる
///
/// どちらも作成日時の新しい順。プレイリストと結合し、削除済みのプレイリストを含めない。
fn load_device_playlists(conn: &Connection, device: &mut DeviceRecord) -> AppResult<()> {
    device.playlist_ids = query_ids(
        conn,
        "SELECT p.id FROM sync_device_playlists sp
         JOIN playlists p ON p.id = sp.playlist_id
         WHERE sp.device_id = ?1
         ORDER BY p.created_at DESC, p.id",
        &device.id,
    )?;
    device.smart_playlist_ids = query_ids(
        conn,
        "SELECT p.id FROM sync_device_smart_playlists sp
         JOIN smart_playlists p ON p.id = sp.smart_playlist_id
         WHERE sp.device_id = ?1
         ORDER BY p.created_at DESC, p.id",
        &device.id,
    )?;
    Ok(())
}

fn query_ids(conn: &Connection, sql: &str, device_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(sql)
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
        load_device_playlists(conn, device)?;
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
            load_device_playlists(conn, &mut device)?;
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

/// デバイスを登録する（同期する対象は未選択。対象から外れた曲は削除する設定。配置は既定の書式）
pub fn insert_device(conn: &Connection, device_id: &str, name: &str, path: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO sync_devices (id, name, path, created_at, path_template)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            device_id,
            name,
            path,
            crate::datetime::now_rfc3339(),
            DEFAULT_DEVICE_TEMPLATE
        ],
    )
    .map_err(|e| AppError::Database(format!("デバイスの登録に失敗しました: {}", e)))?;
    Ok(())
}

/// デバイスの設定（名前と同期する対象）を更新する
///
/// 見つからないプレイリスト・オートプレイリスト（選んだ後に削除された場合など）は無視する。
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
            "UPDATE sync_devices
             SET name = ?2, sync_all = ?3, remove_unselected = ?4, path_template = ?5,
                 copy_extras = ?6, transcode_mode = ?7, transcode_bitrate = ?8
             WHERE id = ?1",
            rusqlite::params![
                device_id,
                config.name,
                config.sync_all,
                config.remove_unselected,
                config.path_template,
                config.copy_extras,
                config.transcode.mode.as_db(),
                config.transcode.bitrate_kbps
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
    tx.execute(
        "DELETE FROM sync_device_smart_playlists WHERE device_id = ?1",
        [device_id],
    )
    .map_err(|e| AppError::Database(format!("デバイスの更新に失敗しました: {}", e)))?;
    for smart_playlist_id in &config.smart_playlist_ids {
        tx.execute(
            "INSERT OR IGNORE INTO sync_device_smart_playlists (device_id, smart_playlist_id)
             SELECT ?1, id FROM smart_playlists WHERE id = ?2",
            [device_id, smart_playlist_id.as_str()],
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
    for table in ["sync_device_playlists", "sync_device_smart_playlists"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE device_id = ?1"),
            [device_id],
        )
        .map_err(|e| AppError::Database(format!("デバイスの削除に失敗しました: {}", e)))?;
    }
    tx.execute("DELETE FROM sync_devices WHERE id = ?1", [device_id])
        .map_err(|e| AppError::Database(format!("デバイスの削除に失敗しました: {}", e)))?;
    tx.commit()
        .map_err(|e| AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e)))
}

/// 同期した日時を記録する
pub fn mark_synced(conn: &Connection, device_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_devices SET last_synced_at = ?2 WHERE id = ?1",
        rusqlite::params![device_id, crate::datetime::now_rfc3339()],
    )
    .map_err(|e| AppError::Database(format!("同期日時の記録に失敗しました: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_transcode() {
        let transcode = |mode, bitrate_kbps| DeviceTranscode { mode, bitrate_kbps };
        assert!(validate_transcode(&DeviceTranscode::default()).is_ok());
        assert!(validate_transcode(&transcode(TranscodeMode::Off, 100)).is_err());
        assert_eq!(
            validate_transcode(&transcode(TranscodeMode::Lossless, 192)).is_ok(),
            crate::transcode::is_available()
        );
    }

    #[test]
    fn test_transcode_setting_is_saved() {
        let mut conn = setup_test_db();
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();
        assert_eq!(
            find_device(&conn, "d1").unwrap().transcode,
            DeviceTranscode::default()
        );

        let transcode = DeviceTranscode {
            mode: TranscodeMode::AboveBitrate,
            bitrate_kbps: 128,
        };
        update_device_config(
            &mut conn,
            "d1",
            &SyncDeviceConfig {
                transcode,
                ..config("SD", true, &[])
            },
        )
        .unwrap();
        assert_eq!(find_device(&conn, "d1").unwrap().transcode, transcode);
    }

    fn setup_test_db() -> Connection {
        crate::db::open_in_memory()
    }

    fn config(name: &str, sync_all: bool, playlist_ids: &[&str]) -> SyncDeviceConfig {
        SyncDeviceConfig {
            name: name.to_string(),
            sync_all,
            playlist_ids: playlist_ids.iter().map(|id| id.to_string()).collect(),
            smart_playlist_ids: Vec::new(),
            remove_unselected: true,
            path_template: DEFAULT_DEVICE_TEMPLATE.to_string(),
            copy_extras: true,
            transcode: DeviceTranscode::default(),
        }
    }

    #[test]
    fn test_path_template_defaults_and_updates() {
        let mut conn = setup_test_db();
        // 新しく登録したデバイスは、既定の書式（アルバムアーティストで分ける）
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();
        assert_eq!(
            find_device(&conn, "d1").unwrap().path_template,
            DEFAULT_DEVICE_TEMPLATE
        );

        // 設定の更新で、書式も変わる
        let mut updated = config("SD", true, &[]);
        updated.path_template = "%artist%/%title%".to_string();
        update_device_config(&mut conn, "d1", &updated).unwrap();
        assert_eq!(
            find_device(&conn, "d1").unwrap().path_template,
            "%artist%/%title%"
        );
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

    /// オートプレイリストは、プレイリストとは別に選ぶ（消したものは選んでいないことにする）
    #[test]
    fn test_update_device_config_replaces_smart_playlists() {
        let mut conn = setup_test_db();
        insert_device(&conn, "d1", "SD", "/Volumes/SD").unwrap();
        let rules = crate::smart_playlist::SmartRules::unreadable();
        let smart = crate::smart_playlist::create_smart_playlist(&conn, "高評価", &rules).unwrap();
        let manual = crate::playlist::create_playlist(&conn, "通勤").unwrap();

        let mut selected = config("SD", false, &[]);
        // 種類を取り違えたIDは、選ばない
        selected.smart_playlist_ids = vec![smart.id.clone(), manual.id.clone()];
        update_device_config(&mut conn, "d1", &selected).unwrap();
        let device = find_device(&conn, "d1").unwrap();
        assert_eq!(device.smart_playlist_ids, [smart.id.clone()]);
        assert!(device.playlist_ids.is_empty());
        assert!(!device.has_no_source());

        crate::smart_playlist::delete_smart_playlist(&conn, &smart.id).unwrap();
        assert!(find_device(&conn, "d1").unwrap().has_no_source());
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
        // 長さは、バイト数ではなく文字数で数える
        assert!(validate_device_name(&"あ".repeat(100)).is_ok());
        assert!(validate_device_name(&"あ".repeat(101)).is_err());
        assert!(validate_device_name("SD\nカード").is_err());
    }

    #[test]
    fn test_volume_mount_point() {
        assert_eq!(
            volume_mount_point(Path::new("/Volumes/SDCARD/Music")),
            Some(std::path::PathBuf::from("/Volumes/SDCARD"))
        );
        assert_eq!(
            volume_mount_point(Path::new("/Volumes/WALKMAN")),
            Some(std::path::PathBuf::from("/Volumes/WALKMAN"))
        );
        // Macの中のフォルダ・`/Volumes`そのものは、取り出すドライブではない
        assert_eq!(volume_mount_point(Path::new("/Users/me/Music")), None);
        assert_eq!(volume_mount_point(Path::new("/Volumes")), None);
        assert_eq!(volume_mount_point(Path::new("relative/Volumes/X")), None);
    }

    #[test]
    fn test_suggest_device_name() {
        let suggest = |path: &str| suggest_device_name(Path::new(path));
        // 取り外せるドライブの中なら、ドライブの名前
        assert_eq!(suggest("/Volumes/SDCARD").as_deref(), Some("SDCARD"));
        assert_eq!(suggest("/Volumes/SDCARD/Music").as_deref(), Some("SDCARD"));
        assert_eq!(
            suggest("/media/demo/WALKMAN/MUSIC").as_deref(),
            Some("WALKMAN")
        );
        assert_eq!(
            suggest("/run/media/demo/WALKMAN").as_deref(),
            Some("WALKMAN")
        );
        // それ以外は、フォルダの名前
        assert_eq!(suggest("/Users/demo/Backup/").as_deref(), Some("Backup"));
        assert_eq!(suggest("/").as_deref(), None);
        // 登録できる名前にそろえる
        let long = format!("/Volumes/{}", "あ".repeat(120));
        let name = suggest(&long).unwrap();
        assert_eq!(name, "あ".repeat(100));
        assert!(validate_device_name(&name).is_ok());
        assert_eq!(suggest("/Volumes/ SD\tCARD ").as_deref(), Some("SDCARD"));
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
