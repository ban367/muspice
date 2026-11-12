//! プレイリストの一括書き出し（ADR-087）
//!
//! 通常のプレイリストを、利用者が選んだフォルダへ、まとめてM3U8で書き出す（手動。自動プレイリストは、
//! 条件で曲が変わるため書き出さない）。プレイリストのフォルダは、選んだフォルダの中のフォルダにする。
//! 書き出したファイルは、ほかのアプリ・機器で使うほか、音楽フォルダを変えた（ライブラリを置き換えた。
//! ADR-086）後に、M3Uの読み込み（ADR-032）で取り込み直せる。
//!
//! - 名前はファイル名に使える形にする（デバイスへの転送と同じ決まり）。同じ場所になるプレイリストは、
//!   番号を付けた名前にする（大文字と小文字は区別しない。macOSのファイルシステムの既定に合わせる）
//! - 同じ名前のファイルがあれば、置き換える（同じフォルダへ書き出し直して、新しくできる）。ほかの
//!   ファイルは消さない
//! - 曲の場所は、1つずつの書き出し（`export_playlist_m3u`）と同じく、絶対パスか、書き出したファイルの
//!   フォルダからの相対パス

use crate::device_sync::sanitize_component;
use crate::error::{AppError, AppResult};
use crate::models::{Playlist, PlaylistFolder, Track};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// 書き出すファイルの拡張子
const EXTENSION: &str = "m3u8";

/// プレイリストをまとめて書き出した結果
#[derive(Debug, Default, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistsExportResult {
    /// 書き出したフォルダ
    pub folder: String,
    /// 書き出したプレイリスト（ファイル）の数
    pub playlist_count: u32,
}

/// 書き出すプレイリスト
struct PlannedFile {
    /// 選んだフォルダからの相対パス（区切りは`/`）
    relative_path: String,
    tracks: Vec<Track>,
}

/// 通常のプレイリストを、`dir`へまとめてM3U8で書き出す
///
/// `relative_paths`がtrueなら、曲の場所を書き出したファイルのフォルダからの相対パスで書く（falseは
/// 絶対パス）。書き出したプレイリストの数を返す（通常のプレイリストがなければ、何も書かずに0）。
pub fn export_all_playlists(state: &AppState, dir: &Path, relative_paths: bool) -> AppResult<u32> {
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

    let planned = plan_files(&playlists, &folders, tracks);
    for file in &planned {
        let path = dir.join(&file.relative_path);
        let base = path.parent().unwrap_or(dir).to_path_buf();
        let content =
            crate::m3u::build_m3u8(&file.tracks, relative_paths.then_some(base.as_path()));
        fs::create_dir_all(&base).map_err(|e| {
            AppError::Io(format!(
                "プレイリストの書き出し先（{}）を作れません: {}",
                base.display(),
                e
            ))
        })?;
        write_file(&path, content.as_bytes())?;
    }
    u32::try_from(planned.len())
        .map_err(|_| AppError::Validation("プレイリストの数が多すぎます".to_string()))
}

/// ファイルを、同じフォルダの一時ファイルに書いてから置き換える（書きかけのファイルを残さない）
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

/// 書き出すファイルの場所を決める
///
/// `<フォルダ名>/<プレイリスト名>.m3u8`（フォルダの外のプレイリストは、選んだフォルダの直下）。
/// 同じ場所になるプレイリストは、番号を付けた名前にする。
fn plan_files(
    playlists: &[Playlist],
    folders: &[PlaylistFolder],
    tracks: Vec<Vec<Track>>,
) -> Vec<PlannedFile> {
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
            .find(|candidate| !taken.contains(&candidate.to_lowercase()))
            .expect("numbered names never run out");
        taken.insert(relative_path.to_lowercase());
        planned.push(PlannedFile {
            relative_path,
            tracks,
        });
    }
    planned
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

    /// 通常のプレイリストを、フォルダの構成のまま書き、自動プレイリストは書かない
    #[test]
    fn test_exports_regular_playlists_with_folders() {
        let dir = TempDir::new();
        let state = AppState::new(crate::db::open_in_memory());
        insert_track(&state, "a", "/Users/me/Music/Artist/01 a.mp3");
        create_playlist(&state, "通勤: 朝", &["a"]);
        let id = create_playlist(&state, "Drive", &["a"]);
        state
            .with_db(|db| {
                let folder = crate::playlist::create_playlist_folder(db, "Car")?;
                crate::playlist::move_playlist(db, &id, Some(&folder.id))?;
                crate::playlist::create_smart_playlist(
                    db,
                    "自動",
                    &crate::smart_playlist::SmartRules::unreadable(),
                )
                .map(|_| ())
            })
            .unwrap();

        let count = export_all_playlists(&state, dir.path(), false).unwrap();

        assert_eq!(count, 2);
        // ファイル名に使えない文字は`_`にする（デバイスへの転送と同じ決まり）
        let content = read(&dir.path().join("通勤_ 朝.m3u8"));
        assert!(
            content.contains("\r\n/Users/me/Music/Artist/01 a.mp3\r\n"),
            "{content}"
        );
        assert!(dir.path().join("Car/Drive.m3u8").is_file());
        assert!(!dir.path().join("自動.m3u8").exists());
    }

    /// 相対パスでは、書き出したファイルのフォルダからの場所を書く
    #[test]
    fn test_relative_paths_start_from_the_file_folder() {
        let dir = TempDir::new();
        let state = AppState::new(crate::db::open_in_memory());
        let music = dir.path().join("Music/Artist/01 a.mp3");
        insert_track(&state, "a", music.to_str().unwrap());
        let id = create_playlist(&state, "Drive", &["a"]);
        state
            .with_db(|db| {
                let folder = crate::playlist::create_playlist_folder(db, "Car")?;
                crate::playlist::move_playlist(db, &id, Some(&folder.id))
            })
            .unwrap();
        let playlists = dir.path().join("Playlists");

        export_all_playlists(&state, &playlists, true).unwrap();

        let content = read(&playlists.join("Car/Drive.m3u8"));
        assert!(
            content.contains("\r\n../../Music/Artist/01 a.mp3\r\n"),
            "{content}"
        );
    }

    /// 同じ場所になるプレイリストは番号を付け、前からある同じ名前のファイルは置き換える（ほかは残す）
    #[test]
    fn test_numbers_same_names_and_replaces_existing_files() {
        let dir = TempDir::new();
        fs::write(dir.path().join("Drive.m3u8"), "old").unwrap();
        fs::write(dir.path().join("memo.txt"), "memo").unwrap();
        let state = AppState::new(crate::db::open_in_memory());
        create_playlist(&state, "Drive", &[]);
        create_playlist(&state, "Drive", &[]);

        assert_eq!(export_all_playlists(&state, dir.path(), false).unwrap(), 2);

        assert_eq!(read(&dir.path().join("Drive.m3u8")), "#EXTM3U\r\n");
        assert!(dir.path().join("Drive (2).m3u8").is_file());
        assert_eq!(read(&dir.path().join("memo.txt")), "memo");
    }

    /// 大文字と小文字だけが違う名前も、同じ場所とみなして番号を付ける（macOSのファイルシステムの既定）
    #[test]
    fn test_names_differing_only_in_case_are_numbered() {
        let playlist = |id: &str, name: &str| Playlist {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            tracks: Vec::new(),
            rules: None,
            folder_id: None,
            position: 0,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let planned = plan_files(
            &[playlist("1", "Drive"), playlist("2", "drive")],
            &[],
            vec![Vec::new(), Vec::new()],
        );

        let paths: Vec<&str> = planned
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(paths, ["Drive.m3u8", "drive (2).m3u8"]);
    }

    /// 通常のプレイリストがなければ、何も書かない
    #[test]
    fn test_nothing_to_export_writes_nothing() {
        let dir = TempDir::new();
        let state = AppState::new(crate::db::open_in_memory());
        assert_eq!(export_all_playlists(&state, dir.path(), true).unwrap(), 0);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
