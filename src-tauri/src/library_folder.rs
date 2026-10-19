//! ライブラリフォルダ（インポートしたフォルダ）の記録と、再スキャンの差分の計算
//!
//! インポートしたフォルダを`library_folders`に記録し、再スキャンでフォルダ内の
//! ファイルの追加・削除・変更をライブラリに反映する。
//!
//! - トラックとフォルダの対応は、`tracks.file_path`がフォルダのパスで始まるかで判定する
//!   （トラックにフォルダのIDは持たせない）
//! - フォルダ同士は入れ子にしない。登録済みのフォルダの中をインポートしても新しく登録せず、
//!   登録済みのフォルダを含むフォルダをインポートした場合は、中のフォルダの記録をまとめる
//! - 再スキャンで、ファイルが見つからなくなったトラックは外さず、「見つからない曲」として残す
//!   （移動・改名されたファイルが見つかれば、同じ曲として結び付ける。`track_relink`）。
//!   ただしフォルダ自体が見つからない、または音楽ファイルが1件も見つからない場合は、
//!   見つからない曲にもしない（外付けドライブが外れている場合などに、全曲を見つからない曲にしないため）
//! - フォルダごとに、読み込まないフォルダ（除外）をフォルダからの相対パスで記録する
//!   （`library_folder_exclusions`）。新しく登録する時は、直下にある既知のアプリのフォルダを入れておく
//!   （ADR-062）

use crate::error::{AppError, AppResult};
use crate::library::ScanExclusions;
use crate::repository::TrackFileState;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
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
    /// 読み込まないフォルダ（パス順）
    pub exclusions: Vec<FolderExclusion>,
}

/// ライブラリフォルダの、読み込まないフォルダ（除外。一覧表示用）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderExclusion {
    pub id: String,
    /// ライブラリフォルダからの相対パス
    pub path: String,
    /// 中にある、ライブラリの曲の数（除外する前に登録した曲。再スキャンでは変えない）
    pub track_count: u32,
}

/// ライブラリフォルダの一覧
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolderList {
    pub folders: Vec<LibraryFolder>,
    /// どのライブラリフォルダにも属さないトラック数（フォルダの記録を始める前に
    /// インポートした曲など。同じフォルダをインポートし直すと登録される）
    pub unregistered_track_count: u32,
    /// 見つからない曲の数（ファイルが見つからなくなり、利用者が外すまで残しているトラック）
    pub missing_track_count: u32,
}

/// 再スキャンの結果
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RescanResult {
    /// 新しく追加したトラック数
    pub added_count: u32,
    /// 変更を読み直したトラック数
    pub updated_count: u32,
    /// 移動・改名されたファイルに結び付けたトラック数（見つからない曲を、同じ曲として引き継いだ）
    pub relinked_count: u32,
    /// ファイルが見つからなくなり、見つからない曲にしたトラック数（ライブラリからは外さない）
    pub missing_count: u32,
    /// ファイルが同じ場所に戻り、見つかる曲に戻したトラック数
    pub restored_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
    /// 音楽ファイルが1件も見つからなかったため、見つからない曲にしなかった
    pub missing_skipped: bool,
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
///
/// `exclusions`（フォルダからの相対パス）は、新しく登録する時だけ、読み込まないフォルダとして
/// 記録する（`import_exclusions`で求めた`to_register`を渡す）。
pub fn register_folder(
    conn: &Connection,
    folder_path: &str,
    exclusions: &[String],
) -> AppResult<()> {
    let path = normalize_folder_path(folder_path);
    let now = Utc::now().to_rfc3339();
    let existing = find_all_folders(conn)?;

    if let Some(same) = existing.iter().find(|f| f.path == path) {
        return mark_scanned(conn, &same.id);
    }
    if existing.iter().any(|f| is_same_or_within(&path, &f.path)) {
        return Ok(());
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;
    // 中のフォルダの除外は、外のフォルダの除外として`exclusions`に入っている（記録は外部キーで消える）
    for nested in existing
        .iter()
        .filter(|f| is_same_or_within(&f.path, &path))
    {
        delete_folder(&tx, &nested.id)?;
    }

    let folder_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO library_folders (id, path, added_at, last_scanned_at) VALUES (?1, ?2, ?3, ?3)",
        rusqlite::params![folder_id, path, now],
    )
    .map_err(|e| AppError::Database(format!("ライブラリフォルダの登録に失敗しました: {}", e)))?;
    for exclusion in exclusions {
        insert_exclusion(&tx, &folder_id, exclusion)?;
    }

    tx.commit()
        .map_err(|e| AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e)))
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

/// 記録している、読み込まないフォルダ（DBの行）
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionRecord {
    pub id: String,
    pub folder_id: String,
    /// ライブラリフォルダからの相対パス
    pub path: String,
}

/// 読み込まないフォルダの絶対パス（ライブラリフォルダのパス＋相対パス）
pub fn excluded_path(folder_path: &str, relative: &str) -> PathBuf {
    Path::new(folder_path).join(relative)
}

/// ライブラリフォルダの、読み込まないフォルダをパス順に取得する
pub fn find_exclusions(conn: &Connection, folder_id: &str) -> AppResult<Vec<ExclusionRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, folder_id, path FROM library_folder_exclusions
             WHERE folder_id = ?1 ORDER BY path",
        )
        .map_err(|e| AppError::Database(format!("クエリの準備に失敗しました: {}", e)))?;

    stmt.query_map([folder_id], |row| {
        Ok(ExclusionRecord {
            id: row.get(0)?,
            folder_id: row.get(1)?,
            path: row.get(2)?,
        })
    })
    .map_err(|e| AppError::Database(format!("クエリの実行に失敗しました: {}", e)))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| AppError::Database(format!("結果の取得に失敗しました: {}", e)))
}

/// IDで、読み込まないフォルダを取得する
pub fn find_exclusion(conn: &Connection, exclusion_id: &str) -> AppResult<ExclusionRecord> {
    conn.query_row(
        "SELECT id, folder_id, path FROM library_folder_exclusions WHERE id = ?1",
        [exclusion_id],
        |row| {
            Ok(ExclusionRecord {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                path: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::Database(format!("読み込まないフォルダの取得に失敗しました: {}", e)))?
    .ok_or_else(|| AppError::NotFound("読み込まないフォルダが見つかりません".to_string()))
}

/// ライブラリフォルダの走査で読み飛ばす場所（読み込まないフォルダを、絶対パスにしたもの）
pub fn scan_exclusions(conn: &Connection, folder: &FolderRecord) -> AppResult<ScanExclusions> {
    let excluded = find_exclusions(conn, &folder.id)?
        .iter()
        .map(|exclusion| excluded_path(&folder.path, &exclusion.path))
        .collect();
    Ok(ScanExclusions::new(excluded))
}

/// 読み込まないフォルダを記録する
fn insert_exclusion(
    conn: &Connection,
    folder_id: &str,
    relative: &str,
) -> AppResult<ExclusionRecord> {
    let record = ExclusionRecord {
        id: Uuid::new_v4().to_string(),
        folder_id: folder_id.to_string(),
        path: relative.to_string(),
    };
    conn.execute(
        "INSERT INTO library_folder_exclusions (id, folder_id, path) VALUES (?1, ?2, ?3)",
        rusqlite::params![record.id, record.folder_id, record.path],
    )
    .map_err(|e| AppError::Database(format!("読み込まないフォルダの記録に失敗しました: {}", e)))?;
    Ok(record)
}

/// ライブラリフォルダに、読み込まないフォルダを足す
///
/// - すでに除外しているフォルダ（かその中）なら、`VALIDATION`
/// - 除外しているフォルダを含むフォルダなら、中の除外をまとめる（記録を消す）
///
/// 中の曲には触れない（ライブラリから外すかは、利用者が選ぶ）。
pub fn add_exclusion(
    conn: &Connection,
    folder_id: &str,
    relative: &str,
) -> AppResult<ExclusionRecord> {
    let existing = find_exclusions(conn, folder_id)?;
    if existing
        .iter()
        .any(|exclusion| is_same_or_within(relative, &exclusion.path))
    {
        return Err(AppError::Validation(
            "すでに読み込まないフォルダです（または、その中のフォルダです）".to_string(),
        ));
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;
    for nested in existing
        .iter()
        .filter(|exclusion| is_same_or_within(&exclusion.path, relative))
    {
        delete_exclusion(&tx, &nested.id)?;
    }
    let added = insert_exclusion(&tx, folder_id, relative)?;
    tx.commit().map_err(|e| {
        AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
    })?;
    Ok(added)
}

/// 読み込まないフォルダの記録を削除する（中の曲には触れない）
pub fn delete_exclusion(conn: &Connection, exclusion_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM library_folder_exclusions WHERE id = ?1",
        [exclusion_id],
    )
    .map_err(|e| AppError::Database(format!("読み込まないフォルダの削除に失敗しました: {}", e)))?;
    Ok(())
}

/// 選んだフォルダを、ライブラリフォルダからの相対パスにする（読み込まないフォルダを足す時）
///
/// ライブラリフォルダの中のフォルダだけを受け付ける（ライブラリフォルダそのもの・外のフォルダは
/// `VALIDATION`）。ダイアログが、リンクをたどった後のパスを返した場合に備えて、そのままで
/// 中にない時は、両方の実体のパスで比べる。
pub fn relative_to_folder(folder_path: &str, selected: &Path) -> AppResult<String> {
    let folder = Path::new(folder_path);
    let relative = match selected.strip_prefix(folder) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            let real_folder = fs::canonicalize(folder).ok();
            let real_selected = fs::canonicalize(selected).ok();
            match (real_folder, real_selected) {
                (Some(real_folder), Some(real_selected)) => real_selected
                    .strip_prefix(&real_folder)
                    .map(Path::to_path_buf)
                    .map_err(|_| outside_folder_error())?,
                _ => return Err(outside_folder_error()),
            }
        }
    };
    if relative.as_os_str().is_empty() {
        return Err(AppError::Validation(
            "ライブラリフォルダそのものは、読み込まないフォルダにできません".to_string(),
        ));
    }
    relative
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Validation("フォルダの名前を扱えません".to_string()))
}

fn outside_folder_error() -> AppError {
    AppError::Validation("ライブラリフォルダの中のフォルダを選んでください".to_string())
}

/// ほかのアプリが管理するフォルダの名前（新しく登録するフォルダの直下にあれば、読み込まないフォルダに
/// 入れておく。大文字と小文字は区別しない）
///
/// 固定では読み飛ばさない（ミュージックのフォルダに自分の曲を置いている人もいるため、利用者が外せる）。
/// `Music`は、ほかの名前より一般的なため、ミュージック（Music.app）の印がある時だけ入れる
/// （`is_music_app_folder`）。
const APP_FOLDER_NAMES: &[&str] = &[
    "Music",
    "iTunes",
    "MusicBee",
    "GarageBand",
    "Logic",
    "Audio Music Apps",
];

/// ミュージック（Music.app）のフォルダか（ライブラリのパッケージか、メディアのフォルダがある）
fn is_music_app_folder(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name();
        let name = Path::new(&name);
        name == Path::new("Media.localized")
            || name
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("musiclibrary"))
    })
}

/// 新しく登録するフォルダの、既定の読み込まないフォルダ（直下にある、既知のアプリのフォルダ。名前順）
///
/// ファイルシステムを読むため、DBのロックの外で呼ぶ。フォルダを読めなければ空。
pub fn default_exclusions(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let known = APP_FOLDER_NAMES
                .iter()
                .find(|known| known.eq_ignore_ascii_case(&name))?;
            let path = entry.path();
            if !path.is_dir() || (*known == "Music" && !is_music_app_folder(&path)) {
                return None;
            }
            Some(name)
        })
        .collect();
    names.sort();
    names
}

/// フォルダのインポートで使う、読み込まないフォルダ
#[derive(Debug, Default, PartialEq)]
pub struct ImportExclusions {
    /// 走査で読み飛ばす場所
    pub scan: ScanExclusions,
    /// 新しく登録する時に記録する、読み込まないフォルダ（フォルダからの相対パス）。
    /// 登録済みのフォルダ（かその中）をインポートする場合は空（記録は変えない）
    pub to_register: Vec<String>,
}

/// フォルダのインポートで使う、読み込まないフォルダを求める
///
/// - 登録済みのフォルダ（かその中）なら、そのフォルダの除外を効かせる（除外したフォルダの中を
///   選んだ場合は、選んだものとして読む）
/// - 新しく登録するフォルダなら、`defaults`（`default_exclusions`）に、まとめる中のフォルダの
///   除外（外のフォルダからの相対パスに直す）を足す。登録済みのフォルダを含む既定の除外は
///   入れない（利用者が登録したフォルダを、読み込まない場所にしないため）
pub fn import_exclusions(
    conn: &Connection,
    folder_path: &str,
    defaults: &[String],
) -> AppResult<ImportExclusions> {
    let path = normalize_folder_path(folder_path);
    let folders = find_all_folders(conn)?;

    if let Some(registered) = folders.iter().find(|f| is_same_or_within(&path, &f.path)) {
        return Ok(ImportExclusions {
            scan: scan_exclusions(conn, registered)?,
            to_register: Vec::new(),
        });
    }

    let nested: Vec<&FolderRecord> = folders
        .iter()
        .filter(|f| is_same_or_within(&f.path, &path))
        .collect();
    let mut relative: Vec<String> = defaults
        .iter()
        .filter(|name| {
            let excluded = excluded_path(&path, name);
            !nested
                .iter()
                .any(|folder| Path::new(&folder.path).starts_with(&excluded))
        })
        .cloned()
        .collect();
    for folder in nested {
        for exclusion in find_exclusions(conn, &folder.id)? {
            let excluded = excluded_path(&folder.path, &exclusion.path);
            if let Some(rebased) = excluded
                .strip_prefix(&path)
                .ok()
                .and_then(Path::to_str)
                .filter(|rebased| !rebased.is_empty())
            {
                relative.push(rebased.to_string());
            }
        }
    }
    relative.sort();
    relative.dedup();

    Ok(ImportExclusions {
        scan: ScanExclusions::new(
            relative
                .iter()
                .map(|exclusion| excluded_path(&path, exclusion))
                .collect(),
        ),
        to_register: relative,
    })
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
    /// ファイルが見つからなくなったトラックのID（見つからない曲にする）
    pub to_mark_missing: Vec<String>,
    /// 見つからない曲にしていたが、ファイルが同じ場所に戻ったトラックのID
    pub to_restore: Vec<String>,
    /// 音楽ファイルが1件も見つからなかったため、見つからない曲にしなかった
    pub missing_skipped: bool,
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
///
/// ファイルが見つからないトラックは、見つからない曲にする（外さない）。ライブラリにないファイル
/// （`to_add`）が、見つからない曲の移動・改名の先かどうかは、ファイルを読んだ後に判定する
/// （`track_relink`）。
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
        if track.is_missing {
            plan.to_restore.push(track.id.clone());
        }
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

    // すでに見つからない曲にしているトラックは、そのままにする
    let missing: Vec<String> = tracks
        .iter()
        .filter(|t| !t.is_missing && !disk_paths.contains(t.file_path.as_str()))
        .map(|t| t.id.clone())
        .collect();
    if disk_files.is_empty() && !missing.is_empty() {
        plan.missing_skipped = true;
    } else {
        plan.to_mark_missing = missing;
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    fn setup_test_db() -> Connection {
        crate::db::open_in_memory()
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
            is_missing: false,
        }
    }

    /// 前の再スキャンで見つからない曲にしたトラック
    fn missing_track(id: &str, path: &str, size: i64, modified_at: Option<i64>) -> TrackFileState {
        TrackFileState {
            is_missing: true,
            ..track(id, path, size, modified_at)
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

        register_folder(&conn, "/music/jazz/", &[]).unwrap();
        register_folder(&conn, "/music/rock", &[]).unwrap();
        register_folder(&conn, "/other", &[]).unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/rock", "/other"]
        );

        // 登録済みのフォルダの中は登録しない
        register_folder(&conn, "/music/jazz/live", &[]).unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/rock", "/other"]
        );

        // 名前が前方一致するだけの別のフォルダは中とみなさない
        register_folder(&conn, "/music/jazz2", &[]).unwrap();
        assert_eq!(
            folder_paths(&conn),
            ["/music/jazz", "/music/jazz2", "/music/rock", "/other"]
        );

        // 登録済みのフォルダを含むフォルダは、中のフォルダの記録をまとめる
        register_folder(&conn, "/music", &[]).unwrap();
        assert_eq!(folder_paths(&conn), ["/music", "/other"]);
    }

    #[test]
    fn test_register_same_folder_updates_scanned_at_only() {
        let conn = setup_test_db();
        register_folder(&conn, "/music", &[]).unwrap();
        let first = find_all_folders(&conn).unwrap().remove(0);

        conn.execute("UPDATE library_folders SET last_scanned_at = NULL", [])
            .unwrap();
        register_folder(&conn, "/music/", &[]).unwrap();

        let folders = find_all_folders(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, first.id);
        assert!(folders[0].last_scanned_at.is_some());
    }

    fn exclusion_paths(conn: &Connection, folder_path: &str) -> Vec<String> {
        let folder = find_all_folders(conn)
            .unwrap()
            .into_iter()
            .find(|f| f.path == folder_path)
            .unwrap();
        find_exclusions(conn, &folder.id)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect()
    }

    fn folder_id(conn: &Connection, folder_path: &str) -> String {
        find_all_folders(conn)
            .unwrap()
            .into_iter()
            .find(|f| f.path == folder_path)
            .unwrap()
            .id
    }

    /// 新しく登録する時だけ除外を記録し、フォルダの記録を消すと除外も消える
    #[test]
    fn test_register_folder_records_exclusions_only_when_new() {
        let conn = setup_test_db();
        register_folder(&conn, "/music", &["Music".to_string()]).unwrap();
        // 登録済みのフォルダをインポートし直しても、除外は変えない
        register_folder(&conn, "/music", &["Other".to_string()]).unwrap();
        register_folder(&conn, "/music/sub", &["Other".to_string()]).unwrap();
        assert_eq!(exclusion_paths(&conn, "/music"), ["Music"]);

        let id = folder_id(&conn, "/music");
        delete_folder(&conn, &id).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM library_folder_exclusions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_add_exclusion_merges_nested_and_rejects_inside() {
        let conn = setup_test_db();
        register_folder(&conn, "/music", &[]).unwrap();
        let id = folder_id(&conn, "/music");

        let added = add_exclusion(&conn, &id, "MusicBee/Podcasts").unwrap();
        assert_eq!(added.path, "MusicBee/Podcasts");
        assert_eq!(added.folder_id, id);
        add_exclusion(&conn, &id, "Music").unwrap();
        assert_eq!(
            exclusion_paths(&conn, "/music"),
            ["Music", "MusicBee/Podcasts"]
        );

        // すでに除外しているフォルダ（かその中）は足せない
        for inside in ["Music", "MusicBee/Podcasts/Show"] {
            assert!(matches!(
                add_exclusion(&conn, &id, inside),
                Err(AppError::Validation(_))
            ));
        }
        // 名前の先頭が同じだけのフォルダは、別のフォルダ
        add_exclusion(&conn, &id, "Music2").unwrap();
        // 除外しているフォルダを含むフォルダは、中の除外をまとめる
        add_exclusion(&conn, &id, "MusicBee").unwrap();
        assert_eq!(
            exclusion_paths(&conn, "/music"),
            ["Music", "Music2", "MusicBee"]
        );

        let music = find_exclusions(&conn, &id).unwrap().remove(0);
        assert_eq!(find_exclusion(&conn, &music.id).unwrap(), music);
        delete_exclusion(&conn, &music.id).unwrap();
        assert!(matches!(
            find_exclusion(&conn, &music.id),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(exclusion_paths(&conn, "/music"), ["Music2", "MusicBee"]);
    }

    #[test]
    fn test_relative_to_folder() {
        let dir = TempDir::new();
        let root = dir.path().join("music");
        fs::create_dir_all(root.join("MusicBee/Podcasts")).unwrap();
        let root_str = root.to_str().unwrap();

        assert_eq!(
            relative_to_folder(root_str, &root.join("MusicBee/Podcasts")).unwrap(),
            Path::new("MusicBee").join("Podcasts").to_str().unwrap()
        );
        // ライブラリフォルダそのもの・外のフォルダは選べない
        assert!(matches!(
            relative_to_folder(root_str, &root),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            relative_to_folder(root_str, dir.path()),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            relative_to_folder(root_str, &dir.path().join("music2")),
            Err(AppError::Validation(_))
        ));
    }

    /// ダイアログが、リンクをたどった後のパスを返した場合も、ライブラリフォルダの中として扱う
    #[cfg(unix)]
    #[test]
    fn test_relative_to_folder_through_a_link() {
        let dir = TempDir::new();
        let real = dir.path().join("real");
        fs::create_dir_all(real.join("Podcasts")).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let selected = fs::canonicalize(real.join("Podcasts")).unwrap();
        assert_eq!(
            relative_to_folder(link.to_str().unwrap(), &selected).unwrap(),
            "Podcasts"
        );
    }

    #[test]
    fn test_default_exclusions_finds_known_app_folders() {
        let dir = TempDir::new();
        let root = dir.path();
        for folder in [
            "Music/Media.localized/Music",
            "musicbee/Podcasts",
            "GarageBand",
            "Logic",
            "Audio Music Apps",
            "iTunes/iTunes Media",
            "Artist/Album",
            // 既知のアプリのフォルダの中は、直下ではないため入れない
            "Artist/MusicBee",
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        // 同じ名前のファイルは入れない
        fs::write(root.join("Logic.txt"), b"").unwrap();

        assert_eq!(
            default_exclusions(root),
            [
                "Audio Music Apps",
                "GarageBand",
                "Logic",
                "Music",
                "iTunes",
                "musicbee"
            ]
        );
        assert!(default_exclusions(&root.join("none")).is_empty());
    }

    /// `Music`は、ミュージック（Music.app）の印がある時だけ入れる（自分の曲を入れた`Music`は読む）
    #[test]
    fn test_default_exclusions_checks_music_app_folder() {
        let dir = TempDir::new();
        let own = dir.path().join("drive");
        fs::create_dir_all(own.join("Music/Artist/Album")).unwrap();
        assert!(default_exclusions(&own).is_empty());

        let library = dir.path().join("home");
        fs::create_dir_all(library.join("Music/Music Library.musiclibrary")).unwrap();
        assert_eq!(default_exclusions(&library), ["Music"]);
    }

    #[test]
    fn test_import_exclusions_for_registered_folder() {
        let conn = setup_test_db();
        register_folder(&conn, "/music", &["Music".to_string()]).unwrap();

        // 登録済みのフォルダ（かその中）は、そのフォルダの除外を効かせ、記録は変えない
        for path in ["/music", "/music/", "/music/Artist"] {
            let exclusions = import_exclusions(&conn, path, &["Other".to_string()]).unwrap();
            assert_eq!(
                exclusions,
                ImportExclusions {
                    scan: ScanExclusions::new(vec![PathBuf::from("/music/Music")]),
                    to_register: Vec::new(),
                }
            );
        }
    }

    /// 新しく登録するフォルダは、既定の除外に、まとめる中のフォルダの除外を足す
    #[test]
    fn test_import_exclusions_for_new_folder_rebases_nested_exclusions() {
        let conn = setup_test_db();
        register_folder(&conn, "/home/Music/MusicBee", &["Podcasts".to_string()]).unwrap();
        // 利用者が登録したフォルダを含む既定の除外は、入れない
        register_folder(&conn, "/home/Music/Music/Media.localized/Music", &[]).unwrap();

        let defaults = ["Music".to_string(), "iTunes".to_string()];
        let exclusions = import_exclusions(&conn, "/home/Music", &defaults).unwrap();

        let podcasts = Path::new("MusicBee")
            .join("Podcasts")
            .to_str()
            .unwrap()
            .to_string();
        let mut expected = vec!["iTunes".to_string(), podcasts];
        expected.sort();
        assert_eq!(exclusions.to_register, expected);
        assert_eq!(
            exclusions.scan,
            ScanExclusions::new(
                expected
                    .iter()
                    .map(|path| Path::new("/home/Music").join(path))
                    .collect()
            )
        );

        // 登録すると、中のフォルダの記録をまとめ、除外を引き継ぐ
        register_folder(&conn, "/home/Music", &exclusions.to_register).unwrap();
        assert_eq!(folder_paths(&conn), ["/home/Music"]);
        assert_eq!(exclusion_paths(&conn, "/home/Music"), expected);
    }

    #[test]
    fn test_find_and_delete_folder() {
        let conn = setup_test_db();
        register_folder(&conn, "/music", &[]).unwrap();
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
        assert_eq!(plan.to_mark_missing, ["gone"]);
        assert!(plan.to_restore.is_empty());
        assert!(!plan.missing_skipped);
    }

    #[test]
    fn test_plan_rescan_restores_and_keeps_missing_tracks() {
        let plan = plan_rescan(
            vec![
                disk("/m/back.mp3", 100, 10),
                disk("/m/back-changed.mp3", 201, 20),
                disk("/m/other.mp3", 300, 30),
            ],
            vec![
                // ファイルが同じ場所に戻った
                missing_track("back", "/m/back.mp3", 100, Some(10)),
                // 戻ったファイルの内容が変わっている場合は、読み直す
                missing_track("back-changed", "/m/back-changed.mp3", 200, Some(20)),
                // 見つからないままのトラックは、もう一度見つからない曲にはしない
                missing_track("still-gone", "/m/still-gone.mp3", 400, Some(40)),
                track("other", "/m/other.mp3", 300, Some(30)),
            ],
        );

        assert_eq!(plan.to_restore, ["back", "back-changed"]);
        assert_eq!(plan.to_update.len(), 1);
        assert_eq!(plan.to_update[0].path, "/m/back-changed.mp3");
        assert!(plan.to_mark_missing.is_empty());
        assert!(plan.to_add.is_empty());
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
        // 外付けドライブの空のマウントポイントなどで、全曲を見つからない曲にしない
        let plan = plan_rescan(vec![], vec![track("a", "/m/a.mp3", 100, Some(1))]);
        assert!(plan.to_mark_missing.is_empty());
        assert!(plan.missing_skipped);

        // ライブラリにも曲がなければ、何もしないだけ
        let plan = plan_rescan(vec![], vec![]);
        assert!(!plan.missing_skipped);

        // すでに見つからない曲だけの場合も、何もしない
        let plan = plan_rescan(vec![], vec![missing_track("a", "/m/a.mp3", 100, Some(1))]);
        assert_eq!(plan, RescanPlan::default());
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
