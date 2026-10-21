//! 音楽フォルダ（新しい曲を置く宛先）の設定のコマンド
//!
//! 音楽フォルダは、ライブラリフォルダの1つに付けた印（`library_folders.is_music_folder`。ADR-068）。
//! 選んだフォルダは、空でもライブラリフォルダとして登録し、曲があれば読み込む。変えた時は印を移すだけで、
//! ファイルは動かさない。音楽フォルダの書式は、設定（`Settings::music_folder_template`）に持つ。

use super::import::{FolderRegistration, import_folder_blocking};
use super::library_folders::notify_library_changed;
use super::{pick_folder, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::ImportProgress;
use crate::file_organize::Template;
use crate::library::{DuplicateAction, ImportResult};
use crate::library_folder::{
    find_enclosing_folder, find_folder_by_path, find_music_folder, normalize_folder_path,
};
use crate::library_sync::LibrarySync;
use crate::models::Track;
use crate::settings::Language;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// 書式の例に出す曲の数
const TEMPLATE_EXAMPLE_COUNT: u32 = 5;

/// 音楽フォルダ（印を付けたライブラリフォルダ）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolder {
    /// ライブラリフォルダのID
    pub id: String,
    pub path: String,
    /// フォルダが今も存在するか（外付けドライブが外れている場合などはfalse）
    pub exists: bool,
}

/// 音楽フォルダの設定（画面に出す）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MusicFolderInfo {
    /// 音楽フォルダ（設定していなければnull）
    pub folder: Option<MusicFolder>,
    /// OSの音楽フォルダ（音楽フォルダの候補。見つからなければnull）
    pub system_folder: Option<String>,
}

/// 音楽フォルダにするフォルダの選び方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MusicFolderSource {
    /// OSの音楽フォルダ（macOSは`~/Music`。パスはRust側で求める）
    System,
    /// ダイアログで選ぶ
    Pick,
}

/// 音楽フォルダを選んだ結果
#[derive(Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MusicFolderChange {
    /// 音楽フォルダにした
    Set {
        path: String,
        /// 中の曲を読み込んだ結果（登録済みのライブラリフォルダに、印を移しただけの場合はnull）
        imported: Option<ImportResult>,
    },
    /// ライブラリフォルダの中のフォルダを選んだ（フォルダは入れ子にできないため、音楽フォルダには
    /// していない。画面は、外側のフォルダを案内する）
    InsideLibraryFolder {
        path: String,
        #[serde(rename = "libraryFolderPath")]
        library_folder_path: String,
    },
}

/// 音楽フォルダの設定と、OSの音楽フォルダ（候補）を取得
///
/// フォルダの存在確認は、外れたネットワークドライブなどで時間がかかることがあるため、
/// ブロッキング処理用スレッドで行う。
#[tauri::command]
#[specta::specta]
pub async fn get_music_folder(app_handle: AppHandle) -> AppResult<MusicFolderInfo> {
    run_blocking(move || {
        let record = app_handle
            .state::<AppState>()
            .with_db(|db| find_music_folder(db))?;
        Ok(MusicFolderInfo {
            folder: record.map(|record| MusicFolder {
                exists: Path::new(&record.path).is_dir(),
                id: record.id,
                path: record.path,
            }),
            system_folder: system_music_folder(&app_handle)
                .and_then(|path| path.to_str().map(str::to_string)),
        })
    })
    .await
}

/// 音楽フォルダを選ぶ
///
/// OSの音楽フォルダか、ダイアログ（Rust側で、呼び出したウィンドウの上に出す。ADR-065）で選んだ
/// フォルダを、音楽フォルダにする。
///
/// - 登録済みのライブラリフォルダなら、印を移すだけ（読み込み直さない）
/// - 登録済みのライブラリフォルダの中のフォルダなら、音楽フォルダにせず`InsideLibraryFolder`を返す
/// - それ以外は、空でもライブラリフォルダとして登録し、中の曲を読み込む（`ImportProgress`を送る）。
///   直下の既知のアプリのフォルダは、読み込まないフォルダに入れる（ADR-062）。中にあるライブラリ
///   フォルダの記録は、まとめる
///
/// ファイルは動かさない（前の音楽フォルダは、ライブラリフォルダとして残る）。
///
/// @returns 結果（ダイアログで選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn set_music_folder(
    source: MusicFolderSource,
    window: WebviewWindow,
    app_handle: AppHandle,
) -> AppResult<Option<MusicFolderChange>> {
    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let folder = match source {
            MusicFolderSource::System => system_music_folder(&app_handle).ok_or_else(|| {
                AppError::NotFound("OSの音楽フォルダが見つかりません".to_string())
            })?,
            MusicFolderSource::Pick => {
                let title = match crate::menu::current_language(&app_handle) {
                    Language::Ja => "音楽フォルダを選択",
                    Language::En => "Select a Music Folder",
                };
                // 今の音楽フォルダ（なければOSの音楽フォルダ）を開いた状態で出す
                let current = state
                    .with_db(|db| find_music_folder(db))?
                    .map(|folder| PathBuf::from(folder.path))
                    .filter(|path| path.is_dir());
                let mut dialog = app_handle
                    .dialog()
                    .file()
                    .set_parent(&window)
                    .set_title(title);
                if let Some(directory) = current.or_else(|| system_music_folder(&app_handle)) {
                    dialog = dialog.set_directory(directory);
                }
                let Some(folder) = pick_folder(dialog)? else {
                    return Ok(None);
                };
                folder
            }
        };

        let change = change_music_folder(state.inner(), &folder, &|progress| {
            if let Err(e) = progress.emit(&app_handle) {
                log::warn!("進捗イベントの送信に失敗しました: {}", e);
            }
        })?;
        if matches!(change, MusicFolderChange::Set { .. }) {
            // 新しく登録したフォルダを監視する（監視が有効な場合）
            app_handle
                .state::<LibrarySync>()
                .refresh_watches(&app_handle);
            // 曲の一覧と、両方のウィンドウの音楽フォルダの表示に反映させる
            notify_library_changed(&app_handle);
        }
        Ok(Some(change))
    })
    .await
}

/// 音楽フォルダの設定を外す（ライブラリフォルダの記録・ファイルは、そのまま）
#[tauri::command]
#[specta::specta]
pub async fn clear_music_folder(app_handle: AppHandle) -> AppResult<()> {
    app_handle
        .state::<AppState>()
        .with_db(|db| crate::library_folder::clear_music_folder(db))?;
    notify_library_changed(&app_handle);
    Ok(())
}

/// 音楽フォルダの書式の例（最近追加したアルバムの最初の曲を、書式に当てはめた、音楽フォルダからの場所）
///
/// 書式が読めない場合は`VALIDATION`。ライブラリに曲がなければ空。
#[tauri::command]
#[specta::specta]
pub async fn preview_music_folder_template(
    template: String,
    app_handle: AppHandle,
) -> AppResult<Vec<String>> {
    let template = Template::parse(&template)?;
    run_blocking(move || {
        let tracks = app_handle.state::<AppState>().with_db(|db| {
            crate::repository::find_recent_album_tracks(db, TEMPLATE_EXAMPLE_COUNT)
        })?;
        Ok(template_examples(&template, &tracks))
    })
    .await
}

/// OSの音楽フォルダ（見つからなければNone）
fn system_music_folder(app: &AppHandle) -> Option<PathBuf> {
    app.path().audio_dir().ok().filter(|path| path.is_dir())
}

/// フォルダを音楽フォルダにする（同期処理。`set_music_folder`のダイアログの後の処理）
fn change_music_folder(
    state: &AppState,
    folder: &Path,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<MusicFolderChange> {
    let path = folder
        .to_str()
        .map(normalize_folder_path)
        .ok_or_else(|| AppError::Validation("フォルダの名前を扱えません".to_string()))?;

    // 自動の再スキャン・インポートと同じフォルダを同時に登録しないよう、終わるまで待ってから始める
    let _scan = state.lock_library_scan();
    let (registered, enclosing) = state.with_db(|db| {
        Ok((
            find_folder_by_path(db, &path)?,
            find_enclosing_folder(db, &path)?,
        ))
    })?;
    if let Some(registered) = registered {
        state.with_db(|db| crate::library_folder::set_music_folder(db, &registered.id))?;
        return Ok(MusicFolderChange::Set {
            path,
            imported: None,
        });
    }
    if let Some(enclosing) = enclosing {
        return Ok(MusicFolderChange::InsideLibraryFolder {
            path,
            library_folder_path: enclosing.path,
        });
    }

    let (imported, folder_id) = import_folder_blocking(
        Path::new(&path),
        DuplicateAction::Skip,
        state,
        on_progress,
        FolderRegistration::Always,
    )?;
    let folder_id = folder_id
        .ok_or_else(|| AppError::Database("音楽フォルダを登録できませんでした".to_string()))?;
    state.with_db(|db| crate::library_folder::set_music_folder(db, &folder_id))?;
    Ok(MusicFolderChange::Set {
        path,
        imported: Some(imported),
    })
}

/// 曲を書式に当てはめた、音楽フォルダからの場所（区切りは`/`）
fn template_examples(template: &Template, tracks: &[Track]) -> Vec<String> {
    tracks
        .iter()
        .map(|track| {
            template
                .components(track, Path::new(&track.file_path))
                .join("/")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::library_folder::{find_all_folders, find_exclusions, register_folder};
    use crate::metadata::test_images::write_wav;
    use std::fs;

    fn test_state() -> AppState {
        AppState::new(crate::db::open_in_memory())
    }

    fn change(state: &AppState, folder: &Path) -> MusicFolderChange {
        change_music_folder(state, folder, &|_| {}).unwrap()
    }

    fn music_folder_path(state: &AppState) -> Option<String> {
        state
            .with_db(|db| find_music_folder(db))
            .unwrap()
            .map(|folder| folder.path)
    }

    fn folder_paths(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| find_all_folders(db))
            .unwrap()
            .into_iter()
            .map(|folder| folder.path)
            .collect()
    }

    fn path_str(path: &Path) -> String {
        path.to_str().unwrap().to_string()
    }

    /// 空のフォルダも、ライブラリフォルダとして登録して音楽フォルダにする
    #[test]
    fn test_empty_folder_becomes_music_folder() {
        let dir = TempDir::new();
        let state = test_state();

        let result = change(&state, dir.path());

        assert_eq!(
            result,
            MusicFolderChange::Set {
                path: path_str(dir.path()),
                imported: Some(ImportResult::default()),
            }
        );
        assert_eq!(folder_paths(&state), [path_str(dir.path())]);
        assert_eq!(music_folder_path(&state), Some(path_str(dir.path())));
    }

    /// OSの音楽フォルダのような構成では、曲を読み込み、ほかのアプリのフォルダは除外の一覧に入れる
    #[test]
    fn test_music_folder_imports_tracks_and_records_app_folder_exclusions() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        for path in [
            "Artist/Album/01.wav",
            "Music/Media.localized/Music/a.wav",
            "MusicBee/Podcasts/b.wav",
            "GarageBand/c.wav",
        ] {
            let path = music.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            write_wav(&path);
        }
        fs::create_dir_all(music.join("Music/Music Library.musiclibrary")).unwrap();
        let state = test_state();
        let progress = std::cell::RefCell::new(Vec::new());

        let result = change_music_folder(&state, &music, &|event| {
            progress.borrow_mut().push(event.current_file);
        })
        .unwrap();

        let MusicFolderChange::Set {
            imported: Some(imported),
            ..
        } = result
        else {
            panic!("音楽フォルダにしていない: {:?}", result);
        };
        assert_eq!(imported.imported_count, 1);
        assert_eq!(*progress.borrow(), ["01.wav"]);
        assert_eq!(music_folder_path(&state), Some(path_str(&music)));
        let exclusions: Vec<String> = state
            .with_db(|db| {
                let folder = find_music_folder(db)?.unwrap();
                find_exclusions(db, &folder.id)
            })
            .unwrap()
            .into_iter()
            .map(|exclusion| exclusion.path)
            .collect();
        assert_eq!(exclusions, ["GarageBand", "Music", "MusicBee"]);
    }

    /// 登録済みのライブラリフォルダを選ぶと、印を移すだけで読み込み直さない
    #[test]
    fn test_registered_folder_only_moves_the_mark() {
        let dir = TempDir::new();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        // 登録した後に置いた曲は、印を移すだけでは読まない
        let state = test_state();
        change(&state, &first);
        state
            .with_db(|db| register_folder(db, second.to_str().unwrap(), &[]))
            .unwrap();
        write_wav(&second.join("later.wav"));

        let result = change(&state, &second);

        assert_eq!(
            result,
            MusicFolderChange::Set {
                path: path_str(&second),
                imported: None,
            }
        );
        assert_eq!(music_folder_path(&state), Some(path_str(&second)));
        // 前の音楽フォルダは、ライブラリフォルダとして残る
        assert_eq!(folder_paths(&state), [path_str(&first), path_str(&second)]);
        let tracks = state
            .with_db(|db| crate::repository::count_all_tracks(db))
            .unwrap();
        assert_eq!(tracks, 0);
    }

    /// ライブラリフォルダの中のフォルダは、音楽フォルダにしない（外側のフォルダを返す）
    #[test]
    fn test_folder_inside_library_folder_is_not_used() {
        let dir = TempDir::new();
        let inside = dir.path().join("Artist");
        fs::create_dir_all(&inside).unwrap();
        let state = test_state();
        state
            .with_db(|db| register_folder(db, dir.path().to_str().unwrap(), &[]))
            .unwrap();

        assert_eq!(
            change(&state, &inside),
            MusicFolderChange::InsideLibraryFolder {
                path: path_str(&inside),
                library_folder_path: path_str(dir.path()),
            }
        );
        assert_eq!(music_folder_path(&state), None);
        assert_eq!(folder_paths(&state), [path_str(dir.path())]);
    }

    /// ライブラリフォルダを含むフォルダを選ぶと、中の記録をまとめて音楽フォルダにする
    #[test]
    fn test_outer_folder_merges_library_folders() {
        let dir = TempDir::new();
        let inner = dir.path().join("Downloads");
        fs::create_dir_all(&inner).unwrap();
        write_wav(&inner.join("a.wav"));
        let state = test_state();
        state
            .with_db(|db| register_folder(db, inner.to_str().unwrap(), &[]))
            .unwrap();

        let result = change(&state, dir.path());

        let MusicFolderChange::Set {
            imported: Some(imported),
            ..
        } = result
        else {
            panic!("音楽フォルダにしていない: {:?}", result);
        };
        assert_eq!(imported.imported_count, 1);
        assert_eq!(folder_paths(&state), [path_str(dir.path())]);
        assert_eq!(music_folder_path(&state), Some(path_str(dir.path())));
    }

    #[test]
    fn test_template_examples_are_relative_paths() {
        let track = |file_path: &str, album_artist: Option<&str>, disc: Option<i32>| Track {
            id: "a1000000-0000-4000-8000-000000000001".to_string(),
            file_path: file_path.to_string(),
            file_name: file_path.rsplit('/').next().unwrap().to_string(),
            title: Some("Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: album_artist.map(str::to_string),
            genre: None,
            year: None,
            track_number: Some(3),
            disc_number: disc,
            duration: Some(200),
            file_size: 4,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: crate::models::ReplayGain::default(),
            sort_tags: Default::default(),
            is_missing: false,
        };
        let template = Template::parse(crate::settings::DEFAULT_MUSIC_FOLDER_TEMPLATE).unwrap();

        assert_eq!(
            template_examples(
                &template,
                &[
                    track("/m/a.flac", Some("Various"), Some(1)),
                    track("/m/b.mp3", None, None),
                ]
            ),
            ["Various/Album/1-03 Song.flac", "Artist/Album/03 Song.mp3"]
        );
    }
}
