//! アプリ自身がライブラリの曲のファイルへ書いた変更の記録
//!
//! ライブラリフォルダの監視（`library_sync`）は、フォルダの中の変更の通知を受けるたびに、フォルダ
//! 全体を再スキャンする。アプリ自身が書いた変更（タグの保存・評価・アルバムアート・音量の解析・
//! ファイルの整理・音楽フォルダへのコピー・形式の変換の置き換え・ファイルごとの削除）は、書いた後に
//! DBへも記録してあるため、再スキャンしても変わらない。ラウドネスの解析・タグの一括ツールのように
//! 多くのファイルへ続けて書くと、通知がまとまるたび（3秒ごと）にフォルダ全体の走査を繰り返すため、
//! 書いたファイルと、書いた直後の大きさ・更新日時を記録しておき、監視はその通知を無視する。
//!
//! - 記録するのは、DBへの記録まで済ませる所だけ（書き出すだけで登録しないファイルは記録しない。
//!   監視の再スキャンで取り込まれる）
//! - 通知が届いた時に、ファイルの大きさ・更新日時（消したファイルは、ないこと）が記録と同じ場合だけ
//!   無視する。記録の後にほかのアプリが書き換えた場合は、無視しない
//! - 記録は`RECORD_TTL`で消す（通知が遅れて届く分まで待つ。消えた後の通知は、ふつうに扱う）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

/// 記録を残す時間（監視が変更をまとめる3秒と、通知の遅れより十分に長くする）
const RECORD_TTL: Duration = Duration::from_secs(60);

/// ファイルの状態（ないファイルはNone）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy)]
struct Record {
    stamp: Option<FileStamp>,
    at: Instant,
}

/// 記録（プロセスで1つ。書き込む所は多く、どれも`AppState`を持たないため）
static RECORDS: Mutex<Option<HashMap<PathBuf, Record>>> = Mutex::new(None);

fn records() -> MutexGuard<'static, Option<HashMap<PathBuf, Record>>> {
    // 記録は、なくなっても困らない（再スキャンするだけ）ため、途中でパニックした後も続ける
    RECORDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 今のファイルの状態
fn stamp_of(path: &Path) -> Option<FileStamp> {
    fs::metadata(path).ok().map(|metadata| FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// 記録・照合に使うパス（書いたパスと、フォルダのシンボリックリンクを解決したパス）
///
/// 監視の通知は、OSによってはリンクを解決したパスで届く。消したファイルも引けるよう、フォルダだけを
/// 解決する。
fn keys_of(path: &Path) -> Vec<PathBuf> {
    let mut keys = vec![path.to_path_buf()];
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(parent) = fs::canonicalize(parent)
    {
        let resolved = parent.join(name);
        if resolved != path {
            keys.push(resolved);
        }
    }
    keys
}

/// アプリが、ライブラリの曲のファイルを書いた（置いた・消した）ことを記録する
///
/// 書き終えて、DBへの記録が済む（済ませる）所で呼ぶ。今のファイルの状態を記録する。
pub fn record(path: &Path) {
    record_at(path, Instant::now());
}

fn record_at(path: &Path, now: Instant) {
    let stamp = stamp_of(path);
    let mut guard = records();
    let records = guard.get_or_insert_with(HashMap::new);
    records.retain(|_, record| now.duration_since(record.at) < RECORD_TTL);
    for key in keys_of(path) {
        records.insert(key, Record { stamp, at: now });
    }
}

/// 監視の通知を受けた変更が、アプリ自身の書き込みか（記録があり、ファイルが記録した時のままか）
pub fn is_own_change(path: &Path) -> bool {
    is_own_change_at(path, Instant::now())
}

fn is_own_change_at(path: &Path, now: Instant) -> bool {
    let guard = records();
    let Some(records) = guard.as_ref() else {
        return false;
    };
    let Some(record) = keys_of(path)
        .iter()
        .find_map(|key| records.get(key).copied())
    else {
        return false;
    };
    drop(guard);
    now.duration_since(record.at) < RECORD_TTL && record.stamp == stamp_of(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    #[test]
    fn test_own_writes_are_recognized_until_the_file_changes() {
        let dir = TempDir::new();
        let file = dir.path().join("song.mp3");
        fs::write(&file, "tagged").unwrap();

        assert!(!is_own_change(&file));
        record(&file);
        assert!(is_own_change(&file));

        // 記録の後にほかのアプリが書き換えた（大きさが変わった）ら、自分の書き込みとして扱わない
        fs::write(&file, "edited by another app").unwrap();
        assert!(!is_own_change(&file));
    }

    #[test]
    fn test_removed_files_are_recognized_while_they_stay_removed() {
        let dir = TempDir::new();
        let file = dir.path().join("deleted.flac");
        fs::write(&file, "audio").unwrap();
        fs::remove_file(&file).unwrap();

        record(&file);
        assert!(is_own_change(&file));
        // 同じ名前のファイルが置かれたら、ふつうの変更として扱う
        fs::write(&file, "new").unwrap();
        assert!(!is_own_change(&file));
    }

    #[test]
    fn test_records_expire() {
        let dir = TempDir::new();
        let file = dir.path().join("old.mp3");
        fs::write(&file, "audio").unwrap();
        let written_at = Instant::now();

        record_at(&file, written_at);

        assert!(is_own_change_at(&file, written_at + RECORD_TTL / 2));
        assert!(!is_own_change_at(&file, written_at + RECORD_TTL));
    }

    #[cfg(unix)]
    #[test]
    fn test_paths_through_a_linked_folder_are_matched() {
        let dir = TempDir::new();
        let real = dir.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        fs::write(real.join("a.mp3"), "audio").unwrap();

        // リンクを通して書いたファイルの通知が、リンクを解決したパスで届いても引ける
        record(&link.join("a.mp3"));
        assert!(is_own_change(
            &fs::canonicalize(&real).unwrap().join("a.mp3")
        ));
    }
}
