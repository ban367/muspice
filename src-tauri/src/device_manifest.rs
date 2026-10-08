//! 転送先デバイス上の管理ファイル（`.muspice/manifest.json`）
//!
//! デバイスのIDと、Muspiceがデバイスにコピーしたファイルの一覧を記録する。
//!
//! - 同期で削除・上書きするのは、ここに記録のあるファイルだけにする（ユーザーが自分で
//!   デバイスに置いたファイルには触れない）
//! - 接続の判定に使う（転送先のフォルダに、同じIDの管理ファイルがあるか）
//! - パスはデバイスのフォルダからの相対パスで、区切りは`/`
//!
//! 管理ファイルは外部メディア上にあり、書き換えられている可能性がある。記録されたパスは
//! `is_safe_relative_path`で検証し、フォルダの外を指すものは読み込まない。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 管理ファイルを置くフォルダ（デバイスのフォルダの直下）
pub const MANIFEST_DIR: &str = ".muspice";
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_TEMP_FILE: &str = "manifest.json.tmp";

/// 管理ファイルの形式のバージョン（読めない新しい形式は、同期せずにエラーにする）
const MANIFEST_VERSION: u32 = 1;

/// デバイス上の管理ファイル
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub device_id: String,
    /// Muspiceがコピーした曲のファイル
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    /// Muspiceが書き出したプレイリストのファイル
    #[serde(default)]
    pub playlists: Vec<ManifestPlaylist>,
}

/// デバイスにコピーした曲のファイル
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    /// デバイスのフォルダからの相対パス（区切りは`/`）
    pub path: String,
    pub track_id: String,
    /// コピーした時点の、元のファイルのサイズと更新日時（UNIX時間の秒）
    ///
    /// 元のファイルが変わったかは、デバイス側の更新日時ではなくこの値と比べて判定する
    /// （FAT系の2秒の精度や、夏時間の切り替えによるずれの影響を受けない）。
    pub size: i64,
    pub modified_at: Option<i64>,
}

/// デバイスに書き出したプレイリストのファイル
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPlaylist {
    /// デバイスのフォルダからの相対パス（フォルダの直下に置くため、ファイル名のみ）
    pub path: String,
    pub playlist_id: String,
}

impl Manifest {
    /// 空の管理ファイル（デバイスの登録時に書く）
    pub fn new(device_id: &str) -> Self {
        Self {
            version: MANIFEST_VERSION,
            device_id: device_id.to_string(),
            files: Vec::new(),
            playlists: Vec::new(),
        }
    }
}

/// 管理ファイルに記録された相対パスが、デバイスのフォルダの中だけを指すか
///
/// 空の要素・`.`・`..`・絶対パス・`\`や`:`（Windowsのドライブ・代替データストリーム）を
/// 含むパスと、管理ファイルのフォルダ自身を指すパスは認めない。
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains(['\\', ':', '\0']) {
        return false;
    }
    let mut components = path.split('/');
    let first_is_manifest_dir = components
        .clone()
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case(MANIFEST_DIR));
    !first_is_manifest_dir && components.all(|c| !c.is_empty() && c != "." && c != "..")
}

/// 相対パス（区切りは`/`）を、デバイスのフォルダの中のパスにする
pub fn to_device_path(root: &Path, relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(MANIFEST_DIR).join(MANIFEST_FILE)
}

/// 管理ファイルを読む（ない場合はNone）
///
/// 壊れている・新しい形式で読めない場合はエラーにする（空として扱うと、デバイス上の
/// 曲を管理外とみなして、同じ曲を別の名前でもう一度コピーしてしまうため）。
/// フォルダの外を指すパスの記録は読み込まない。
pub fn read_manifest(root: &Path) -> AppResult<Option<Manifest>> {
    let path = manifest_path(root);
    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // 転送先のフォルダ自体がない（デバイスが外れている）場合も「ない」として扱う
        Err(_) if !root.is_dir() => return Ok(None),
        Err(e) => {
            return Err(AppError::Io(format!(
                "デバイスの管理ファイルを読み込めません: {}",
                e
            )));
        }
    };

    let mut manifest: Manifest = serde_json::from_slice(&content).map_err(|e| {
        AppError::Io(format!(
            "デバイスの管理ファイルが壊れています（{}）: {}",
            path.display(),
            e
        ))
    })?;
    if manifest.version > MANIFEST_VERSION {
        return Err(AppError::Validation(
            "デバイスの管理ファイルが新しいバージョンのMuspiceで作られているため、同期できません"
                .to_string(),
        ));
    }

    let file_count = manifest.files.len() + manifest.playlists.len();
    manifest.files.retain(|f| is_safe_relative_path(&f.path));
    manifest
        .playlists
        .retain(|p| is_safe_relative_path(&p.path));
    let dropped = file_count - manifest.files.len() - manifest.playlists.len();
    if dropped > 0 {
        log::warn!(
            "デバイスの管理ファイルに不正なパスの記録が{}件あったため、無視しました: {}",
            dropped,
            path.display()
        );
    }

    Ok(Some(manifest))
}

/// 管理ファイルに記録されたデバイスのID（管理ファイルがない・読めない場合はNone）
///
/// 接続の判定に使う。ファイルの一覧は読み込まない。
pub fn read_device_id(root: &Path) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Header {
        device_id: String,
    }

    let content = fs::read(manifest_path(root)).ok()?;
    serde_json::from_slice::<Header>(&content)
        .ok()
        .map(|header| header.device_id)
}

/// 管理ファイルを書く
///
/// 書き込みの途中でデバイスが外れても壊れないよう、別名で書いてから置き換える。
pub fn write_manifest(root: &Path, manifest: &Manifest) -> AppResult<()> {
    let to_error =
        |e: std::io::Error| AppError::Io(format!("デバイスの管理ファイルを書き込めません: {}", e));
    let dir = root.join(MANIFEST_DIR);
    fs::create_dir_all(&dir).map_err(to_error)?;

    let content = serde_json::to_vec(manifest)
        .map_err(|e| AppError::Io(format!("デバイスの管理ファイルを作成できません: {}", e)))?;
    let temp_path = dir.join(MANIFEST_TEMP_FILE);
    let mut file = fs::File::create(&temp_path).map_err(to_error)?;
    file.write_all(&content).map_err(to_error)?;
    // 置き換える前にディスクへ書き出す（取り外しで、空の管理ファイルが残らないようにする）
    file.sync_all().map_err(to_error)?;
    drop(file);
    fs::rename(&temp_path, dir.join(MANIFEST_FILE)).map_err(to_error)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    /// テスト用の一時フォルダ（破棄すると削除する）
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("一時フォルダの作成に失敗");
            Self(path)
        }

        pub fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TempDir;
    use super::*;

    #[test]
    fn test_is_safe_relative_path() {
        assert!(is_safe_relative_path("Artist/Album/01 Title.mp3"));
        assert!(is_safe_relative_path("通勤.m3u8"));
        // ファイル名に".."を含むだけなら認める
        assert!(is_safe_relative_path("Artist/Album/Live..2024.mp3"));

        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("../outside.mp3"));
        assert!(!is_safe_relative_path("Artist/../../outside.mp3"));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("Artist//Title.mp3"));
        assert!(!is_safe_relative_path("./Title.mp3"));
        assert!(!is_safe_relative_path("C:\\Windows\\system.ini"));
        assert!(!is_safe_relative_path("Artist\\Title.mp3"));
        assert!(!is_safe_relative_path("Title.mp3:stream"));
        // 管理ファイルのフォルダ自身は対象にしない
        assert!(!is_safe_relative_path(".muspice/manifest.json"));
        assert!(!is_safe_relative_path(".MUSPICE/manifest.json"));
    }

    #[test]
    fn test_to_device_path_joins_components() {
        let root = Path::new("/Volumes/SD");
        assert_eq!(
            to_device_path(root, "Artist/Album/01 Title.mp3"),
            root.join("Artist").join("Album").join("01 Title.mp3")
        );
    }

    #[test]
    fn test_write_and_read_manifest() {
        let dir = TempDir::new();
        assert_eq!(read_manifest(dir.path()).unwrap(), None);

        let mut manifest = Manifest::new("device-1");
        manifest.files.push(ManifestFile {
            path: "Artist/Album/01 Title.mp3".to_string(),
            track_id: "t1".to_string(),
            size: 123,
            modified_at: Some(456),
        });
        manifest.playlists.push(ManifestPlaylist {
            path: "通勤.m3u8".to_string(),
            playlist_id: "p1".to_string(),
        });
        write_manifest(dir.path(), &manifest).unwrap();

        assert_eq!(read_manifest(dir.path()).unwrap(), Some(manifest));
        assert_eq!(read_device_id(dir.path()).as_deref(), Some("device-1"));
        // 置き換えに使った別名のファイルは残らない
        assert!(
            !dir.path()
                .join(MANIFEST_DIR)
                .join(MANIFEST_TEMP_FILE)
                .exists()
        );
    }

    #[test]
    fn test_read_manifest_returns_none_when_folder_is_missing() {
        let dir = TempDir::new();
        assert_eq!(read_manifest(&dir.path().join("unplugged")).unwrap(), None);
        assert_eq!(read_device_id(&dir.path().join("unplugged")), None);
    }

    #[test]
    fn test_read_manifest_drops_unsafe_paths() {
        let dir = TempDir::new();
        fs::create_dir_all(dir.path().join(MANIFEST_DIR)).unwrap();
        fs::write(
            manifest_path(dir.path()),
            r#"{
                "version": 1,
                "deviceId": "device-1",
                "files": [
                    { "path": "Artist/Album/01 Title.mp3", "trackId": "t1", "size": 1, "modifiedAt": null },
                    { "path": "../../Documents/secret.txt", "trackId": "t2", "size": 1, "modifiedAt": null },
                    { "path": "/etc/passwd", "trackId": "t3", "size": 1, "modifiedAt": null }
                ],
                "playlists": [{ "path": "../evil.m3u8", "playlistId": "p1" }]
            }"#,
        )
        .unwrap();

        let manifest = read_manifest(dir.path()).unwrap().unwrap();
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["Artist/Album/01 Title.mp3"]);
        assert!(manifest.playlists.is_empty());
    }

    #[test]
    fn test_read_manifest_rejects_broken_or_newer_file() {
        let dir = TempDir::new();
        fs::create_dir_all(dir.path().join(MANIFEST_DIR)).unwrap();

        fs::write(manifest_path(dir.path()), "{ broken").unwrap();
        assert!(matches!(read_manifest(dir.path()), Err(AppError::Io(_))));

        fs::write(
            manifest_path(dir.path()),
            r#"{ "version": 99, "deviceId": "device-1" }"#,
        )
        .unwrap();
        assert!(matches!(
            read_manifest(dir.path()),
            Err(AppError::Validation(_))
        ));
    }
}
