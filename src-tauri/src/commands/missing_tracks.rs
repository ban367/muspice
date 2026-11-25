//! 見つからない曲を、ファイルに結び付け直すコマンド（ADR-117）
//!
//! - 「ファイルの場所を指定」: 1曲について、Rust側のダイアログで利用者がファイルを選び、その曲に
//!   結び付ける（ミュージックの「ファイルが見つかりません → 場所を指定」にあたる）
//! - 「フォルダから探す」: 利用者が選んだフォルダの中から、見つからない曲と同じ曲のファイルを探して
//!   結び付ける。同じ曲かは、再スキャンでの移動・改名の判定（`track_relink`）と同じ規則で決める
//!
//! どちらも、トラックのID・お気に入り・再生回数・プレイリストへの登録は変えない。ファイルはWebViewから
//! 受け取らず、Rust側のダイアログで選ぶ（ADR-065）。

use super::import::create_track_from_file;
use super::library_folders::{disk_file_of, notify_library_changed};
use super::{file_dialog, pick_folder, run_blocking};
use crate::error::{AppError, AppResult};
use crate::library::{
    ScanExclusions, get_file_modified_at, is_supported_audio_file, scan_directory,
    supported_extensions,
};
use crate::library_folder::DiskFile;
use crate::models::Track;
use crate::settings::Language;
use crate::state::AppState;
use crate::track_relink::MissingTrackIndex;
use crate::validation::validate_track_id;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// ダイアログの文言（アプリの設定の言語に合わせる）
struct Labels {
    locate_title: &'static str,
    audio_filter: &'static str,
    search_title: &'static str,
}

impl Labels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                locate_title: "曲のファイルを選ぶ",
                audio_filter: "音楽ファイル",
                search_title: "見つからない曲を探すフォルダを選ぶ",
            },
            Language::En => Self {
                locate_title: "Choose the Track's File",
                audio_filter: "Audio Files",
                search_title: "Choose a Folder to Search for Missing Tracks",
            },
        }
    }
}

/// フォルダから見つからない曲を探した結果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissingTrackSearchResult {
    /// 結び付けた曲の数
    pub relinked_count: u32,
    /// まだ見つからない曲の数
    pub remaining_count: u32,
}

/// 見つからない曲のファイルの場所を、ファイルを選ぶダイアログで指定する（ADR-117）
///
/// ダイアログは、曲の元の場所に近い、今あるフォルダから開く。結び付けた曲を返す（選ばずに閉じた
/// 場合はNone）。見つかっている曲・選んだファイルがほかの曲として登録済みの場合は`VALIDATION`。
#[tauri::command]
#[specta::specta]
pub async fn locate_track_file(track_id: String, app: AppHandle) -> AppResult<Option<Track>> {
    validate_track_id(&track_id)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        let track = state.with_db(|db| crate::repository::find_track_by_id(db, &track_id))?;
        if !track.is_missing {
            return Err(AppError::Validation(
                "この曲のファイルは見つかっています".to_string(),
            ));
        }
        // CUEシートの曲は、1つのファイルに結び付けられない（ADR-123）
        if track.cue.is_some() {
            return Err(AppError::Validation(
                "CUEシートの曲は、ファイルの場所を指定できません。「フォルダから探す」か、CUEシートのあるフォルダの再スキャンで結び付け直してください".to_string(),
            ));
        }

        let labels = Labels::for_language(crate::menu::current_language(&app));
        let mut dialog = file_dialog(&app)
            .set_title(labels.locate_title)
            .add_filter(labels.audio_filter, supported_extensions());
        if let Some(folder) = nearest_existing_folder(Path::new(&track.file_path)) {
            dialog = dialog.set_directory(folder);
        }
        let Some(file) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|e| AppError::Io(format!("選んだファイルを扱えません: {}", e)))?;

        let relinked = {
            // 再スキャンの途中（見つからない曲にしてから、移動の先に結び付けるまでの間）と重ねない
            let _scan = state.lock_library_scan();
            relink_to_file(&state, &track_id, &path)?
        };
        notify_library_changed(&app);
        Ok(Some(relinked))
    })
    .await
}

/// フォルダを選ぶダイアログで選んだフォルダの中から、見つからない曲のファイルを探して結び付ける（ADR-117）
///
/// フォルダを選ばずに閉じた場合はNone。
#[tauri::command]
#[specta::specta]
pub async fn find_missing_tracks_in_folder(
    app: AppHandle,
) -> AppResult<Option<MissingTrackSearchResult>> {
    run_blocking(move || {
        let labels = Labels::for_language(crate::menu::current_language(&app));
        let Some(folder) = pick_folder(file_dialog(&app).set_title(labels.search_title))? else {
            return Ok(None);
        };
        let state = app.state::<AppState>();
        let result = {
            let _scan = state.lock_library_scan();
            search_folder(&state, &folder)?
        };
        log::info!(
            "Searched a folder for missing tracks: relinked={}, remaining={}",
            result.relinked_count,
            result.remaining_count
        );
        if result.relinked_count > 0 {
            notify_library_changed(&app);
        }
        Ok(Some(result))
    })
    .await
}

/// 元の場所に近い、今あるフォルダ（ファイルを選ぶダイアログを開く場所。なければNone）
fn nearest_existing_folder(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|folder| folder.is_dir())
        .map(Path::to_path_buf)
}

/// 見つからない曲を、選んだファイルに結び付ける
fn relink_to_file(state: &AppState, track_id: &str, path: &Path) -> AppResult<Track> {
    if !path.is_file() || !is_supported_audio_file(path) {
        return Err(AppError::Validation(
            "選んだファイルは、対応している音楽ファイルではありません".to_string(),
        ));
    }
    let file = create_track_from_file(path)?;
    let modified_at = get_file_modified_at(path);

    state.with_db(|db| {
        // ダイアログを開いている間に、取り込みなどで見つかる曲に戻っていたら、結び付けを変えない
        if !crate::repository::find_track_by_id(db, track_id)?.is_missing {
            return Err(AppError::Validation(
                "この曲のファイルは、ファイルを選んでいる間に見つかりました".to_string(),
            ));
        }
        if is_registered_to_other(db, &file.file_path, track_id)? {
            return Err(AppError::Validation(
                "選んだファイルは、ほかの曲としてライブラリにあります".to_string(),
            ));
        }
        crate::repository::relink_track_to_file(db, track_id, &file, modified_at)?;
        crate::repository::find_track_by_id(db, track_id)
    })
}

/// ファイルが、ほかの曲としてライブラリに登録されているか
fn is_registered_to_other(
    conn: &rusqlite::Connection,
    file_path: &str,
    track_id: &str,
) -> AppResult<bool> {
    let registered = crate::repository::find_tracks_by_file_paths(conn, &[file_path.to_string()])?;
    Ok(registered.iter().any(|other| other.id != track_id))
}

/// フォルダの中から、見つからない曲と同じ曲のファイルを探して結び付ける
///
/// ファイルの大きさが同じ見つからない曲がある場合だけタグを読む（数万曲のフォルダでも、ほとんどの
/// ファイルは開かない）。ほかの曲として登録済みのファイルは、候補にしない。
fn search_folder(state: &AppState, folder: &Path) -> AppResult<MissingTrackSearchResult> {
    let (missing, present) = state.with_db(|db| {
        Ok((
            crate::repository::find_missing_tracks(db)?,
            crate::repository::find_present_file_paths(db)?,
        ))
    })?;
    if missing.is_empty() {
        return Ok(MissingTrackSearchResult {
            relinked_count: 0,
            remaining_count: 0,
        });
    }

    let scanned = scan_directory(folder, &ScanExclusions::default())?;
    let mut index = MissingTrackIndex::new(missing);
    let mut found = Vec::new();
    for path in &scanned.audio_files {
        let Some(path_text) = path.to_str() else {
            continue;
        };
        if present.contains(path_text) {
            continue;
        }
        // CUEシートの曲（仮想のパス）は、音声ファイルの大きさとCUEシートの更新日時で比べる
        let Some(DiskFile {
            size: Some(size),
            modified_at,
            ..
        }) = disk_file_of(path)
        else {
            continue;
        };
        if !index.has_size(size) {
            continue;
        }
        // 読めないファイルは、候補にしない
        let Ok(file) = create_track_from_file(path) else {
            continue;
        };
        if let Some(track) = index.take_match(&file, modified_at) {
            found.push((track.id, file, modified_at));
        }
    }

    state.with_transaction(|tx| {
        // フォルダを読んでいる間に、取り込みなどで見つかる曲に戻った曲・ほかの曲として登録された
        // ファイルは、結び付けない
        let mut relinked = 0;
        for (track_id, file, modified_at) in &found {
            if !crate::repository::find_track_by_id(tx, track_id)?.is_missing
                || is_registered_to_other(tx, &file.file_path, track_id)?
            {
                continue;
            }
            crate::repository::relink_track(
                tx,
                track_id,
                &file.file_path,
                &file.file_name,
                *modified_at,
                file.cue.as_ref(),
            )?;
            relinked += 1;
        }
        Ok(MissingTrackSearchResult {
            relinked_count: relinked,
            remaining_count: crate::repository::count_missing_tracks(tx)?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::device_manifest::test_support::TempDir;
    use std::fs;

    /// テスト用の音楽ファイル（タグのない無音のWAV。大きさは`samples`で決まる）を作る
    fn write_wav(path: &Path, samples: u32) {
        let data_len = samples * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, data_len as usize));
        fs::write(path, bytes).unwrap();
    }

    fn state_with_missing_track(path: &Path) -> (AppState, String) {
        let state = AppState::new(crate::db::open_in_memory());
        let track = create_track_from_file(path).unwrap();
        state
            .with_db(|db| {
                crate::repository::insert_track(db, &track, get_file_modified_at(path))?;
                crate::repository::mark_track_missing(db, &track.id, "2026-11-20T00:00:00Z")?;
                Ok(())
            })
            .unwrap();
        (state, track.id)
    }

    #[test]
    fn test_nearest_existing_folder() {
        let dir = TempDir::new();
        let gone = dir.path().join("消えたフォルダ").join("曲.mp3");
        assert_eq!(
            nearest_existing_folder(&gone),
            Some(dir.path().to_path_buf())
        );
        assert_eq!(nearest_existing_folder(Path::new("曲.mp3")), None);
    }

    #[test]
    fn test_relink_to_file_keeps_the_track_and_reads_the_file_properties() {
        let dir = TempDir::new();
        let old = dir.path().join("old.wav");
        write_wav(&old, 800);
        let (state, track_id) = state_with_missing_track(&old);
        state
            .with_db(|db| crate::repository::set_track_rating(db, &track_id, 8))
            .unwrap();
        fs::remove_file(&old).unwrap();
        // 別の版のファイル（長さが違う）を選ぶ
        let new = dir.path().join("new.wav");
        write_wav(&new, 16000);

        let track = relink_to_file(&state, &track_id, &new).unwrap();
        assert_eq!(track.id, track_id);
        assert_eq!(track.file_path, new.to_str().unwrap());
        assert!(!track.is_missing);
        assert_eq!(track.rating, 8);
        // タグ（タイトル）はライブラリの値のまま、大きさ・長さはファイルから読む
        assert_eq!(track.title.as_deref(), Some("old"));
        assert_eq!(track.file_size, fs::metadata(&new).unwrap().len() as i64);
        assert_eq!(track.duration, Some(2));
        assert_eq!(track.details.bit_depth, Some(16));
        assert_eq!(track.details.channels, Some(1));
    }

    #[test]
    fn test_relink_to_file_rejects_registered_and_unsupported_files() {
        let dir = TempDir::new();
        let old = dir.path().join("old.wav");
        write_wav(&old, 800);
        let (state, track_id) = state_with_missing_track(&old);
        let other = dir.path().join("other.wav");
        write_wav(&other, 800);
        let other_track = create_track_from_file(&other).unwrap();
        state
            .with_db(|db| crate::repository::insert_track(db, &other_track, None))
            .unwrap();

        assert!(matches!(
            relink_to_file(&state, &track_id, &other),
            Err(AppError::Validation(_))
        ));
        let text = dir.path().join("memo.txt");
        fs::write(&text, "memo").unwrap();
        assert!(matches!(
            relink_to_file(&state, &track_id, &text),
            Err(AppError::Validation(_))
        ));
        // 元の場所のファイルが戻っていれば、同じ場所にも結び付けられる
        assert!(relink_to_file(&state, &track_id, &old).is_ok());
        // 見つかる曲に戻った曲は、結び付けを変えない（ファイルを選んでいる間に取り込みで戻った場合など）
        assert!(matches!(
            relink_to_file(&state, &track_id, &old),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn test_search_folder_relinks_moved_files() {
        let dir = TempDir::new();
        let old = dir.path().join("old").join("曲.wav");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        write_wav(&old, 800);
        let (state, track_id) = state_with_missing_track(&old);
        // 移した先（中のフォルダ。改名しても、更新日時と大きさで同じ曲と分かる）と、ほかの曲のファイル
        let moved: PathBuf = dir.path().join("new").join("中").join("曲（改名）.wav");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(&old, &moved).unwrap();
        write_wav(&dir.path().join("new").join("別.wav"), 1600);

        let result = search_folder(&state, &dir.path().join("new")).unwrap();
        assert_eq!(
            result,
            MissingTrackSearchResult {
                relinked_count: 1,
                remaining_count: 0
            }
        );
        let track = state
            .with_db(|db| crate::repository::find_track_by_id(db, &track_id))
            .unwrap();
        assert_eq!(track.file_path, moved.to_str().unwrap());
        assert!(!track.is_missing);

        // 見つからない曲がなければ、フォルダを読まない
        let again = search_folder(&state, Path::new("/nonexistent")).unwrap();
        assert_eq!(again.relinked_count, 0);
    }

    /// CUEシートの曲は、移した先のCUEシートの曲に結び付け、音声ファイルの場所も変える
    #[test]
    fn test_search_folder_relinks_moved_cue_tracks() {
        let dir = TempDir::new();
        let old = dir.path().join("old");
        fs::create_dir_all(&old).unwrap();
        write_wav(&old.join("Album.wav"), 8000);
        let cue_text = "FILE \"Album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:00:30\n";
        fs::write(old.join("Album.cue"), cue_text).unwrap();
        let virtual_path = crate::cue::virtual_path(&old.join("Album.cue"), 2);
        let (state, track_id) = state_with_missing_track(Path::new(&virtual_path));
        let moved = dir.path().join("new");
        fs::rename(&old, &moved).unwrap();

        let result = search_folder(&state, &moved).unwrap();
        assert_eq!(result.relinked_count, 1);
        let track = state
            .with_db(|db| crate::repository::find_track_by_id(db, &track_id))
            .unwrap();
        assert_eq!(
            track.file_path,
            crate::cue::virtual_path(&moved.join("Album.cue"), 2)
        );
        assert_eq!(
            track.cue.unwrap().source,
            moved.join("Album.wav").to_str().unwrap()
        );
    }
}
