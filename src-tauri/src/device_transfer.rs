//! デバイスへの同期の実行（削除・リネーム・コピー・プレイリストの書き出し）
//!
//! `device_sync::plan_sync`が決めた処理を、デバイスのフォルダに対して行う。
//!
//! - 順序は「削除 → リネーム → コピー → プレイリスト」（削除で空いた容量と場所を先に使えるようにする）
//! - コピーは`std::fs::copy`を使わず、読み書きを自分で行う。macOSの`fs::copy`は拡張属性も
//!   コピーするため、FAT32・exFATでは`._`で始まるファイルができ、機器によっては曲として
//!   表示されてしまう。進捗の通知と、ファイルの途中での中止もできる
//! - コピーは別名（`.part`）に書いてから置き換える（中断しても、壊れたファイルを曲として残さない）
//! - ファイル単位の失敗は記録して続ける。デバイスが外れた場合は中止する
//! - 管理ファイルは途中でも保存する（中止・取り外しの後、次の同期で続きから差分になる）

use crate::device_manifest::{
    Manifest, ManifestFile, ManifestPlaylist, to_device_path, write_manifest,
};
use crate::device_sync::{CopyOp, SyncPlan, build_m3u8, path_key};
use crate::error::{AppError, AppResult};
use crate::library::modified_at_of;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// コピー中の一時ファイルの接尾辞
const TEMP_SUFFIX: &str = ".part";

/// コピーの読み書きの単位
const COPY_BUFFER_SIZE: usize = 1024 * 1024;

/// 管理ファイルを保存する間隔（コピーしたファイル数）
const MANIFEST_SAVE_INTERVAL: u32 = 50;

/// 1つのファイルのコピー中に、進捗を通知する間隔
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// コピーの進捗
#[derive(Debug, Clone, PartialEq)]
pub struct TransferProgress {
    /// コピーが済んだファイル数
    pub current: u32,
    /// コピーするファイルの総数
    pub total: u32,
    /// コピーが済んだバイト数と、コピーするバイト数の合計
    pub bytes_done: i64,
    pub bytes_total: i64,
    /// コピー中のファイル名
    pub current_file: String,
}

/// 同期の実行結果
#[derive(Debug, Default, PartialEq)]
pub struct TransferOutcome {
    pub copied_count: u32,
    pub deleted_count: u32,
    pub renamed_count: u32,
    /// 書き出したプレイリスト数
    pub playlist_count: u32,
    /// 失敗したファイルと理由
    pub errors: Vec<String>,
    /// 途中で中止された
    pub cancelled: bool,
}

/// デバイスのフォルダと、実行中の管理ファイルの内容
struct Device<'a> {
    root: &'a Path,
    /// シンボリックリンクを解決したフォルダのパス（フォルダの外を指すパスの検出に使う）
    canonical_root: PathBuf,
    device_id: String,
    version: u32,
    /// コピー済みの曲（パスのキー → 記録）
    files: BTreeMap<String, ManifestFile>,
    /// 書き出し済みのプレイリスト（パスのキー → 記録）
    playlists: BTreeMap<String, ManifestPlaylist>,
}

impl Device<'_> {
    /// デバイスのフォルダの中のパスにする
    ///
    /// 親フォルダがシンボリックリンクなどでデバイスのフォルダの外を指す場合はエラーにする
    /// （管理ファイルの記録をもとに、フォルダの外のファイルを削除・上書きしない）。
    /// `create_parent`なら、親フォルダがなければ作る。
    fn resolve(&self, relative_path: &str, create_parent: bool) -> io::Result<PathBuf> {
        let path = to_device_path(self.root, relative_path);
        if let Some(parent) = path.parent() {
            if create_parent {
                fs::create_dir_all(parent)?;
            }
            if !parent.canonicalize()?.starts_with(&self.canonical_root) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "デバイスのフォルダの外を指すパスです",
                ));
            }
        }
        Ok(path)
    }

    /// 管理ファイルを保存する
    fn save(&self) -> AppResult<()> {
        let manifest = Manifest {
            version: self.version,
            device_id: self.device_id.clone(),
            files: self.files.values().cloned().collect(),
            playlists: self.playlists.values().cloned().collect(),
        };
        write_manifest(self.root, &manifest)
    }

    /// デバイスのフォルダが見つからなくなった（取り外された）場合のエラー
    fn ensure_connected(&self) -> AppResult<()> {
        if self.root.is_dir() {
            Ok(())
        } else {
            Err(AppError::Io(
                "デバイスが見つからなくなったため、同期を中止しました".to_string(),
            ))
        }
    }

    /// 空になったフォルダを、デバイスのフォルダの手前まで削除する
    fn remove_empty_parents(&self, file: &Path) {
        let mut dir = file.parent();
        while let Some(current) = dir {
            if current == self.root || fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
    }
}

/// ファイルを削除する（すでにない場合も成功として扱う）
fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// 一時ファイルのパス（ファイル名に`.part`を足す）
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

/// ファイルを別名で書いてから置き換える
///
/// `write`がfalseを返した（中止された）場合と失敗した場合は、一時ファイルを消す。
/// 置き換えたらtrueを返す。
fn write_atomically(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<bool>,
) -> io::Result<bool> {
    let temp = temp_path(path);
    let result = fs::File::create(&temp).and_then(|mut file| {
        let completed = write(&mut file)?;
        if completed {
            file.flush()?;
        }
        Ok(completed)
    });
    match result {
        Ok(true) => fs::rename(&temp, path).map(|()| true),
        other => {
            fs::remove_file(&temp).ok();
            other
        }
    }
}

/// ファイルをコピーする
///
/// 書き込んだバイト数を返す。途中で中止された場合はNone。
/// `on_bytes`には、書き込み済みのバイト数を渡す。
fn copy_file(
    source: &Path,
    dest: &Path,
    buffer: &mut [u8],
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(i64),
) -> io::Result<Option<i64>> {
    let mut reader = fs::File::open(source)?;
    let mut written: i64 = 0;
    let completed = write_atomically(dest, |writer| {
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(false);
            }
            let read = reader.read(buffer)?;
            if read == 0 {
                return Ok(true);
            }
            writer.write_all(&buffer[..read])?;
            written += read as i64;
            on_bytes(written);
        }
    })?;
    Ok(completed.then_some(written))
}

/// 進捗の通知に使うファイル名
fn display_name(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .to_string()
}

/// 同期を実行する
///
/// - `manifest`: 同期を始める前の管理ファイル（`plan`を作ったときのもの）
/// - `cancel`: trueにすると、次のファイルの前（コピー中ならその途中）で中止する
/// - `on_progress`: コピーの進捗
///
/// デバイスが外れた場合と、最後の管理ファイルの保存に失敗した場合はエラーを返す。
pub fn execute_plan(
    root: &Path,
    manifest: Manifest,
    plan: &SyncPlan,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(TransferProgress),
) -> AppResult<TransferOutcome> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::Io(format!("デバイスのフォルダにアクセスできません: {}", e)))?;
    let mut device = Device {
        root,
        canonical_root,
        device_id: manifest.device_id,
        version: manifest.version,
        files: BTreeMap::new(),
        playlists: BTreeMap::new(),
    };
    // 今の記録から始めて、済んだ処理を反映していく（同じパスの記録は最初の1件を使う）
    for entry in manifest.files {
        device.files.entry(path_key(&entry.path)).or_insert(entry);
    }
    for entry in manifest.playlists {
        device
            .playlists
            .entry(path_key(&entry.path))
            .or_insert(entry);
    }
    // そのまま残す曲（コピー済みとして扱うことにした、記録のないファイルを含む）
    for entry in &plan.kept {
        device.files.insert(path_key(&entry.path), entry.clone());
    }

    let mut outcome = TransferOutcome::default();
    let is_cancelled = || cancel.load(Ordering::SeqCst);

    // 1. 削除
    for entry in &plan.to_delete {
        if is_cancelled() {
            break;
        }
        let removed = device
            .resolve(&entry.path, false)
            .and_then(|path| remove_file_if_exists(&path).map(|()| path));
        match removed {
            Ok(path) => {
                device.remove_empty_parents(&path);
                device.files.remove(&path_key(&entry.path));
                outcome.deleted_count += 1;
            }
            // 親フォルダごとなくなっている場合も、削除済みとして扱う
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                device.files.remove(&path_key(&entry.path));
            }
            Err(e) => {
                device.ensure_connected()?;
                outcome
                    .errors
                    .push(format!("{}: 削除できませんでした（{}）", entry.path, e));
            }
        }
    }

    // 2. リネーム
    for op in &plan.to_rename {
        if is_cancelled() {
            break;
        }
        let renamed = device.resolve(&op.entry.path, false).and_then(|from| {
            let to = device.resolve(&op.to, true)?;
            fs::rename(&from, to).map(|()| from)
        });
        match renamed {
            Ok(from) => {
                device.remove_empty_parents(&from);
                device.files.remove(&path_key(&op.entry.path));
                device.files.insert(
                    path_key(&op.to),
                    ManifestFile {
                        path: op.to.clone(),
                        ..op.entry.clone()
                    },
                );
                outcome.renamed_count += 1;
            }
            Err(e) => {
                device.ensure_connected()?;
                outcome.errors.push(format!(
                    "{}: 名前を変更できませんでした（{}）",
                    op.entry.path, e
                ));
            }
        }
    }
    if outcome.deleted_count > 0 || outcome.renamed_count > 0 {
        device.save()?;
    }

    // 3. コピー
    let storage_full = copy_files(&mut device, plan, cancel, on_progress, &mut outcome)?;
    outcome.cancelled = is_cancelled();

    // 4. プレイリスト（中止された場合と、容量が足りなくなった場合は書き出さない）
    if !outcome.cancelled && !storage_full {
        write_playlists(&mut device, plan, &mut outcome)?;
    }

    device.save()?;
    Ok(outcome)
}

/// 曲をコピーする
///
/// デバイスの容量が足りなくなって打ち切った場合はtrueを返す。
fn copy_files(
    device: &mut Device,
    plan: &SyncPlan,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(TransferProgress),
    outcome: &mut TransferOutcome,
) -> AppResult<bool> {
    let total = u32::try_from(plan.to_copy.len()).unwrap_or(u32::MAX);
    let bytes_total = plan.copy_bytes();
    let mut bytes_done: i64 = 0;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let mut unsaved: u32 = 0;

    for (index, op) in plan.to_copy.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let progress = |bytes_done: i64| TransferProgress {
            current: u32::try_from(index).unwrap_or(u32::MAX),
            total,
            bytes_done,
            bytes_total,
            current_file: display_name(&op.dest),
        };
        on_progress(progress(bytes_done));

        let mut last_reported = Instant::now();
        let copied = copy_track(device, op, &mut buffer, cancel, &mut |written| {
            if last_reported.elapsed() >= PROGRESS_INTERVAL {
                last_reported = Instant::now();
                on_progress(progress(bytes_done + written));
            }
        });
        match copied {
            Ok(Some(entry)) => {
                device.files.insert(path_key(&entry.path), entry);
                outcome.copied_count += 1;
                unsaved += 1;
            }
            // 途中で中止された
            Ok(None) => break,
            Err(e) => {
                device.ensure_connected()?;
                if e.kind() == io::ErrorKind::StorageFull {
                    outcome.errors.push(
                        "デバイスの空き容量が足りなくなったため、コピーを打ち切りました"
                            .to_string(),
                    );
                    return Ok(true);
                }
                outcome.errors.push(format!("{}: {}", op.source, e));
            }
        }
        bytes_done += op.size;

        if unsaved >= MANIFEST_SAVE_INTERVAL {
            unsaved = 0;
            if let Err(e) = device.save() {
                device.ensure_connected()?;
                log::warn!("同期の途中の管理ファイルの保存に失敗しました: {}", e);
            }
        }
    }

    if !cancel.load(Ordering::SeqCst) {
        on_progress(TransferProgress {
            current: total,
            total,
            bytes_done: bytes_total,
            bytes_total,
            current_file: String::new(),
        });
    }
    Ok(false)
}

/// 1曲をコピーし、管理ファイルに記録する内容を返す（途中で中止された場合はNone）
fn copy_track(
    device: &Device,
    op: &CopyOp,
    buffer: &mut [u8],
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(i64),
) -> io::Result<Option<ManifestFile>> {
    let source = Path::new(&op.source);
    let dest = device.resolve(&op.dest, true)?;
    let Some(written) = copy_file(source, &dest, buffer, cancel, on_bytes)? else {
        return Ok(None);
    };

    // 差分の計算の後に元のファイルが変わっていた場合に備えて、コピーした内容の状態を記録する
    let modified_at = fs::metadata(source)
        .ok()
        .and_then(|metadata| modified_at_of(&metadata))
        .or(op.modified_at);
    Ok(Some(ManifestFile {
        path: op.dest.clone(),
        track_id: op.track_id.clone(),
        size: written,
        modified_at,
    }))
}

/// プレイリストのファイルを削除・書き出しする
fn write_playlists(
    device: &mut Device,
    plan: &SyncPlan,
    outcome: &mut TransferOutcome,
) -> AppResult<()> {
    for entry in &plan.playlists_to_delete {
        let removed = device
            .resolve(&entry.path, false)
            .and_then(|path| remove_file_if_exists(&path));
        match removed {
            Ok(()) => {
                device.playlists.remove(&path_key(&entry.path));
            }
            Err(e) => {
                device.ensure_connected()?;
                outcome
                    .errors
                    .push(format!("{}: 削除できませんでした（{}）", entry.path, e));
            }
        }
    }

    // デバイスにある曲だけをプレイリストに書く
    let track_paths: HashMap<&str, &str> = device
        .files
        .values()
        .map(|file| (file.track_id.as_str(), file.path.as_str()))
        .collect();
    let mut written: Vec<ManifestPlaylist> = Vec::new();
    for op in &plan.playlists {
        let content = build_m3u8(&op.entries, &track_paths);
        let result = device.resolve(&op.path, false).and_then(|path| {
            write_atomically(&path, |file| {
                file.write_all(content.as_bytes()).map(|()| true)
            })
        });
        match result {
            Ok(_) => written.push(ManifestPlaylist {
                path: op.path.clone(),
                playlist_id: op.playlist_id.clone(),
            }),
            Err(e) => {
                device.ensure_connected()?;
                outcome
                    .errors
                    .push(format!("{}: 書き出せませんでした（{}）", op.path, e));
            }
        }
    }

    for entry in written {
        device.playlists.insert(path_key(&entry.path), entry);
        outcome.playlist_count += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::device_manifest::{MANIFEST_DIR, read_manifest};
    use crate::device_sync::{PlaylistEntry, PlaylistOp, RenameOp};

    /// 元のファイルを置くフォルダと、デバイスのフォルダ
    struct Fixture {
        _dir: TempDir,
        library: PathBuf,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = TempDir::new();
            let library = dir.path().join("library");
            let root = dir.path().join("device");
            fs::create_dir_all(&library).unwrap();
            fs::create_dir_all(&root).unwrap();
            write_manifest(&root, &Manifest::new("device-1")).unwrap();
            Self {
                _dir: dir,
                library,
                root,
            }
        }

        /// 元のファイルを作り、コピーの処理を返す
        fn copy_op(&self, track_id: &str, dest: &str, content: &str) -> CopyOp {
            let source = self.library.join(format!("{}.mp3", track_id));
            fs::write(&source, content).unwrap();
            CopyOp {
                track_id: track_id.to_string(),
                source: source.to_str().unwrap().to_string(),
                dest: dest.to_string(),
                size: content.len() as i64,
                modified_at: Some(1),
            }
        }

        /// デバイスにファイルを置き、管理ファイルの記録を返す
        fn device_file(&self, track_id: &str, path: &str, content: &str) -> ManifestFile {
            let file = to_device_path(&self.root, path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
            ManifestFile {
                path: path.to_string(),
                track_id: track_id.to_string(),
                size: content.len() as i64,
                modified_at: Some(1),
            }
        }

        fn manifest(&self, files: Vec<ManifestFile>) -> Manifest {
            Manifest {
                files,
                ..Manifest::new("device-1")
            }
        }

        fn execute(&self, manifest: Manifest, plan: &SyncPlan) -> TransferOutcome {
            execute_plan(
                &self.root,
                manifest,
                plan,
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .unwrap()
        }

        fn read(&self, path: &str) -> Option<String> {
            fs::read_to_string(to_device_path(&self.root, path)).ok()
        }

        fn saved_paths(&self) -> Vec<String> {
            read_manifest(&self.root)
                .unwrap()
                .unwrap()
                .files
                .into_iter()
                .map(|f| f.path)
                .collect()
        }
    }

    #[test]
    fn test_copies_files_and_records_them() {
        let fixture = Fixture::new();
        let plan = SyncPlan {
            to_copy: vec![
                fixture.copy_op("t1", "Artist/Album/01 One.mp3", "one"),
                fixture.copy_op("t2", "Artist/Album/02 Two.mp3", "two!"),
            ],
            ..SyncPlan::default()
        };
        let mut progress: Vec<TransferProgress> = Vec::new();

        let outcome = execute_plan(
            &fixture.root,
            fixture.manifest(vec![]),
            &plan,
            &AtomicBool::new(false),
            &mut |p| progress.push(p),
        )
        .unwrap();

        assert_eq!(outcome.copied_count, 2);
        assert!(outcome.errors.is_empty() && !outcome.cancelled);
        assert_eq!(
            fixture.read("Artist/Album/01 One.mp3").as_deref(),
            Some("one")
        );
        assert_eq!(
            fixture.read("Artist/Album/02 Two.mp3").as_deref(),
            Some("two!")
        );
        // 一時ファイルは残らない
        assert_eq!(fixture.read("Artist/Album/01 One.mp3.part"), None);

        let saved = read_manifest(&fixture.root).unwrap().unwrap();
        assert_eq!(saved.device_id, "device-1");
        assert_eq!(
            saved
                .files
                .iter()
                .map(|f| (f.path.as_str(), f.size))
                .collect::<Vec<_>>(),
            [
                ("Artist/Album/01 One.mp3", 3),
                ("Artist/Album/02 Two.mp3", 4)
            ]
        );

        // ファイルごとの開始と、最後に完了を通知する
        assert_eq!(progress.first().map(|p| (p.current, p.total)), Some((0, 2)));
        assert_eq!(progress.first().unwrap().current_file, "01 One.mp3");
        let last = progress.last().unwrap();
        assert_eq!((last.current, last.bytes_done, last.bytes_total), (2, 7, 7));
    }

    #[test]
    fn test_deletes_managed_files_and_empty_folders_only() {
        let fixture = Fixture::new();
        let gone = fixture.device_file("t1", "Artist/Old Album/01 Gone.mp3", "gone");
        let kept = fixture.device_file("t2", "Artist/Album/01 Kept.mp3", "kept");
        let other = fixture.device_file("t3", "Other/Album/01 Removed.mp3", "removed");
        // ユーザーが自分で置いたファイル
        fs::write(fixture.root.join("Other").join("notes.txt"), "mine").unwrap();
        let plan = SyncPlan {
            to_delete: vec![gone.clone(), other.clone()],
            kept: vec![kept.clone()],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![gone, kept, other]), &plan);

        assert_eq!(outcome.deleted_count, 2);
        assert_eq!(fixture.read("Artist/Old Album/01 Gone.mp3"), None);
        assert_eq!(
            fixture.read("Artist/Album/01 Kept.mp3").as_deref(),
            Some("kept")
        );
        // 空になったフォルダは消し、他のファイルが残るフォルダは消さない
        assert!(!fixture.root.join("Artist").join("Old Album").exists());
        assert!(!fixture.root.join("Other").join("Album").exists());
        assert_eq!(fixture.read("Other/notes.txt").as_deref(), Some("mine"));
        assert_eq!(fixture.saved_paths(), ["Artist/Album/01 Kept.mp3"]);
    }

    #[test]
    fn test_delete_of_missing_file_drops_the_record() {
        let fixture = Fixture::new();
        let entry = ManifestFile {
            path: "Artist/Album/01 Gone.mp3".to_string(),
            track_id: "t1".to_string(),
            size: 4,
            modified_at: None,
        };
        let plan = SyncPlan {
            to_delete: vec![entry.clone()],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![entry]), &plan);

        assert!(outcome.errors.is_empty());
        assert!(fixture.saved_paths().is_empty());
    }

    #[test]
    fn test_renames_files() {
        let fixture = Fixture::new();
        let entry = fixture.device_file("t1", "Artist/Album/01 Old.mp3", "song");
        let plan = SyncPlan {
            to_rename: vec![RenameOp {
                entry: entry.clone(),
                to: "New Artist/Album/01 New.mp3".to_string(),
            }],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![entry]), &plan);

        assert_eq!(outcome.renamed_count, 1);
        assert_eq!(
            fixture.read("New Artist/Album/01 New.mp3").as_deref(),
            Some("song")
        );
        assert!(!fixture.root.join("Artist").exists());
        let saved = read_manifest(&fixture.root).unwrap().unwrap();
        assert_eq!(saved.files.len(), 1);
        assert_eq!(saved.files[0].path, "New Artist/Album/01 New.mp3");
        assert_eq!(saved.files[0].track_id, "t1");
    }

    #[test]
    fn test_recopy_replaces_file_and_record() {
        let fixture = Fixture::new();
        let path = "Artist/Album/01 One.mp3";
        let old = fixture.device_file("t1", path, "old");
        let plan = SyncPlan {
            to_copy: vec![fixture.copy_op("t1", path, "new content")],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![old]), &plan);

        assert_eq!(outcome.copied_count, 1);
        assert_eq!(fixture.read(path).as_deref(), Some("new content"));
        let saved = read_manifest(&fixture.root).unwrap().unwrap();
        assert_eq!(saved.files.len(), 1);
        assert_eq!(saved.files[0].size, 11);
    }

    #[test]
    fn test_failed_copy_is_reported_and_others_continue() {
        let fixture = Fixture::new();
        let mut missing = fixture.copy_op("t1", "Artist/Album/01 Missing.mp3", "x");
        fs::remove_file(&missing.source).unwrap();
        missing.size = 1;
        let plan = SyncPlan {
            to_copy: vec![
                missing,
                fixture.copy_op("t2", "Artist/Album/02 Two.mp3", "two"),
            ],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![]), &plan);

        assert_eq!(outcome.copied_count, 1);
        assert_eq!(outcome.errors.len(), 1);
        assert_eq!(fixture.read("Artist/Album/01 Missing.mp3"), None);
        assert_eq!(fixture.read("Artist/Album/01 Missing.mp3.part"), None);
        assert_eq!(fixture.saved_paths(), ["Artist/Album/02 Two.mp3"]);
    }

    #[test]
    fn test_cancel_stops_before_copy_and_keeps_records() {
        let fixture = Fixture::new();
        let kept = fixture.device_file("t0", "Artist/Album/00 Kept.mp3", "kept");
        let mut recorded = fixture.manifest(vec![kept.clone()]);
        recorded.playlists.push(ManifestPlaylist {
            path: "通勤.m3u8".to_string(),
            playlist_id: "p1".to_string(),
        });
        let plan = SyncPlan {
            to_copy: vec![fixture.copy_op("t1", "Artist/Album/01 One.mp3", "one")],
            kept: vec![kept],
            playlists: vec![PlaylistOp {
                playlist_id: "p1".to_string(),
                path: "通勤.m3u8".to_string(),
                entries: vec![],
            }],
            ..SyncPlan::default()
        };

        let outcome = execute_plan(
            &fixture.root,
            recorded.clone(),
            &plan,
            &AtomicBool::new(true),
            &mut |_| {},
        )
        .unwrap();

        assert!(outcome.cancelled);
        assert_eq!((outcome.copied_count, outcome.playlist_count), (0, 0));
        assert_eq!(fixture.read("Artist/Album/01 One.mp3"), None);
        // プレイリストは書き出さず、記録はそのまま残る
        assert_eq!(fixture.read("通勤.m3u8"), None);
        assert_eq!(read_manifest(&fixture.root).unwrap().unwrap(), recorded);
    }

    #[test]
    fn test_cancel_during_copy_removes_partial_file() {
        let fixture = Fixture::new();
        let op = fixture.copy_op("t1", "Artist/Album/01 One.mp3", "one");
        let dest = fixture.root.join("01 One.mp3");
        let cancel = AtomicBool::new(false);
        let mut buffer = [0u8; 1];

        // 1バイト書いたところで中止する
        let written = copy_file(
            Path::new(&op.source),
            &dest,
            &mut buffer,
            &cancel,
            &mut |_| cancel.store(true, Ordering::SeqCst),
        )
        .unwrap();

        assert_eq!(written, None);
        assert!(!dest.exists());
        assert!(!temp_path(&dest).exists());
    }

    #[test]
    fn test_writes_and_deletes_playlists() {
        let fixture = Fixture::new();
        let on_device = fixture.device_file("t1", "Artist/Album/01 One.mp3", "one");
        fs::write(fixture.root.join("古い名前.m3u8"), "old").unwrap();
        let mut recorded = fixture.manifest(vec![on_device.clone()]);
        recorded.playlists.push(ManifestPlaylist {
            path: "古い名前.m3u8".to_string(),
            playlist_id: "p1".to_string(),
        });
        let entry = |track_id: &str, label: &str| PlaylistEntry {
            track_id: track_id.to_string(),
            duration: Some(60),
            label: label.to_string(),
        };
        let plan = SyncPlan {
            to_copy: vec![fixture.copy_op("t2", "Artist/Album/02 Two.mp3", "two")],
            kept: vec![on_device],
            playlists: vec![PlaylistOp {
                playlist_id: "p1".to_string(),
                path: "新しい名前.m3u8".to_string(),
                // t3はデバイスにないため、書き出さない
                entries: vec![
                    entry("t2", "Artist - Two"),
                    entry("t3", "x"),
                    entry("t1", "Artist - One"),
                ],
            }],
            playlists_to_delete: recorded.playlists.clone(),
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(recorded, &plan);

        assert_eq!(outcome.playlist_count, 1);
        assert_eq!(fixture.read("古い名前.m3u8"), None);
        assert_eq!(
            fixture.read("新しい名前.m3u8").as_deref(),
            Some(
                "#EXTM3U\r\n\
                 #EXTINF:60,Artist - Two\r\nArtist/Album/02 Two.mp3\r\n\
                 #EXTINF:60,Artist - One\r\nArtist/Album/01 One.mp3\r\n"
            )
        );
        let saved = read_manifest(&fixture.root).unwrap().unwrap();
        assert_eq!(
            saved.playlists,
            [ManifestPlaylist {
                path: "新しい名前.m3u8".to_string(),
                playlist_id: "p1".to_string()
            }]
        );
    }

    #[test]
    fn test_adopted_files_are_recorded() {
        let fixture = Fixture::new();
        // 記録のないファイルを、コピー済みとして扱う
        let adopted = fixture.device_file("t1", "Artist/Album/01 One.mp3", "one");
        let plan = SyncPlan {
            kept: vec![adopted],
            ..SyncPlan::default()
        };

        fixture.execute(fixture.manifest(vec![]), &plan);

        assert_eq!(fixture.saved_paths(), ["Artist/Album/01 One.mp3"]);
    }

    #[test]
    fn test_fails_when_device_folder_is_missing() {
        let fixture = Fixture::new();
        let result = execute_plan(
            &fixture.root.join("unplugged"),
            fixture.manifest(vec![]),
            &SyncPlan::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(matches!(result, Err(AppError::Io(_))));
    }

    #[cfg(unix)]
    #[test]
    fn test_refuses_paths_that_escape_through_symlink() {
        let fixture = Fixture::new();
        // デバイスのフォルダの中に、外のフォルダを指すシンボリックリンクがある
        let outside = fixture.library.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.join("link")).unwrap();
        let entry = ManifestFile {
            path: "link/secret.txt".to_string(),
            track_id: "t1".to_string(),
            size: 6,
            modified_at: None,
        };
        let plan = SyncPlan {
            to_delete: vec![entry.clone()],
            to_copy: vec![fixture.copy_op("t2", "link/02 Two.mp3", "two")],
            ..SyncPlan::default()
        };

        let outcome = fixture.execute(fixture.manifest(vec![entry]), &plan);

        assert_eq!((outcome.deleted_count, outcome.copied_count), (0, 0));
        assert_eq!(outcome.errors.len(), 2);
        assert!(outside.join("secret.txt").exists());
        assert!(!outside.join("02 Two.mp3").exists());
    }

    #[test]
    fn test_manifest_folder_is_kept() {
        let fixture = Fixture::new();
        fixture.execute(fixture.manifest(vec![]), &SyncPlan::default());
        assert!(fixture.root.join(MANIFEST_DIR).is_dir());
    }
}
