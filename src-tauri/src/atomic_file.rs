//! 小さなファイル（設定・再生状態・デバイスの管理ファイル）を、壊さずに書き換える
//!
//! 別の名前の一時ファイルに書き、ディスクへ書き出してから（`sync_all`）、`rename`で置き換える。
//! 書き出さずに置き換えると、停電・強制終了の直後に、置き換えたファイルが空（0バイト）で残る
//! ことがある。音楽ファイルへの書き込みは、元のファイルを写してから書く別の方法で守る
//! （`metadata::write_atomically`。ADR-074）。

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::Path;

/// `content`を`temp_path`に書き、ディスクへ書き出してから`path`と置き換える
///
/// - 一時ファイルは、前の書き込みの残りがあれば消してから、新しく作る（`create_new`。一時ファイルの
///   場所にあるシンボリックリンクの先へは書かない）
/// - `temp_path`は、`path`と同じフォルダに置く（`rename`で一度に置き換えるため）
/// - 置き換えた後、フォルダもディスクへ書き出す（できない環境では、書き出さずに続ける）
/// - 途中で失敗した場合は、一時ファイルを消す（`path`は前の内容のまま）
pub fn replace_file(path: &Path, temp_path: &Path, content: &[u8]) -> io::Result<()> {
    let written = write_synced(temp_path, content).and_then(|()| fs::rename(temp_path, path));
    if written.is_err() {
        fs::remove_file(temp_path).ok();
        return written;
    }
    if let Some(dir) = path.parent() {
        sync_dir(dir);
    }
    Ok(())
}

/// 一時ファイルを作り、書いてディスクへ書き出す
fn write_synced(temp_path: &Path, content: &[u8]) -> io::Result<()> {
    match fs::remove_file(temp_path) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)?;
    file.write_all(content)?;
    file.sync_all()
}

/// フォルダをディスクへ書き出す（置き換えた名前を残す。Windowsなど、フォルダを開けない環境では何もしない）
fn sync_dir(dir: &Path) {
    if let Ok(dir) = File::open(dir) {
        dir.sync_all().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    #[test]
    fn test_replaces_the_file_and_leaves_no_temp_file() {
        let dir = TempDir::new();
        let path = dir.path().join("settings.json");
        let temp = dir.path().join("settings.json.tmp");
        fs::write(&path, "old").unwrap();
        // 前の書き込みの残り
        fs::write(&temp, "partial").unwrap();

        replace_file(&path, &temp, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!temp.exists());
    }

    #[test]
    fn test_failure_keeps_the_previous_file() {
        let dir = TempDir::new();
        let path = dir.path().join("settings.json");
        fs::write(&path, "old").unwrap();
        // 一時ファイルを作れない場所（ないフォルダの中）
        let temp = dir.path().join("missing").join("settings.json.tmp");

        assert!(replace_file(&path, &temp, b"new").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    }
}
