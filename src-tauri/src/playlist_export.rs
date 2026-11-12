//! プレイリストの、音楽フォルダへの書き出し（ADR-085）
//!
//! 通常のプレイリストを、音楽フォルダの`Playlists`へM3U8で書き出す（アプリからファイルへの一方向。
//! DBが正で、ファイルは写し）。プレイリストのフォルダは、`Playlists`の中のフォルダにする。
//!
//! - 曲の場所は、音楽フォルダの中の曲は書き出したファイルからの相対パス（音楽フォルダごと移しても
//!   使える）、外の曲は絶対パス
//! - 自動プレイリストは、条件で曲が変わるため書き出さない
//! - 書いたファイルは、`Playlists/.muspice-playlists.json`に記録する（プレイリストのID → ファイル）。
//!   消す・名前を変えるのは、記録にあるファイルだけ（利用者が置いたファイルは消さない。同じ名前の
//!   ファイルがあれば、番号を付けた名前で書く）
//! - 内容が変わったファイルだけを書く（ネットワーク上の音楽フォルダへの書き込み・更新日時の変化を抑える）
//!
//! 書き直しは、プレイリストの変更・ライブラリの変更（曲の場所・名前が変わる）・設定の変更の後に、
//! 少し待ってまとめて行う（`PlaylistExport`）。

use crate::device_sync::sanitize_component;
use crate::error::{AppError, AppResult};
use crate::events::{LibraryChanged, SettingsChanged};
use crate::library_folder::{find_music_folder, track_path_prefix};
use crate::models::{Playlist, PlaylistFolder, Track};
use crate::settings::SettingsState;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Listener, Manager};
use tauri_specta::Event;

/// 書き出すフォルダの名前（音楽フォルダの中）
pub const PLAYLISTS_FOLDER_NAME: &str = "Playlists";

/// 書いたファイルの記録の名前（`Playlists`の中。`.`で始め、ライブラリの走査・監視で読み飛ばす）
const MANIFEST_FILE_NAME: &str = ".muspice-playlists.json";

/// 記録の形式の版
const MANIFEST_VERSION: u32 = 1;

/// 書き出すファイルの拡張子
const EXTENSION: &str = "m3u8";

/// 書き直しの依頼をまとめる時間（続けて変更した間は、最後の変更から待つ）
const DEBOUNCE: Duration = Duration::from_secs(2);

/// 書いたファイルの記録
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    /// プレイリストのID → `Playlists`からの相対パス（区切りは`/`）
    files: BTreeMap<String, String>,
}

/// 書き出しの結果
#[derive(Debug, Default, PartialEq)]
pub struct ExportResult {
    /// 書いた（内容が変わった・新しい）ファイルの数
    pub written: usize,
    /// 消したファイルの数（プレイリストを消した・名前を変えた・フォルダを移した）
    pub removed: usize,
}

/// 書き出すプレイリスト
struct PlannedFile {
    playlist_id: String,
    /// `Playlists`からの相対パス（区切りは`/`）
    relative_path: String,
    tracks: Vec<Track>,
}

/// 音楽フォルダの`Playlists`へ、通常のプレイリストを書き出す
///
/// `music_folder`は音楽フォルダ（今あるもの）。書き出すプレイリストも、前に書いたファイルもなければ、
/// `Playlists`を作らない。
pub fn export_playlists(state: &AppState, music_folder: &Path) -> AppResult<ExportResult> {
    let dir = music_folder.join(PLAYLISTS_FOLDER_NAME);
    let manifest_path = dir.join(MANIFEST_FILE_NAME);
    let manifest = read_manifest(&manifest_path);

    let context = state.smart_playlist_context();
    let (playlists, folders, tracks) = state.with_db(|db| {
        let playlists: Vec<Playlist> = crate::playlist::get_all_playlists(db)?
            .into_iter()
            .filter(|playlist| playlist.rules.is_none())
            .collect();
        let folders = crate::playlist::get_playlist_folders(db)?;
        let tracks = playlists
            .iter()
            .map(|playlist| crate::playlist::get_playlist_tracks(db, &playlist.id, context))
            .collect::<AppResult<Vec<_>>>()?;
        Ok((playlists, folders, tracks))
    })?;
    if playlists.is_empty() && manifest.files.is_empty() {
        return Ok(ExportResult::default());
    }

    let planned = plan_files(&playlists, &folders, tracks, &manifest, |relative| {
        dir.join(relative).exists()
    });
    let mut result = ExportResult::default();

    // 前に書いたファイルのうち、もう書かないもの（消した・名前を変えた・フォルダを移した）を消す
    let kept: HashSet<&str> = planned
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    for relative in manifest.files.values() {
        if kept.contains(relative.as_str()) {
            continue;
        }
        let path = dir.join(relative);
        match fs::remove_file(&path) {
            Ok(()) => result.removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!(
                "Failed to remove an exported playlist: {}: {}",
                path.display(),
                e
            ),
        }
        // プレイリストのフォルダの中が空になれば、フォルダも消す（`Playlists`は残す）
        if let Some(parent) = path.parent()
            && parent != dir
        {
            let _ = fs::remove_dir(parent);
        }
    }

    // 内容が変わったファイルだけを書く
    let music_prefix = music_folder
        .to_str()
        .map(track_path_prefix)
        .ok_or_else(|| AppError::Validation("フォルダの名前を扱えません".to_string()))?;
    for file in &planned {
        let path = dir.join(&file.relative_path);
        let base = path.parent().unwrap_or(&dir).to_path_buf();
        let content = crate::m3u::build_m3u8_with(&file.tracks, |track| {
            if track.file_path.starts_with(music_prefix.as_str()) {
                crate::m3u::relative_path(&track.file_path, &base)
            } else {
                track.file_path.clone()
            }
        });
        if fs::read(&path).is_ok_and(|current| current == content.as_bytes()) {
            continue;
        }
        fs::create_dir_all(&base).map_err(|e| {
            AppError::Io(format!(
                "プレイリストの書き出し先（{}）を作れません: {}",
                base.display(),
                e
            ))
        })?;
        write_file(&path, content.as_bytes())?;
        result.written += 1;
    }

    let next = Manifest {
        version: MANIFEST_VERSION,
        files: planned
            .into_iter()
            .map(|file| (file.playlist_id, file.relative_path))
            .collect(),
    };
    if next != manifest {
        let json = serde_json::to_vec_pretty(&next)
            .map_err(|e| AppError::Io(format!("記録を作れません: {}", e)))?;
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::Io(format!("プレイリストの書き出し先を作れません: {}", e)))?;
        write_file(&manifest_path, &json)?;
    }
    Ok(result)
}

/// ファイルを、同じフォルダの一時ファイルに書いてから置き換える
fn write_file(path: &Path, content: &[u8]) -> AppResult<()> {
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(path.file_name().unwrap_or_default());
    temp_name.push(".muspice-tmp");
    crate::atomic_file::replace_file(path, &path.with_file_name(temp_name), content).map_err(|e| {
        AppError::Io(format!(
            "プレイリストを書き出せません（{}）: {}",
            path.display(),
            e
        ))
    })
}

/// 書いたファイルの記録を読む（ない・読めない場合は空。読めない場合は、記録にあったファイルを消さない）
fn read_manifest(path: &Path) -> Manifest {
    let Ok(bytes) = fs::read(path) else {
        return Manifest::default();
    };
    match serde_json::from_slice::<Manifest>(&bytes) {
        Ok(manifest) if manifest.version <= MANIFEST_VERSION => manifest,
        Ok(_) => {
            log::warn!("Ignored the exported playlists record written by a newer version");
            Manifest::default()
        }
        Err(e) => {
            log::warn!("Failed to read the exported playlists record: {}", e);
            Manifest::default()
        }
    }
}

/// 書き出すファイルの場所を決める
///
/// `Playlists/<フォルダ名>/<プレイリスト名>.m3u8`（フォルダの外のプレイリストは、`Playlists`の直下）。
/// 名前はファイル名に使える形にする（デバイスへの転送と同じ決まり）。同じ場所になるプレイリスト・
/// 記録にないファイル（利用者が置いたファイル）がある場所は、番号を付けた名前にする。比べる時は、
/// 大文字と小文字を区別しない（macOSのファイルシステムの既定に合わせる）。
fn plan_files(
    playlists: &[Playlist],
    folders: &[PlaylistFolder],
    tracks: Vec<Vec<Track>>,
    manifest: &Manifest,
    exists: impl Fn(&str) -> bool,
) -> Vec<PlannedFile> {
    let owned: HashSet<String> = manifest
        .files
        .values()
        .map(|path| path.to_lowercase())
        .collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut planned = Vec::with_capacity(playlists.len());
    for (playlist, tracks) in playlists.iter().zip(tracks) {
        let folder = playlist
            .folder_id
            .as_deref()
            .and_then(|id| folders.iter().find(|folder| folder.id == id))
            .map(|folder| sanitize_component(&folder.name));
        let name = sanitize_component(&playlist.name);
        let relative_path = (1..)
            .map(|number| {
                let file_name = if number == 1 {
                    format!("{name}.{EXTENSION}")
                } else {
                    format!("{name} ({number}).{EXTENSION}")
                };
                match &folder {
                    Some(folder) => format!("{folder}/{file_name}"),
                    None => file_name,
                }
            })
            .find(|candidate| {
                let key = candidate.to_lowercase();
                !taken.contains(&key) && (owned.contains(&key) || !exists(candidate))
            })
            .expect("numbered names never run out");
        taken.insert(relative_path.to_lowercase());
        planned.push(PlannedFile {
            playlist_id: playlist.id.clone(),
            relative_path,
            tracks,
        });
    }
    planned
}

/// 設定と音楽フォルダに従って書き出す（設定がオフ・音楽フォルダが未設定・見つからない時は何もしない）
fn export_with_settings(app: &AppHandle) -> AppResult<Option<ExportResult>> {
    let settings = app.state::<SettingsState>().get()?;
    if !settings.export_playlists_to_music_folder {
        return Ok(None);
    }
    let state = app.state::<AppState>();
    let Some(folder) = state.with_db(|db| find_music_folder(db))? else {
        return Ok(None);
    };
    let music_folder = PathBuf::from(folder.path);
    if !music_folder.is_dir() {
        return Ok(None);
    }
    export_playlists(state.inner(), &music_folder).map(Some)
}

/// プレイリストの書き出しの依頼を受け、まとめて書き出す（Tauriの状態として管理する）
pub struct PlaylistExport {
    sender: Mutex<mpsc::Sender<()>>,
}

impl PlaylistExport {
    /// 書き出しのスレッドを起動し、ライブラリ・設定の変更で書き出すようにする
    ///
    /// 起動時にも1回書き出す（前回の終了後に変わったものと、設定をオンにした後の最初の書き出し）。
    pub fn start(app: &AppHandle) -> Self {
        let (sender, requests) = mpsc::channel::<()>();
        let worker_app = app.clone();
        let spawned = thread::Builder::new()
            .name("playlist-export".to_string())
            .spawn(move || {
                while requests.recv().is_ok() {
                    // 続けて届いた依頼は、まとめて1回にする
                    loop {
                        match requests.recv_timeout(DEBOUNCE) {
                            Ok(()) => continue,
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    match export_with_settings(&worker_app) {
                        Ok(Some(result)) if result.written > 0 || result.removed > 0 => {
                            log::info!(
                                "Exported playlists to the music folder (written: {}, removed: {})",
                                result.written,
                                result.removed
                            );
                        }
                        Ok(_) => {}
                        Err(e) => log::warn!("Failed to export playlists: {}", e),
                    }
                }
            });
        if let Err(e) = spawned {
            log::error!("Cannot start the playlist export thread: {}", e);
        }

        let export = Self {
            sender: Mutex::new(sender),
        };
        export.request();
        // 通知の中身は使わないため、名前だけで受け取る（型付きの受け取りは、中身を読める型が要る）
        // アプリはプラグインではないため、通知の名前は型の名前（`library-changed`など）のまま
        for name in [LibraryChanged::NAME, SettingsChanged::NAME] {
            let listener_app = app.clone();
            app.listen_any(name, move |_| {
                listener_app.state::<PlaylistExport>().request();
            });
        }
        export
    }

    /// 書き出しを依頼する（少し待ってから、まとめて書き出す）
    pub fn request(&self) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // スレッドが止まっている（起動できなかった）場合は、何もしない
        let _ = sender.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    fn insert_track(state: &AppState, id: &str, path: &str) {
        state
            .with_db(|db| {
                db.execute(
                    "INSERT INTO tracks (id, file_path, file_name, title, artist, format, file_size, duration)
                     VALUES (?1, ?2, ?3, ?1, 'Artist', 'mp3', 1, 60)",
                    rusqlite::params![id, path, format!("{id}.mp3")],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
    }

    fn create_playlist(state: &AppState, name: &str, track_ids: &[&str]) -> String {
        let ids: Vec<String> = track_ids.iter().map(|id| id.to_string()).collect();
        state
            .with_db(|db| crate::playlist::create_playlist_with_tracks(db, name, &ids))
            .unwrap()
            .id
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// 音楽フォルダの中の曲は相対パス、外の曲は絶対パスで書き、自動プレイリストは書かない
    #[test]
    fn test_exports_manual_playlists_with_relative_and_absolute_paths() {
        let dir = TempDir::new();
        let music = dir.path().join("Muspice");
        fs::create_dir_all(&music).unwrap();
        let state = AppState::new(crate::db::open_in_memory());
        let inside = music.join("Artist/Album/01 a.mp3");
        insert_track(&state, "a", inside.to_str().unwrap());
        insert_track(&state, "b", "/Users/me/Downloads/b.mp3");
        create_playlist(&state, "通勤: 朝", &["a", "b"]);
        let rules = crate::smart_playlist::SmartRules::unreadable();
        state
            .with_db(|db| crate::playlist::create_smart_playlist(db, "自動", &rules))
            .unwrap();

        let result = export_playlists(&state, &music).unwrap();

        assert_eq!(result.written, 1);
        // ファイル名に使えない文字は`_`にする（デバイスへの転送と同じ決まり）
        let file = music.join("Playlists/通勤_ 朝.m3u8");
        let content = read(&file);
        assert!(
            content.contains("\r\n../Artist/Album/01 a.mp3\r\n"),
            "{content}"
        );
        assert!(
            content.contains("\r\n/Users/me/Downloads/b.mp3\r\n"),
            "{content}"
        );
        assert!(!music.join("Playlists/自動.m3u8").exists());
        assert!(music.join("Playlists").join(MANIFEST_FILE_NAME).exists());

        // 変わっていなければ、書かない
        assert_eq!(
            export_playlists(&state, &music).unwrap(),
            ExportResult::default()
        );
    }

    /// 名前の変更・フォルダへの移動・削除では、前に書いたファイルを消す。利用者が置いたファイルは残す
    #[test]
    fn test_renames_moves_and_removes_only_exported_files() {
        let dir = TempDir::new();
        let music = dir.path().join("Muspice");
        let playlists_dir = music.join(PLAYLISTS_FOLDER_NAME);
        fs::create_dir_all(&playlists_dir).unwrap();
        // 利用者が置いたファイル（同じ名前のプレイリストは、番号を付けた名前で書く）
        fs::write(playlists_dir.join("Drive.m3u8"), "#EXTM3U\r\n").unwrap();
        let state = AppState::new(crate::db::open_in_memory());
        insert_track(&state, "a", music.join("a.mp3").to_str().unwrap());
        let id = create_playlist(&state, "Drive", &["a"]);

        export_playlists(&state, &music).unwrap();
        assert!(playlists_dir.join("Drive (2).m3u8").exists());
        assert_eq!(read(&playlists_dir.join("Drive.m3u8")), "#EXTM3U\r\n");

        // フォルダへ移すと、フォルダの中に書き、前のファイルは消す
        let folder = state
            .with_db(|db| crate::playlist::create_playlist_folder(db, "Car"))
            .unwrap();
        state
            .with_db(|db| crate::playlist::move_playlist(db, &id, Some(&folder.id)))
            .unwrap();
        let result = export_playlists(&state, &music).unwrap();
        assert_eq!(
            result,
            ExportResult {
                written: 1,
                removed: 1
            }
        );
        assert!(playlists_dir.join("Car/Drive.m3u8").exists());
        assert!(!playlists_dir.join("Drive (2).m3u8").exists());

        // 消すと、書いたファイルとフォルダを消す（利用者のファイルと`Playlists`は残す）
        state
            .with_db(|db| crate::playlist::delete_playlist(db, &id))
            .unwrap();
        let result = export_playlists(&state, &music).unwrap();
        assert_eq!(result.removed, 1);
        assert!(!playlists_dir.join("Car").exists());
        assert!(playlists_dir.join("Drive.m3u8").exists());
    }

    /// 受け取る通知の名前が、フロントエンドの通知の名前と同じこと
    #[test]
    fn test_event_names_match_the_frontend() {
        assert_eq!(LibraryChanged::NAME, "library-changed");
        assert_eq!(SettingsChanged::NAME, "settings-changed");
    }

    /// 書き出すものがなければ、`Playlists`を作らない
    #[test]
    fn test_nothing_to_export_creates_nothing() {
        let dir = TempDir::new();
        let state = AppState::new(crate::db::open_in_memory());
        let result = export_playlists(&state, dir.path()).unwrap();
        assert_eq!(result, ExportResult::default());
        assert!(!dir.path().join(PLAYLISTS_FOLDER_NAME).exists());
    }
}
