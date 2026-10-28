//! ファイルをOSのゴミ箱へ移す
//!
//! 形式の変換で元のファイルを置き換える時（ADR-054）と、曲をファイルごと削除する時（ADR-061）に、
//! ファイルを消さずにゴミ箱へ移す。利用者がゴミ箱から戻せる。macOSだけに対応する。

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// この環境で、ファイルをゴミ箱へ移せるか（macOSだけ）
pub const fn is_available() -> bool {
    cfg!(target_os = "macos")
}

/// ファイルをゴミ箱へ移す
///
/// ゴミ箱のないボリューム（一部のネットワークドライブなど）では、エラーになる（ファイルは消さない）。
/// @returns ゴミ箱の中での場所（元に戻す時に使う。分からない場合はNone）
#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> AppResult<Option<PathBuf>> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path = path
        .to_str()
        .ok_or_else(|| AppError::Io("ファイルのパスを扱えません".to_string()))?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let mut trashed = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut trashed))
        .map_err(|e| {
            AppError::Io(format!(
                "ファイルをゴミ箱へ移せませんでした: {}",
                e.localizedDescription()
            ))
        })?;
    Ok(trashed
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string())))
}

#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(_path: &Path) -> AppResult<Option<PathBuf>> {
    Err(AppError::Io(
        "この環境では、ファイルをゴミ箱へ移せません".to_string(),
    ))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    #[test]
    fn test_moves_a_file_to_the_trash_and_reports_where_it_went() {
        let dir = TempDir::new();
        let file = dir
            .path()
            .join(format!("muspice-trash-test-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&file, b"trash me").unwrap();

        let trashed = move_to_trash(&file)
            .unwrap()
            .expect("the location in the trash should be known");

        assert!(!file.exists());
        assert_eq!(std::fs::read(&trashed).unwrap(), b"trash me");
        // テストで入れたファイルは、ゴミ箱に残さない
        std::fs::remove_file(&trashed).unwrap();
    }

    #[test]
    fn test_missing_file_is_an_error() {
        let dir = TempDir::new();
        assert!(matches!(
            move_to_trash(&dir.path().join("missing.txt")),
            Err(AppError::Io(_))
        ));
    }
}
