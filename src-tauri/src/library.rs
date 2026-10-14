use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// サポートされている音楽ファイル形式（拡張子）
///
/// 一般的な音楽プレーヤーが扱う形式のうち、再生（再生エンジン）とタグの読み書き（lofty）の両方が
/// できる形式に限る。追加する時は、実ファイルで両方を確かめる（調査の結果は
/// `docs/design/decisions.md`のADR-024・ADR-025）。
/// m4aは、AACとALACのどちらも扱える。aacはADTS形式のAAC。oggは、中がVorbisでもOpusでも扱える。
/// wv（WavPack）とape（Monkey's Audio）は、`symphonia`ではなく専用のデコーダーで再生する
/// （ADR-047。WavPackは可逆のものだけ）。
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "aac", "mp2", "aiff", "aif", "ogg", "opus", "wv", "ape",
];

/// サポートされている音楽ファイルの拡張子（ファイルを選ぶダイアログの絞り込みに使う）
pub fn supported_extensions() -> &'static [&'static str] {
    SUPPORTED_EXTENSIONS
}

/// 音楽ファイルをインポートした結果
#[derive(Debug, Default, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_count: u32,
    pub skipped_count: u32,
    /// 見つからない曲を、移動・改名された先のファイルに結び付けた数（新しい曲としては登録しない）
    pub relinked_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
}

impl ImportResult {
    /// 別の結果を足し合わせる（ファイルとフォルダを、まとめてインポートした場合）
    pub fn merge(&mut self, other: ImportResult) {
        self.imported_count += other.imported_count;
        self.skipped_count += other.skipped_count;
        self.relinked_count += other.relinked_count;
        self.error_count += other.error_count;
        self.errors.extend(other.errors);
    }
}

/// 重複ファイルの処理方法
#[derive(Debug, serde::Deserialize, Clone, Copy, specta::Type)]
pub enum DuplicateAction {
    Skip,    // 既存ファイルをスキップ
    Replace, // 既存ファイルを置き換え
}

/// トラック削除の結果
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// 削除に成功したトラック数
    pub success_count: u32,
    /// 削除に失敗したトラック数
    pub failed_count: u32,
    /// 削除に失敗したトラックの詳細
    pub failed_tracks: Vec<DeleteFailure>,
}

/// 削除失敗の詳細
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailure {
    /// トラックID
    pub track_id: String,
    /// ファイルパス
    pub file_path: String,
    /// 失敗の理由
    pub reason: String,
}

/// フォルダを走査した結果
#[derive(Debug, Default, PartialEq)]
pub struct ScannedFolder {
    /// 見つかった音楽ファイル
    pub audio_files: Vec<PathBuf>,
    /// 読めなかったフォルダ（権限がないなど）。中のファイルは`audio_files`に入っていない
    pub unreadable_dirs: Vec<PathBuf>,
}

impl ScannedFolder {
    /// パスが、読めなかったフォルダの中か
    ///
    /// 再スキャンで、読めなかったフォルダの中の曲を「見つからない曲」にしないために使う。
    pub fn is_in_unreadable_dir(&self, path: &Path) -> bool {
        self.unreadable_dirs.iter().any(|dir| path.starts_with(dir))
    }

    /// 読めなかったフォルダを知らせる文（結果のエラーの一覧に足す）
    ///
    /// 名前が`.`で始まるフォルダは出さない（外付けドライブの`.Trashes`・`.Spotlight-V100`のように、
    /// OSが管理していて読めないのが普通のフォルダのため）。
    pub fn unreadable_dir_errors(&self) -> Vec<String> {
        self.unreadable_dirs
            .iter()
            .filter(|dir| {
                !dir.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            })
            .map(|dir| format!("{}: フォルダを読み取れませんでした", dir.display()))
            .collect()
    }
}

/// ディレクトリを再帰的にスキャンして音楽ファイルを検索
///
/// 中のフォルダを読めなくても、ほかのフォルダの走査は続ける（読めなかったフォルダは
/// `unreadable_dirs`に入れる）。指定したフォルダそのものを読めない場合は、エラーにする。
///
/// シンボリックリンクは、たどる（リンクで置いたフォルダの曲も取り込む）。ただし、たどっている
/// 途中のフォルダへ戻るリンク（親フォルダを指すリンクなど）は、終わらなくなるため、たどらない。
pub fn scan_directory(dir_path: &Path) -> AppResult<ScannedFolder> {
    let read_error =
        |e: std::io::Error| AppError::Io(format!("ディレクトリの読み取りに失敗しました: {}", e));
    let entries = fs::read_dir(dir_path).map_err(read_error)?;
    let real_path = fs::canonicalize(dir_path).map_err(read_error)?;

    let mut scanned = ScannedFolder::default();
    let ancestors = Ancestors {
        real_path: &real_path,
        parent: None,
    };
    scan_entries(dir_path, entries, &ancestors, &mut scanned);
    Ok(scanned)
}

/// 指定したフォルダから今のフォルダまでの、実体のパス（リンクの循環の判定に使う）
struct Ancestors<'a> {
    /// 今のフォルダの、リンクをたどった後の実体のパス
    real_path: &'a Path,
    parent: Option<&'a Ancestors<'a>>,
}

impl Ancestors<'_> {
    fn contains(&self, real_path: &Path) -> bool {
        self.real_path == real_path || self.parent.is_some_and(|parent| parent.contains(real_path))
    }
}

/// 走査で見つけた項目の種類（シンボリックリンクは、リンク先の種類）
enum EntryKind {
    /// フォルダ（`real_path`は、リンクをたどった後の実体のパス）
    Dir {
        real_path: PathBuf,
    },
    File,
}

/// 項目の種類を調べる（フォルダでもファイルでもないもの・切れたリンクは値なし）
///
/// `DirEntry::file_type`はリンクをたどらず、フォルダを読んだ時の情報で分かるため、
/// リンクでない項目は、ファイルごとの問い合わせをせずに済む。
fn entry_kind(entry: &fs::DirEntry, parent_real_path: &Path) -> Option<EntryKind> {
    let file_type = entry.file_type().ok()?;
    if file_type.is_dir() {
        return Some(EntryKind::Dir {
            real_path: parent_real_path.join(entry.file_name()),
        });
    }
    if file_type.is_file() {
        return Some(EntryKind::File);
    }
    if !file_type.is_symlink() {
        return None;
    }

    let path = entry.path();
    let target = fs::metadata(&path).ok()?;
    if target.is_dir() {
        Some(EntryKind::Dir {
            real_path: fs::canonicalize(&path).ok()?,
        })
    } else if target.is_file() {
        Some(EntryKind::File)
    } else {
        None
    }
}

/// フォルダの中身を、再帰的にスキャンする内部関数
fn scan_entries(
    dir_path: &Path,
    entries: fs::ReadDir,
    ancestors: &Ancestors,
    scanned: &mut ScannedFolder,
) {
    let mark_unreadable = |scanned: &mut ScannedFolder, dir: &Path, e: std::io::Error| {
        log::warn!("フォルダを読み取れませんでした: {}: {}", dir.display(), e);
        if !scanned.is_in_unreadable_dir(dir) {
            scanned.unreadable_dirs.push(dir.to_path_buf());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                // 途中までしか読めていないため、このフォルダは読めなかったものとして扱う
                mark_unreadable(scanned, dir_path, e);
                continue;
            }
        };
        let path = entry.path();

        match entry_kind(&entry, ancestors.real_path) {
            Some(EntryKind::Dir { real_path }) => {
                if ancestors.contains(&real_path) {
                    log::warn!(
                        "たどっている途中のフォルダへ戻るリンクのため、たどりません: {}",
                        path.display()
                    );
                    continue;
                }
                match fs::read_dir(&path) {
                    Ok(entries) => {
                        let ancestors = Ancestors {
                            real_path: &real_path,
                            parent: Some(ancestors),
                        };
                        scan_entries(&path, entries, &ancestors, scanned);
                    }
                    Err(e) => mark_unreadable(scanned, &path, e),
                }
            }
            Some(EntryKind::File) if is_supported_audio_file(&path) => {
                scanned.audio_files.push(path);
            }
            Some(EntryKind::File) | None => {}
        }
    }
}

/// ファイルがサポートされている音楽ファイルかチェック
pub fn is_supported_audio_file(path: &Path) -> bool {
    if let Some(extension) = path.extension()
        && let Some(ext_str) = extension.to_str()
    {
        return SUPPORTED_EXTENSIONS.contains(&ext_str.to_lowercase().as_str());
    }
    false
}

/// ファイル名からデフォルトのタイトルを生成
pub fn get_default_title(file_path: &Path) -> String {
    file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// ファイルサイズを取得
pub fn get_file_size(file_path: &Path) -> AppResult<i64> {
    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Io(format!("ファイルメタデータの取得に失敗しました: {}", e)))?;
    Ok(metadata.len() as i64)
}

/// ファイルの更新日時（UNIX時間の秒）を取得する。取得できない場合はNone
///
/// 再スキャンでファイルの変更を検出するために記録する。
pub fn get_file_modified_at(file_path: &Path) -> Option<i64> {
    modified_at_of(&fs::metadata(file_path).ok()?)
}

/// 取得済みのメタデータから、ファイルの更新日時（UNIX時間の秒）を取り出す
pub fn modified_at_of(metadata: &fs::Metadata) -> Option<i64> {
    let modified = metadata.modified().ok()?;
    let seconds = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(seconds).ok()
}

/// ファイル形式を取得
pub fn get_file_format(file_path: &Path) -> String {
    file_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_uppercase()
}

/// 件数（usize）をフロントエンドへ返すu32へ変換する
///
/// u32に収まらない件数は現実的に発生しないが、`as`によるサイレントな
/// 切り捨てを避けるため明示的に変換し、収まらない場合はエラーとする。
/// 件数（usize）をフロントエンドへ返すu32へ変換する
pub(crate) fn to_count(value: usize) -> AppResult<u32> {
    u32::try_from(value)
        .map_err(|_| AppError::Database(format!("件数が扱える範囲を超えました: {}", value)))
}

/// トラックをデータベースから削除（ライブラリからのみ削除）
///
/// トラックIDのバリデーションはコマンド層（入力境界）で実施済みであることを前提とする。
pub fn delete_tracks(conn: &Connection, track_ids: &[String]) -> AppResult<u32> {
    if track_ids.is_empty() {
        return Ok(0);
    }

    // トランザクションを使用して一括削除
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    let mut deleted_count: usize = 0;

    for track_id in track_ids {
        deleted_count += crate::repository::delete_track(&tx, track_id)?;
    }

    tx.commit().map_err(|e| {
        AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
    })?;

    to_count(deleted_count)
}

/// トラックをデータベースとファイルシステムから削除
/// ファイル削除に失敗した場合でも、可能な限り処理を続行する
pub fn delete_tracks_with_files(
    conn: &Connection,
    track_ids: &[String],
) -> AppResult<DeleteResult> {
    if track_ids.is_empty() {
        return Ok(DeleteResult {
            success_count: 0,
            failed_count: 0,
            failed_tracks: Vec::new(),
        });
    }

    // まずファイルパスを取得（見つからないトラックはスキップ）
    let mut track_info: Vec<(String, String)> = Vec::new();
    for track_id in track_ids {
        if let Some(path) = crate::repository::try_find_file_path_by_track_id(conn, track_id)? {
            track_info.push((track_id.clone(), path));
        }
    }

    let mut success_count: usize = 0;
    let mut failed_tracks: Vec<DeleteFailure> = Vec::new();

    // トランザクションを使用してデータベースから削除
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Database(format!("トランザクションの開始に失敗しました: {}", e)))?;

    for (track_id, file_path) in &track_info {
        // ファイルの削除を試みる
        let file_delete_result = fs::remove_file(file_path);

        match file_delete_result {
            Ok(()) => {
                // ファイル削除成功、データベースからも削除
                crate::repository::delete_track(&tx, track_id)?;
                success_count += 1;
            }
            Err(e) => {
                // ファイル削除失敗
                // ファイルが見つからない場合はDBからも削除を許可
                if e.kind() == std::io::ErrorKind::NotFound {
                    crate::repository::delete_track(&tx, track_id)?;
                    success_count += 1;
                } else {
                    failed_tracks.push(DeleteFailure {
                        track_id: track_id.clone(),
                        file_path: file_path.clone(),
                        reason: format!("ファイルの削除に失敗しました: {}", e),
                    });
                }
            }
        }
    }

    tx.commit().map_err(|e| {
        AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
    })?;

    Ok(DeleteResult {
        success_count: to_count(success_count)?,
        failed_count: to_count(failed_tracks.len())?,
        failed_tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    /// 空のファイルを作る（途中のフォルダも作る）
    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    /// 走査で見つかったファイルを、フォルダからの相対パスで返す（名前の順）
    fn scanned_paths(root: &Path) -> Vec<String> {
        let mut paths: Vec<String> = scan_directory(root)
            .unwrap()
            .audio_files
            .iter()
            .map(|path| {
                let relative = path.strip_prefix(root).unwrap();
                relative.to_str().unwrap().to_string()
            })
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn test_scan_directory_finds_audio_files_in_subfolders() {
        let dir = TempDir::new();
        touch(&dir.path().join("a.mp3"));
        touch(&dir.path().join("Artist/Album/b.FLAC"));
        touch(&dir.path().join("Artist/cover.jpg"));

        assert_eq!(scanned_paths(dir.path()), ["Artist/Album/b.FLAC", "a.mp3"]);
    }

    #[test]
    fn test_scan_directory_fails_when_the_folder_cannot_be_read() {
        let dir = TempDir::new();
        assert!(scan_directory(&dir.path().join("none")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_directory_follows_links_but_not_back_to_an_ancestor() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new();
        let root = dir.path().join("music");
        let outside = dir.path().join("outside");
        touch(&root.join("a.mp3"));
        touch(&root.join("sub/b.mp3"));
        touch(&outside.join("c.mp3"));
        // リンクで置いたフォルダ・ファイルは取り込む
        symlink(&outside, root.join("linked")).unwrap();
        symlink(outside.join("c.mp3"), root.join("d.mp3")).unwrap();
        // 親フォルダ・自分自身へ戻るリンクと、リンク先から戻ってくるリンクは、たどらない
        symlink(&root, root.join("sub/parent")).unwrap();
        symlink(root.join("sub"), root.join("sub/self")).unwrap();
        symlink(&root, outside.join("back")).unwrap();
        // 切れたリンクは飛ばす
        symlink(dir.path().join("none"), root.join("broken.mp3")).unwrap();

        assert_eq!(
            scanned_paths(&root),
            ["a.mp3", "d.mp3", "linked/c.mp3", "sub/b.mp3"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_directory_continues_past_unreadable_folders() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new();
        touch(&dir.path().join("a.mp3"));
        touch(&dir.path().join("locked/b.mp3"));
        touch(&dir.path().join(".Trashes/c.mp3"));
        let set_mode = |name: &str, mode: u32| {
            fs::set_permissions(dir.path().join(name), fs::Permissions::from_mode(mode)).unwrap();
        };
        set_mode("locked", 0o000);
        set_mode(".Trashes", 0o000);
        // 管理者として動かしている場合は、権限がなくても読めるため、確かめられない
        let is_locked = fs::read_dir(dir.path().join("locked")).is_err();

        let scanned = scan_directory(dir.path());

        // 一時フォルダを削除できるように、権限を戻す
        set_mode("locked", 0o755);
        set_mode(".Trashes", 0o755);
        if !is_locked {
            return;
        }
        let scanned = scanned.unwrap();
        assert_eq!(scanned.audio_files, [dir.path().join("a.mp3")]);
        assert!(scanned.is_in_unreadable_dir(&dir.path().join("locked/b.mp3")));
        assert!(scanned.is_in_unreadable_dir(&dir.path().join(".Trashes/c.mp3")));
        assert!(!scanned.is_in_unreadable_dir(&dir.path().join("a.mp3")));
        assert!(!scanned.is_in_unreadable_dir(&dir.path().join("locked-2/d.mp3")));
        // 名前が`.`で始まるフォルダは、エラーとしては知らせない
        assert_eq!(
            scanned.unreadable_dir_errors(),
            [format!(
                "{}: フォルダを読み取れませんでした",
                dir.path().join("locked").display()
            )]
        );
    }

    #[test]
    fn test_is_supported_audio_file() {
        assert!(is_supported_audio_file(Path::new("test.mp3")));
        assert!(is_supported_audio_file(Path::new("test.flac")));
        assert!(is_supported_audio_file(Path::new("test.wav")));
        assert!(is_supported_audio_file(Path::new("test.m4a")));
        for extension in ["aac", "mp2", "aiff", "aif", "ogg", "opus", "wv", "ape"] {
            assert!(
                is_supported_audio_file(Path::new(&format!("test.{extension}"))),
                "{extension}"
            );
        }
        assert!(is_supported_audio_file(Path::new("test.MP3")));
        assert!(is_supported_audio_file(Path::new("test.AIFF")));
        // 再生かタグの読み書きができない形式は、取り込まない（ADR-024）
        for extension in ["wma", "dsf", "tta", "mpc", "mka", "oga"] {
            assert!(
                !is_supported_audio_file(Path::new(&format!("test.{extension}"))),
                "{extension}"
            );
        }
        assert!(!is_supported_audio_file(Path::new("test.txt")));
        assert!(!is_supported_audio_file(Path::new("test.jpg")));
    }

    #[test]
    fn test_get_default_title() {
        assert_eq!(get_default_title(Path::new("song.mp3")), "song");
        assert_eq!(
            get_default_title(Path::new("/path/to/my_song.flac")),
            "my_song"
        );
    }

    #[test]
    fn test_get_file_modified_at() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mp3");
        fs::write(&file, b"dummy").unwrap();

        let modified = get_file_modified_at(&file).expect("更新日時を取得できること");
        assert!(modified > 0);
        assert_eq!(get_file_modified_at(&dir.join("missing.mp3")), None);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_get_file_format() {
        assert_eq!(get_file_format(Path::new("test.mp3")), "MP3");
        assert_eq!(get_file_format(Path::new("test.flac")), "FLAC");
    }

    #[test]
    fn test_to_count() {
        assert_eq!(to_count(0).unwrap(), 0);
        assert_eq!(to_count(42).unwrap(), 42);
        assert_eq!(to_count(u32::MAX as usize).unwrap(), u32::MAX);
    }

    /// u32に収まらない件数は切り捨てずエラーにする（64bit環境でのみ検証可能）
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_to_count_overflow() {
        assert!(to_count(u32::MAX as usize + 1).is_err());
    }
}
