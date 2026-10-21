use crate::error::{AppError, AppResult};
use crate::state::AppState;
use rusqlite::Connection;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    /// 音楽フォルダへコピーして取り込んだ数（`imported_count`・`relinked_count`の内数）
    pub copied_count: u32,
    /// 音楽フォルダへコピーしなかった曲（コピー先に同じ名前のファイルがあるなど。取り込んでいない）
    pub not_copied: Vec<crate::music_folder_copy::NotCopied>,
}

impl ImportResult {
    /// 別の結果を足し合わせる（ファイルとフォルダを、まとめてインポートした場合）
    pub fn merge(&mut self, other: ImportResult) {
        self.imported_count += other.imported_count;
        self.skipped_count += other.skipped_count;
        self.relinked_count += other.relinked_count;
        self.error_count += other.error_count;
        self.errors.extend(other.errors);
        self.copied_count += other.copied_count;
        self.not_copied.extend(other.not_copied);
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
    /// 外付けドライブの`.Trashes`・`.Spotlight-V100`のような、OSが管理していて読めないのが普通の
    /// フォルダは、隠しフォルダとして読み飛ばすため、ここには入らない（`is_always_skipped`）。
    pub fn unreadable_dir_errors(&self) -> Vec<String> {
        self.unreadable_dirs
            .iter()
            .map(|dir| format!("{}: フォルダを読み取れませんでした", dir.display()))
            .collect()
    }
}

/// どのライブラリフォルダでも読み飛ばすフォルダの名前（ゴミ箱と、OS・NASが管理するフォルダ）
///
/// 大文字と小文字は区別しない（Windows・NASのドライブは区別しないため）。
const SKIPPED_FOLDER_NAMES: &[&str] = &[
    // Windowsのゴミ箱（`RECYCLER`は古い版）と、Windowsが管理するフォルダ
    "$RECYCLE.BIN",
    "RECYCLER",
    "System Volume Information",
    // macOSがネットワークドライブに作るゴミ箱
    "Network Trash Folder",
    // NAS（Synology・QNAP）のごみ箱・スナップショット（ライブラリの複製）・サムネイル
    "#recycle",
    "#snapshot",
    "@Recycle",
    "@eaDir",
];

/// どのライブラリフォルダでも読み飛ばすフォルダの拡張子（macOSのパッケージ。Finderが1つのファイル
/// として見せるフォルダ）
///
/// 中に音声ファイルを持つもの（GarageBand・Logicのプロジェクト、写真・動画のライブラリなど）と、
/// アプリ・プラグイン。判定は拡張子の一覧で行い、OSには問い合わせない（ADR-062）。
const PACKAGE_EXTENSIONS: &[&str] = &[
    // アプリ・プラグイン
    "app",
    "bundle",
    "framework",
    "plugin",
    "component",
    "vst",
    "vst3",
    // 音楽・写真・映像のアプリのプロジェクトとライブラリ
    "band",
    "logicx",
    "logic",
    "musiclibrary",
    "tvlibrary",
    "photoslibrary",
    "photolibrary",
    "fcpbundle",
    "imovielibrary",
    "theater",
];

/// 名前から、どのライブラリフォルダでも読み飛ばすものか（隠しフォルダ・ゴミ箱・パッケージ）
///
/// 名前が`.`で始まるものは、ファイルも読み飛ばす（macOSが外付けドライブに作る`._曲名.mp3`は、
/// 曲ではなく、元のファイルの付加情報のため）。パッケージの拡張子は、音楽ファイルの拡張子と
/// 重ならないため、ファイルにも同じ判定を使う。
pub fn is_always_skipped(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    if name.starts_with('.') {
        return true;
    }
    if SKIPPED_FOLDER_NAMES
        .iter()
        .any(|skipped| skipped.eq_ignore_ascii_case(&name))
    {
        return true;
    }
    Path::new(name.as_ref())
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            PACKAGE_EXTENSIONS
                .iter()
                .any(|package| package.eq_ignore_ascii_case(extension))
        })
}

/// フォルダの走査で読み飛ばす場所
///
/// どのライブラリフォルダでも読み飛ばすもの（`is_always_skipped`）と、ライブラリフォルダごとの
/// 除外（読み込まないフォルダ。`library_folder_exclusions`）。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScanExclusions {
    /// 除外したフォルダ（絶対パス）
    excluded: Vec<PathBuf>,
}

impl ScanExclusions {
    pub fn new(excluded: Vec<PathBuf>) -> Self {
        Self { excluded }
    }

    /// 走査で、この項目（ファイルかフォルダ）を読み飛ばすか
    fn skips_entry(&self, path: &Path, name: &OsStr) -> bool {
        is_always_skipped(name) || self.excluded.iter().any(|dir| dir == path)
    }

    /// `root`を走査した時に、`path`（ファイルかフォルダ）が読み飛ばす場所にあるか（`root`の外ならfalse）
    ///
    /// 再スキャンで、読み飛ばす場所にある登録済みの曲を比べる対象から外す時と、フォルダの監視で、
    /// 読み飛ばす場所の中の変更を無視する時に使う。`root`より上のフォルダの名前は見ない
    /// （利用者が選んだフォルダが隠しフォルダの中にあっても、中は読む）。
    pub fn skips(&self, root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        self.excluded.iter().any(|dir| path.starts_with(dir))
            || relative.components().any(
                |component| matches!(component, Component::Normal(name) if is_always_skipped(name)),
            )
    }
}

/// ディレクトリを再帰的にスキャンして音楽ファイルを検索
///
/// 中のフォルダを読めなくても、ほかのフォルダの走査は続ける（読めなかったフォルダは
/// `unreadable_dirs`に入れる）。指定したフォルダそのものを読めない場合は、エラーにする。
///
/// 隠しフォルダ・ゴミ箱・パッケージと、`exclusions`の除外したフォルダは、中をたどらない
/// （指定したフォルダそのものは、名前によらず読む）。
///
/// シンボリックリンクは、たどる（リンクで置いたフォルダの曲も取り込む）。ただし、たどっている
/// 途中のフォルダへ戻るリンク（親フォルダを指すリンクなど）は、終わらなくなるため、たどらない。
pub fn scan_directory(dir_path: &Path, exclusions: &ScanExclusions) -> AppResult<ScannedFolder> {
    let read_error =
        |e: std::io::Error| AppError::Io(format!("ディレクトリの読み取りに失敗しました: {}", e));
    let entries = fs::read_dir(dir_path).map_err(read_error)?;
    let real_path = fs::canonicalize(dir_path).map_err(read_error)?;

    let mut scanned = ScannedFolder::default();
    let ancestors = Ancestors {
        real_path: &real_path,
        parent: None,
    };
    scan_entries(dir_path, entries, &ancestors, exclusions, &mut scanned);
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
    exclusions: &ScanExclusions,
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
        // 種類を調べる前に名前で決める（読み飛ばす項目には、ファイルごとの問い合わせをしない）
        if exclusions.skips_entry(&path, &entry.file_name()) {
            continue;
        }

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
                        scan_entries(&path, entries, &ancestors, exclusions, scanned);
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

/// 曲のファイルを消す（macOSではゴミ箱へ移し、ほかのOSでは完全に削除する）
///
/// @returns ゴミ箱の中での場所（完全に削除した場合と、分からない場合はNone）
pub fn discard_track_file(path: &Path) -> AppResult<Option<PathBuf>> {
    if crate::trash::is_available() {
        crate::trash::move_to_trash(path)
    } else {
        fs::remove_file(path)
            .map(|()| None)
            .map_err(|e| AppError::Io(format!("ファイルの削除に失敗しました: {}", e)))
    }
}

/// ファイルを消せた曲
struct DiscardedTrack<'a> {
    track_id: &'a str,
    file_path: &'a str,
    /// ゴミ箱の中での場所（元に戻す時に使う）
    trashed: Option<PathBuf>,
}

/// トラックをデータベースとファイルシステムから削除
///
/// ファイルはDBのロックの外で消す（遅いドライブでも、その間ほかのコマンドとアルバムアートの表示を
/// 止めない）。ファイルを消せなかった曲は、ライブラリに残して`failed_tracks`に入れ、ほかの曲は続ける。
///
/// 1. ロックの中で、ファイルのパスを求める（見つからないトラックは飛ばす）
/// 2. ロックの外で、ファイルを消す（`discard`。普段は`discard_track_file`。ゴミ箱へ移した場合は、
///    ゴミ箱の中の場所を返す）。ファイルがすでにない曲は、消せたものとして扱う
/// 3. ファイルを消せた曲の行を、1つの短いトランザクションで削除する。削除できなかった場合は、
///    ゴミ箱へ移したファイルを元の場所へ戻してから、エラーを返す
///
/// トラックIDのバリデーションはコマンド層（入力境界）で実施済みであることを前提とする。
pub fn delete_tracks_with_files(
    state: &AppState,
    track_ids: &[String],
    discard: &dyn Fn(&Path) -> AppResult<Option<PathBuf>>,
) -> AppResult<DeleteResult> {
    let targets: Vec<(String, String)> = state.with_db(|db| {
        let mut targets = Vec::new();
        for track_id in track_ids {
            if let Some(path) = crate::repository::try_find_file_path_by_track_id(db, track_id)? {
                targets.push((track_id.clone(), path));
            }
        }
        Ok(targets)
    })?;

    let mut discarded: Vec<DiscardedTrack> = Vec::new();
    let mut failed_tracks: Vec<DeleteFailure> = Vec::new();
    for (track_id, file_path) in &targets {
        let path = Path::new(file_path);
        // リンクはたどらずに確かめる（リンクの先がなくても、リンクそのものは消す）
        let result = match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            _ => discard(path),
        };
        match result {
            Ok(trashed) => discarded.push(DiscardedTrack {
                track_id,
                file_path,
                trashed,
            }),
            Err(e) => failed_tracks.push(DeleteFailure {
                track_id: track_id.clone(),
                file_path: file_path.clone(),
                reason: e.to_string(),
            }),
        }
    }

    let ids: Vec<String> = discarded
        .iter()
        .map(|track| track.track_id.to_string())
        .collect();
    if let Err(e) = state.with_db(|db| delete_tracks(db, &ids)) {
        restore_discarded_files(&discarded);
        return Err(e);
    }

    Ok(DeleteResult {
        success_count: to_count(discarded.len())?,
        failed_count: to_count(failed_tracks.len())?,
        failed_tracks,
    })
}

/// ゴミ箱へ移したファイルを、元の場所へ戻す（戻せなかったファイルは、ログに残す）
fn restore_discarded_files(discarded: &[DiscardedTrack]) {
    for track in discarded {
        let Some(trashed) = &track.trashed else {
            continue;
        };
        if let Err(e) = crate::file_organize::move_no_replace(trashed, Path::new(track.file_path)) {
            log::error!(
                "ファイルを、ゴミ箱から戻せませんでした: {}: {}",
                track.file_path,
                e
            );
        }
    }
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
        scanned_paths_excluding(root, &ScanExclusions::default())
    }

    /// 除外を指定して走査し、見つかったファイルを、フォルダからの相対パスで返す（名前の順）
    fn scanned_paths_excluding(root: &Path, exclusions: &ScanExclusions) -> Vec<String> {
        let mut paths: Vec<String> = scan_directory(root, exclusions)
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
        assert!(scan_directory(&dir.path().join("none"), &ScanExclusions::default()).is_err());
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

        let scanned = scan_directory(dir.path(), &ScanExclusions::default());

        // 一時フォルダを削除できるように、権限を戻す
        set_mode("locked", 0o755);
        set_mode(".Trashes", 0o755);
        if !is_locked {
            return;
        }
        let scanned = scanned.unwrap();
        assert_eq!(scanned.audio_files, [dir.path().join("a.mp3")]);
        assert!(scanned.is_in_unreadable_dir(&dir.path().join("locked/b.mp3")));
        assert!(!scanned.is_in_unreadable_dir(&dir.path().join("a.mp3")));
        assert!(!scanned.is_in_unreadable_dir(&dir.path().join("locked-2/d.mp3")));
        // 隠しフォルダは読みに行かないため、読めなくても知らせない
        assert!(!scanned.is_in_unreadable_dir(&dir.path().join(".Trashes/c.mp3")));
        assert_eq!(
            scanned.unreadable_dir_errors(),
            [format!(
                "{}: フォルダを読み取れませんでした",
                dir.path().join("locked").display()
            )]
        );
    }

    /// 隠しフォルダ・ゴミ箱・パッケージ・名前が`.`で始まるファイルは、どこでも読み飛ばす
    #[test]
    fn test_scan_directory_skips_hidden_trash_and_package_folders() {
        let dir = TempDir::new();
        // 選んだフォルダそのものは、名前によらず読む
        let root = dir.path().join(".music");
        for path in [
            "Artist/01.mp3",
            "Artist/._01.mp3",
            ".Trashes/501/a.mp3",
            "Artist/.hidden/b.mp3",
            "$RECYCLE.BIN/S-1-5-21/c.mp3",
            "$Recycle.Bin/d.mp3",
            "System Volume Information/e.mp3",
            "#recycle/f.mp3",
            "@eaDir/g.mp3",
            "GarageBand/Song.band/Media/h.aif",
            "Logic/Project.logicx/Media/Audio Files/i.wav",
            "Music/Music Library.musiclibrary/j.mp3",
            "Plug-ins/Synth.component/Contents/k.wav",
            // 名前に含むだけのフォルダ・拡張子に似たフォルダは読む
            "My $RECYCLE.BIN/l.mp3",
            "Rock.band - Live/m.mp3",
            "Band/n.mp3",
        ] {
            touch(&root.join(path));
        }

        assert_eq!(
            scanned_paths(&root),
            [
                "Artist/01.mp3",
                "Band/n.mp3",
                "My $RECYCLE.BIN/l.mp3",
                "Rock.band - Live/m.mp3"
            ]
        );
    }

    /// 除外したフォルダは、中をたどらない（名前が同じ、ほかの場所のフォルダは読む）
    #[test]
    fn test_scan_directory_skips_excluded_folders() {
        let dir = TempDir::new();
        let root = dir.path();
        for path in [
            "Artist/01.mp3",
            "Music/Media.localized/Music/a.m4a",
            "MusicBee/Podcasts/Show/b.mp3",
            "Artist/Music/c.mp3",
        ] {
            touch(&root.join(path));
        }
        let exclusions =
            ScanExclusions::new(vec![root.join("Music"), root.join("MusicBee/Podcasts")]);

        assert_eq!(
            scanned_paths_excluding(root, &exclusions),
            ["Artist/01.mp3", "Artist/Music/c.mp3"]
        );
    }

    #[test]
    fn test_scan_exclusions_skips() {
        let root = Path::new("/music");
        let exclusions = ScanExclusions::new(vec![root.join("Music"), root.join("A/Podcasts")]);
        let skips = |path: &str| exclusions.skips(root, Path::new(path));

        assert!(skips("/music/Music/Media/a.m4a"));
        assert!(skips("/music/Music"));
        assert!(skips("/music/A/Podcasts/Show/b.mp3"));
        assert!(skips("/music/A/.hidden/c.mp3"));
        assert!(skips("/music/A/._c.mp3"));
        assert!(skips("/music/GarageBand/Song.band/Media/d.aif"));
        assert!(skips("/music/$RECYCLE.BIN/e.mp3"));
        assert!(!skips("/music/A/01.mp3"));
        assert!(!skips("/music/A/Podcasts2/f.mp3"));
        assert!(!skips("/music/MusicBee/g.mp3"));
        // フォルダより上の名前は見ない。フォルダの外はfalse
        assert!(!ScanExclusions::default().skips(
            Path::new("/Volumes/.hidden/music"),
            Path::new("/Volumes/.hidden/music/a.mp3")
        ));
        assert!(!skips("/other/.hidden/a.mp3"));
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

    /// ファイルごとの削除のテスト用の状態（ゴミ箱の代わりに、`trash`フォルダへ移す）
    struct DeleteFixture {
        state: AppState,
        dir: TempDir,
    }

    impl DeleteFixture {
        fn new() -> Self {
            Self {
                state: AppState::new(crate::db::open_in_memory()),
                dir: TempDir::new(),
            }
        }

        /// 曲をライブラリに登録する（`exists`がtrueなら、ファイルも作る）
        fn add_track(&self, id: &str, exists: bool) -> PathBuf {
            let path = self.dir.path().join(format!("{id}.mp3"));
            if exists {
                touch(&path);
            }
            self.state
                .with_db(|db| {
                    db.execute(
                        "INSERT INTO tracks (id, file_path, file_name) VALUES (?1, ?2, ?3)",
                        rusqlite::params![id, path.to_string_lossy(), format!("{id}.mp3")],
                    )
                    .map_err(|e| AppError::Database(e.to_string()))
                })
                .unwrap();
            path
        }

        fn trash_path(&self, path: &Path) -> PathBuf {
            self.dir
                .path()
                .join("trash")
                .join(path.file_name().unwrap())
        }

        /// ゴミ箱の代わり（名前が`locked`で始まるファイルは、移せない）
        fn trash(&self, path: &Path) -> AppResult<Option<PathBuf>> {
            let name = path.file_name().unwrap().to_string_lossy();
            if name.starts_with("locked") {
                return Err(AppError::Io(
                    "ファイルをゴミ箱へ移せませんでした".to_string(),
                ));
            }
            let trashed = self.trash_path(path);
            fs::create_dir_all(trashed.parent().unwrap()).unwrap();
            fs::rename(path, &trashed).unwrap();
            Ok(Some(trashed))
        }

        fn track_ids(&self) -> Vec<String> {
            self.state
                .with_db(|db| {
                    let mut stmt = db.prepare("SELECT id FROM tracks ORDER BY id").unwrap();
                    let ids = stmt
                        .query_map([], |row| row.get(0))
                        .unwrap()
                        .collect::<Result<Vec<String>, _>>()
                        .unwrap();
                    Ok(ids)
                })
                .unwrap()
        }
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    /// ファイルを移せた曲・ファイルがもうない曲は削除し、移せなかった曲はライブラリに残す
    #[test]
    fn test_delete_tracks_with_files_keeps_tracks_whose_files_cannot_be_removed() {
        let fixture = DeleteFixture::new();
        let a = fixture.add_track("a", true);
        fixture.add_track("b", false);
        let locked = fixture.add_track("locked", true);
        fixture.add_track("other", true);
        let trash = |path: &Path| fixture.trash(path);

        let result = delete_tracks_with_files(
            &fixture.state,
            &ids(&["a", "b", "locked", "unknown"]),
            &trash,
        )
        .unwrap();

        assert_eq!((result.success_count, result.failed_count), (2, 1));
        assert_eq!(result.failed_tracks[0].track_id, "locked");
        assert_eq!(result.failed_tracks[0].file_path, locked.to_string_lossy());
        assert!(
            result.failed_tracks[0]
                .reason
                .contains("ゴミ箱へ移せません")
        );
        assert_eq!(fixture.track_ids(), ids(&["locked", "other"]));
        assert!(!a.exists());
        assert!(fixture.trash_path(&a).is_file());
        assert!(locked.is_file());
    }

    /// ファイルを消している間は、DBのロックを持たない
    #[test]
    fn test_delete_tracks_with_files_removes_files_outside_the_db_lock() {
        let fixture = DeleteFixture::new();
        fixture.add_track("a", true);
        let trash = |path: &Path| {
            assert!(
                fixture.state.db.try_lock().is_ok(),
                "DBのロックを持ったまま、ファイルを消している"
            );
            fixture.trash(path)
        };

        let result = delete_tracks_with_files(&fixture.state, &ids(&["a"]), &trash).unwrap();
        assert_eq!(result.success_count, 1);
        assert!(fixture.track_ids().is_empty());
    }

    /// 行を削除できなかった場合は、ゴミ箱へ移したファイルを元の場所へ戻し、ライブラリも変えない
    #[test]
    fn test_delete_tracks_with_files_restores_files_when_rows_cannot_be_deleted() {
        let fixture = DeleteFixture::new();
        let a = fixture.add_track("a", true);
        let b = fixture.add_track("b", true);
        fixture
            .state
            .with_db(|db| {
                db.execute_batch(
                    "CREATE TRIGGER fail_delete BEFORE DELETE ON tracks WHEN old.id = 'b'
                     BEGIN SELECT RAISE(ABORT, '削除できない'); END;",
                )
                .map_err(|e| AppError::Database(e.to_string()))
            })
            .unwrap();
        let trash = |path: &Path| fixture.trash(path);

        let result = delete_tracks_with_files(&fixture.state, &ids(&["a", "b"]), &trash);

        assert!(matches!(result, Err(AppError::Database(_))));
        assert_eq!(fixture.track_ids(), ids(&["a", "b"]));
        assert!(a.is_file());
        assert!(b.is_file());
        assert!(!fixture.trash_path(&a).exists());
    }
}
