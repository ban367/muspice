//! ライブラリフォルダ（インポートしたフォルダ）の記録と、再スキャンの差分の計算
//!
//! インポートしたフォルダを`library_folders`に記録し、再スキャンでフォルダ内の
//! ファイルの追加・削除・変更をライブラリに反映する。
//!
//! - トラックとフォルダの対応は、`tracks.file_path`がフォルダのパスで始まるかで判定する
//!   （トラックにフォルダのIDは持たせない）
//! - フォルダ同士は入れ子にしない。登録済みのフォルダの中をインポートしても新しく登録せず、
//!   登録済みのフォルダを含むフォルダをインポートした場合は、中のフォルダの記録をまとめる
//! - 再スキャンで、ファイルが見つからなくなったトラックはライブラリから外す。ただし
//!   フォルダ自体が見つからない、または音楽ファイルが1件も見つからない場合は外さない
//!   （外付けドライブが外れている場合などに、ライブラリを誤って空にしないため）

use crate::error::{AppError, AppResult};
use crate::repository::TrackFileState;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::{MAIN_SEPARATOR, Path};
use uuid::Uuid;

/// ライブラリフォルダ（一覧表示用）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolder {
    pub id: String,
    pub path: String,
    /// フォルダ内のトラック数
    pub track_count: u32,
    /// フォルダが今も存在するか（外付けドライブが外れている場合などはfalse）
    pub exists: bool,
    pub added_at: String,
    pub last_scanned_at: Option<String>,
}

/// ライブラリフォルダの一覧
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolderList {
    pub folders: Vec<LibraryFolder>,
    /// どのライブラリフォルダにも属さないトラック数（フォルダの記録を始める前に
    /// インポートした曲など。同じフォルダをインポートし直すと登録される）
    pub unregistered_track_count: u32,
}

/// 再スキャンの結果
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RescanResult {
    /// 新しく追加したトラック数
    pub added_count: u32,
    /// 変更を読み直したトラック数
    pub updated_count: u32,
    /// ファイルが見つからなくなり、ライブラリから外したトラック数
    pub removed_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
    /// 音楽ファイルが1件も見つからなかったため、ライブラリから外さなかった
    pub removal_skipped: bool,
}

/// 記録しているライブラリフォルダ（DBの行）
#[derive(Debug, Clone, PartialEq)]
pub struct FolderRecord {
    pub id: String,
    pub path: String,
    pub added_at: String,
    pub last_scanned_at: Option<String>,
}

/// フォルダのパスを記録用の形にそろえる（末尾の区切り文字を除く。ルートはそのまま）
pub fn normalize_folder_path(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    // "/"や"C:\\"のようなルートは区切り文字を除くと別の意味になるため、そのまま残す
    if trimmed.is_empty() || trimmed.ends_with(':') {
        path.to_string()
    } else {
        trimmed.to_string()
    }
}

/// フォルダ内のファイルのパスに共通する接頭辞（フォルダのパス＋区切り文字）
///
/// 単純な前方一致では`/Music`が`/Music2/a.mp3`にも一致するため、区切り文字まで含める。
pub fn track_path_prefix(folder_path: &str) -> String {
    if folder_path.ends_with(['/', '\\']) {
        folder_path.to_string()
    } else {
        format!("{}{}", folder_path, MAIN_SEPARATOR)
    }
}

/// `path`が`folder`自身か、その中にあるか（パスの要素単位で比較する）
fn is_same_or_within(path: &str, folder: &str) -> bool {
    Path::new(path).starts_with(Path::new(folder))
}

/// 記録しているライブラリフォルダをパス順に取得する
pub fn find_all_folders(conn: &Connection) -> AppResult<Vec<FolderRecord>> {
    let mut stmt = conn
        .prepare("SELECT id, path, added_at, last_scanned_at FROM library_folders ORDER BY path")
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([], |row| {
        Ok(FolderRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            added_at: row.get(2)?,
            last_scanned_at: row.get(3)?,
        })
    })
    .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// IDでライブラリフォルダを取得する
pub fn find_folder(conn: &Connection, folder_id: &str) -> AppResult<FolderRecord> {
    conn.query_row(
        "SELECT id, path, added_at, last_scanned_at FROM library_folders WHERE id = ?1",
        [folder_id],
        |row| {
            Ok(FolderRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                added_at: row.get(2)?,
                last_scanned_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::Database(format!("ライブラリフォルダの取得に失敗しました: {}", e)))?
    .ok_or_else(|| AppError::NotFound("ライブラリフォルダが見つかりません".to_string()))
}

/// インポートしたフォルダを記録する（スキャンした日時も記録する）
///
/// - 登録済みのフォルダの中（または同じフォルダ）なら、新しく登録しない。同じフォルダの
///   場合はスキャンした日時だけ更新する
/// - 登録済みのフォルダを含むフォルダなら、中のフォルダの記録を消してまとめる
pub fn register_folder(conn: &Connection, folder_path: &str) -> AppResult<()> {
    let path = normalize_folder_path(folder_path);
    let now = Utc::now().to_rfc3339();
    let existing = find_all_folders(conn)?;

    if let Some(same) = existing.iter().find(|f| f.path == path) {
        return mark_scanned(conn, &same.id);
    }
    if existing.iter().any(|f| is_same_or_within(&path, &f.path)) {
        return Ok(());
    }

    for nested in existing
        .iter()
        .filter(|f| is_same_or_within(&f.path, &path))
    {
        delete_folder(conn, &nested.id)?;
    }

    conn.execute(
        "INSERT INTO library_folders (id, path, added_at, last_scanned_at) VALUES (?1, ?2, ?3, ?3)",
        rusqlite::params![Uuid::new_v4().to_string(), path, now],
    )
    .map_err(|e| AppError::Database(format!("ライブラリフォルダの登録に失敗しました: {}", e)))?;
    Ok(())
}

/// ライブラリフォルダの記録を削除する（トラックは消さない）
pub fn delete_folder(conn: &Connection, folder_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM library_folders WHERE id = ?1", [folder_id])
        .map_err(|e| {
            AppError::Database(format!("ライブラリフォルダの削除に失敗しました: {}", e))
        })?;
    Ok(())
}

/// スキャンした日時を記録する
pub fn mark_scanned(conn: &Connection, folder_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE library_folders SET last_scanned_at = ?2 WHERE id = ?1",
        rusqlite::params![folder_id, Utc::now().to_rfc3339()],
    )
    .map_err(|e| AppError::Database(format!("スキャン日時の記録に失敗しました: {}", e)))?;
    Ok(())
}

/// ディスク上の音楽ファイル（再スキャンで見つかったもの）
#[derive(Debug, Clone, PartialEq)]
pub struct DiskFile {
    pub path: String,
    /// ファイルサイズと更新日時（UNIX時間の秒）。取得できなかった場合はNone
    pub size: Option<i64>,
    pub modified_at: Option<i64>,
}

/// 再スキャンで行う処理
#[derive(Debug, Default, PartialEq)]
pub struct RescanPlan {
    /// ライブラリにないファイル（読み込んで追加する）
    pub to_add: Vec<DiskFile>,
    /// サイズか更新日時が変わったファイル（読み直す）
    pub to_update: Vec<DiskFile>,
    /// 更新日時が未記録か、夏時間の切り替えでずれたトラック（読み直さず、日時だけ記録する）:
    /// (トラックID, 更新日時)
    pub to_record_modified_at: Vec<(String, i64)>,
    /// ファイルが見つからなくなったトラックのID
    pub to_remove: Vec<String>,
    /// 音楽ファイルが1件も見つからなかったため、ライブラリから外さなかった
    pub removal_skipped: bool,
}

/// 夏時間の切り替えで、更新日時をローカル時刻で記録するファイルシステム（FAT32など）の
/// 更新日時がずれる幅（秒）
const DST_SHIFT_SECONDS: i64 = 3600;

/// FAT系の更新日時の精度（2秒）を見込んだ許容差（秒）
const MTIME_TOLERANCE_SECONDS: i64 = 2;

/// 更新日時の違いが、夏時間の切り替えによるずれ（ちょうど1時間）か
pub(crate) fn is_dst_shift(recorded: i64, current: i64) -> bool {
    ((current - recorded).abs() - DST_SHIFT_SECONDS).abs() <= MTIME_TOLERANCE_SECONDS
}

/// ディスク上のファイルとライブラリのトラックを比べて、再スキャンで行う処理を決める
///
/// 読み直すのはサイズか更新日時が変わったファイルだけにする（ファイルが変わっていない曲の、
/// DBだけで編集したメタデータを上書きしないため）。サイズ・更新日時を取得できなかった
/// ファイルは、変更なしとして扱う（ライブラリからも外さない）。
///
/// サイズが同じで更新日時がちょうど1時間ずれたファイルは、夏時間の切り替えによるずれ
/// （外付けドライブのFAT32など）とみなし、読み直さずに日時だけ記録する（夏時間の切り替えの
/// たびに、ドライブ上の全曲を読み直して編集を上書きしないため）。
pub fn plan_rescan(disk_files: Vec<DiskFile>, tracks: Vec<TrackFileState>) -> RescanPlan {
    let mut plan = RescanPlan::default();
    let tracks_by_path: HashMap<&str, &TrackFileState> =
        tracks.iter().map(|t| (t.file_path.as_str(), t)).collect();
    let disk_paths: HashSet<&str> = disk_files.iter().map(|f| f.path.as_str()).collect();

    for file in &disk_files {
        let Some(track) = tracks_by_path.get(file.path.as_str()) else {
            plan.to_add.push(file.clone());
            continue;
        };
        let (Some(size), Some(modified_at)) = (file.size, file.modified_at) else {
            continue;
        };
        if size != track.file_size {
            plan.to_update.push(file.clone());
        } else {
            match track.file_modified_at {
                None => plan
                    .to_record_modified_at
                    .push((track.id.clone(), modified_at)),
                Some(recorded) if recorded == modified_at => {}
                Some(recorded) if is_dst_shift(recorded, modified_at) => plan
                    .to_record_modified_at
                    .push((track.id.clone(), modified_at)),
                Some(_) => plan.to_update.push(file.clone()),
            }
        }
    }

    let missing: Vec<String> = tracks
        .iter()
        .filter(|t| !disk_paths.contains(t.file_path.as_str()))
        .map(|t| t.id.clone())
        .collect();
    if disk_files.is_empty() && !missing.is_empty() {
        plan.removal_skipped = true;
    } else {
        plan.to_remove = missing;
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("インメモリDB作成に失敗");
        crate::db::run_migrations(&conn).expect("マイグレーション実行に失敗");
        conn
    }

    fn folder_paths(conn: &Connection) -> Vec<String> {
        find_all_folders(conn)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect()
    }

    fn disk(path: &str, size: i64, modified_at: i64) -> DiskFile {
        DiskFile {
            path: path.to_string(),
            size: Some(size),
            modified_at: Some(modified_at),
        }
    }

    fn track(id: &str, path: &str, size: i64, modified_at: Option<i64>) -> TrackFileState {
        TrackFileState {
            id: id.to_string(),
            file_path: path.to_string(),
            file_size: size,
            file_modified_at: modified_at,
        }
    }

    #[test]
    fn test_normalize_folder_path() {
        assert_eq!(normalize_folder_path("/music/"), "/music");
        assert_eq!(normalize_folder_path("/music"), "/music");
        assert_eq!(normalize_folder_path("C:\\Music\\"), "C:\\Music");
        // ルートは区切り文字を残す
        assert_eq!(normalize_folder_path("/"), "/");
        assert_eq!(normalize_folder_path("C:\\"), "C:\\");
    }

    #[test]
    fn test_track_path_prefix_does_not_match_sibling_folder() {
        let prefix = track_path_prefix("/music");
        assert!(format!("/music{}a.mp3", MAIN_SEPARATOR).starts_with(&prefix));
        assert!(!"/music2/a.mp3".starts_with(&prefix));
        // ルートには区切り文字を足さない
        assert_eq!(track_path_prefix("/"), "/");
    }

    #[test]
    fn test_register_folder_merges_nested_folders() {
        let conn = setup_test_db();

        register_folder(&conn, "/music/jazz/").unwrap();
        register_folder(&conn, "/music/rock").unwrap();
        register_folder(&conn, "/other").unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/rock", "/other"]
        );

        // 登録済みのフォルダの中は登録しない
        register_folder(&conn, "/music/jazz/live").unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/rock", "/other"]
        );

        // 名前が前方一致するだけの別のフォルダは中とみなさない
        register_folder(&conn, "/music/jazz2").unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/jazz2", "/music/rock", "/other"]
        );

        // 登録済みのフォルダを含むフォルダは、中のフォルダの記録をまとめる
        register_folder(&conn, "/music").unwrap();
        assert_eq!(folder_paths(&conn), ["/music", "/other"]);
    }

    #[test]
    fn test_register_same_folder_updates_scanned_at_only() {
        let conn = setup_test_db();
        register_folder(&conn, "/music").unwrap();
        let first = find_all_folders(&conn).unwrap().remove(0);

        conn.execute("UPDATE library_folders SET last_scanned_at = NULL", [])
            .unwrap();
        register_folder(&conn, "/music/").unwrap();

        let folders = find_all_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, first.id);
        assert!(folders[0].last_scanned_at.is_some());
    }

    #[test]
    fn test_find_and_delete_folder() {
        let conn = setup_test_db();
        register_folder(&conn, "/music").unwrap();
        let id = find_all_folders(&conn).unwrap()[0].id.clone();

        assert_eq!(find_folder(&conn, &id).unwrap().path, "/music");
        delete_folder(&conn, &id).unwrap();
        assert!(matches!(
            find_folder(&conn, &id),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn test_plan_rescan_classifies_files() {
        let plan = plan_rescan(
            vec![
                disk("/m/new.mp3", 100, 10),
                disk("/m/same.mp3", 200, 20),
                disk("/m/resized.mp3", 301, 30),
                disk("/m/touched.mp3", 400, 41),
                disk("/m/legacy.mp3", 500, 50),
            ],
            vec![
                track("same", "/m/same.mp3", 200, Some(20)),
                track("resized", "/m/resized.mp3", 300, Some(30)),
                track("touched", "/m/touched.mp3", 400, Some(40)),
                track("legacy", "/m/legacy.mp3", 500, None),
                track("gone", "/m/gone.mp3", 600, Some(60)),
            ],
        );

        let paths = |files: &[DiskFile]| files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&plan.to_add), ["/m/new.mp3"]);
        assert_eq!(paths(&plan.to_update), ["/m/resized.mp3", "/m/touched.mp3"]);
        // 更新日時が未記録のトラックは、サイズが同じなら読み直さずに日時だけ記録する
        assert_eq!(plan.to_record_modified_at, [("legacy".to_string(), 50)]);
        assert_eq!(plan.to_remove, ["gone"]);
        assert!(!plan.removal_skipped);
    }

    #[test]
    fn test_plan_rescan_treats_dst_shift_as_unchanged() {
        let tracks = vec![
            track("dst", "/m/dst.mp3", 100, Some(1_000_000)),
            track("dst-fat", "/m/dst-fat.mp3", 100, Some(1_000_000)),
            track("edited", "/m/edited.mp3", 100, Some(1_000_000)),
        ];
        let disk_files = vec![
            // 夏時間の切り替えでちょうど1時間ずれた（FAT系の2秒の精度を含む）
            disk("/m/dst.mp3", 100, 1_000_000 - 3600),
            disk("/m/dst-fat.mp3", 100, 1_000_000 + 3602),
            // 1時間と違うずれは、変更として読み直す
            disk("/m/edited.mp3", 100, 1_000_000 + 3700),
        ];

        let plan = plan_rescan(disk_files, tracks);

        assert_eq!(
            plan.to_record_modified_at,
            vec![
                ("dst".to_string(), 1_000_000 - 3600),
                ("dst-fat".to_string(), 1_000_000 + 3602)
            ]
        );
        let updated: Vec<&str> = plan.to_update.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(updated, ["/m/edited.mp3"]);
    }

    #[test]
    fn test_plan_rescan_resized_legacy_track_is_reread() {
        let plan = plan_rescan(
            vec![disk("/m/a.mp3", 101, 10)],
            vec![track("a", "/m/a.mp3", 100, None)],
        );
        assert_eq!(plan.to_update.len(), 1);
        assert!(plan.to_record_modified_at.is_empty());
    }

    #[test]
    fn test_plan_rescan_keeps_tracks_when_no_files_found() {
        // 外付けドライブの空のマウントポイントなどで、ライブラリを空にしない
        let plan = plan_rescan(vec![], vec![track("a", "/m/a.mp3", 100, Some(1))]);
        assert!(plan.to_remove.is_empty());
        assert!(plan.removal_skipped);

        // ライブラリにも曲がなければ、外すものがないだけ
        let plan = plan_rescan(vec![], vec![]);
        assert!(!plan.removal_skipped);
    }

    #[test]
    fn test_plan_rescan_treats_unreadable_metadata_as_unchanged() {
        let plan = plan_rescan(
            vec![DiskFile {
                path: "/m/a.mp3".to_string(),
                size: None,
                modified_at: None,
            }],
            vec![track("a", "/m/a.mp3", 100, Some(1))],
        );
        assert_eq!(plan, RescanPlan::default());
    }
}
