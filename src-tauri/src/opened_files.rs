//! 「このアプリで開く」で開かれた音楽ファイルの受け取り
//!
//! Finderの「このアプリで開く」・Dockのアイコンへのドロップ（macOS）と、起動時の引数
//! （ファイルの関連付けからの起動。Windows・Linux）で渡されたファイルを、ライブラリへ
//! インポートしてから再生する（ADR-050）。
//!
//! 再生キューはフロントエンドが持つため、ここでは曲を登録して「再生する曲」として預かり、
//! フロントエンドへ知らせる（`TracksOpened`）。フロントエンドは`take_opened_files`で受け取って
//! 再生する。起動の途中に届いた分（フロントエンドがまだ聞いていない）も、預かっておけば、
//! 起動した後に受け取れる。
//!
//! アプリが起動していない状態でファイルを開くと、macOSは、起動の準備（`setup`。データベースを
//! 開くなど）が済む前にファイルを渡してくる。その分は`EARLY_PATHS`に取っておき、準備が済んで
//! から開く（`open_startup_paths`）。

use crate::commands::{ImportTargets, import_paths};
use crate::events::TracksOpened;
use crate::library::{DuplicateAction, is_supported_audio_file};
use crate::models::Track;
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// 開かれたファイルのうち、フロントエンドがまだ受け取っていないもの
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedFiles {
    /// 再生する曲（開かれた順）
    pub tracks: Vec<Track>,
    /// 開けなかったファイルの、エラーの内容
    pub errors: Vec<String>,
}

/// 起動の準備が済む前に開かれたファイル
static EARLY_PATHS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// 開かれたファイルの預かり場所
#[derive(Default)]
pub struct OpenedFilesState {
    pending: Mutex<OpenedFiles>,
}

impl OpenedFilesState {
    /// 預かっている分を取り出す（取り出した分は、手放す）
    pub fn take(&self) -> OpenedFiles {
        self.pending
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default()
    }

    fn add(&self, tracks: Vec<Track>, errors: Vec<String>) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.tracks.extend(tracks);
            pending.errors.extend(errors);
        }
    }
}

/// 起動の準備が済んだ時に、起動と一緒に開かれたファイルを開く
///
/// - 準備が済む前に届いていたファイル（macOS。`RunEvent::Opened`）
/// - 起動時の引数のファイル（ファイルの関連付けから起動された時、Windows・Linuxでは引数で渡される）
pub fn open_startup_paths(app: &AppHandle) {
    let mut paths = EARLY_PATHS
        .lock()
        .map(|mut early| std::mem::take(&mut *early))
        .unwrap_or_default();
    paths.extend(audio_file_arguments(
        std::env::args_os().skip(1).map(PathBuf::from),
    ));
    open_paths(app, paths);
}

/// 引数のうち、実際にある音楽ファイルだけを、絶対パスにして返す（オプションなどは読み飛ばす）
fn audio_file_arguments(arguments: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    arguments
        .filter(|path| path.is_file() && is_supported_audio_file(path))
        .filter_map(|path| std::path::absolute(path).ok())
        .collect()
}

/// 開かれたファイルをインポートし、再生する曲としてフロントエンドへ知らせる
///
/// インポートには時間がかかることがあるため、別のスレッドで行う。
pub fn open_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    // 起動の準備が済む前に届いた分は、取っておく（`open_startup_paths`が開く）
    if app.try_state::<OpenedFilesState>().is_none() {
        if let Ok(mut early) = EARLY_PATHS.lock() {
            early.extend(paths);
        }
        return;
    }
    if paths.is_empty() {
        return;
    }

    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("opened-files".to_string())
        .spawn(move || {
            let (tracks, errors) = import_and_find(&app, &paths);
            log::info!(
                "開かれたファイル: {}曲を再生します（エラー{}件）",
                tracks.len(),
                errors.len()
            );
            app.state::<OpenedFilesState>().add(tracks, errors);
            if let Err(e) = TracksOpened.emit(&app) {
                log::warn!("開かれたファイルの通知に失敗しました: {}", e);
            }
            // メニューバーに常駐して隠れている・最小化している場合も、ウィンドウを出す
            crate::tray::show_main_window(&app);
        });
    if let Err(e) = spawned {
        log::error!("開かれたファイルを処理できません: {}", e);
    }
}

/// 開かれたファイルのうち、音楽ファイルでもフォルダでもないものの、エラーの内容
fn unsupported_errors(paths: &[PathBuf], targets: &ImportTargets) -> Vec<String> {
    paths
        .iter()
        .filter(|path| !targets.files.contains(path) && !targets.folders.contains(path))
        .map(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            format!("{}: 対応している音楽ファイルではありません", name)
        })
        .collect()
}

/// 開かれたファイルをインポートし、その曲（開かれた順）と、エラーの内容を返す
fn import_and_find(app: &AppHandle, paths: &[PathBuf]) -> (Vec<Track>, Vec<String>) {
    let targets = ImportTargets::from_paths(paths);
    let mut errors = unsupported_errors(paths, &targets);
    if targets.is_empty() {
        return (Vec::new(), errors);
    }
    match import_paths(app, paths, DuplicateAction::Skip) {
        Ok(result) => errors.extend(result.errors),
        Err(e) => errors.push(e.to_string()),
    }

    // 登録済みだった曲も含めて、開かれたファイルの曲を再生する（フォルダの中の曲は、再生しない）
    let file_paths: Vec<String> = targets
        .files
        .iter()
        .filter_map(|path| path.to_str().map(str::to_string))
        .collect();
    let tracks = app
        .state::<AppState>()
        .with_db(|db| crate::repository::find_tracks_by_file_paths(db, &file_paths))
        .unwrap_or_else(|e| {
            log::error!("開かれたファイルの曲を取得できません: {}", e);
            Vec::new()
        });
    (tracks, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_returns_pending_files_once() {
        let state = OpenedFilesState::default();
        let is_empty = |files: OpenedFiles| files.tracks.is_empty() && files.errors.is_empty();
        assert!(is_empty(state.take()));

        state.add(Vec::new(), vec!["a.flac: 読めません".to_string()]);
        state.add(Vec::new(), vec!["b.flac: 読めません".to_string()]);
        let taken = state.take();
        assert_eq!(taken.errors.len(), 2);
        // 取り出した分は、残らない
        assert!(is_empty(state.take()));
    }

    /// 音楽ファイルでもフォルダでもないものは、開けなかったものとして知らせる
    #[test]
    fn test_unsupported_files_are_reported() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("song.flac");
        let notes = dir.join("notes.txt");
        std::fs::write(&song, b"x").unwrap();
        std::fs::write(&notes, b"x").unwrap();
        let paths = vec![song, notes, dir.clone(), dir.join("missing.mp3")];

        let errors = unsupported_errors(&paths, &ImportTargets::from_paths(&paths));
        assert_eq!(
            errors,
            [
                "notes.txt: 対応している音楽ファイルではありません",
                "missing.mp3: 対応している音楽ファイルではありません"
            ]
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_startup_arguments_keep_only_existing_audio_files() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("song.flac");
        let cover = dir.join("cover.jpg");
        std::fs::write(&song, b"x").unwrap();
        std::fs::write(&cover, b"x").unwrap();

        let arguments = vec![
            PathBuf::from("--some-option"),
            song.clone(),
            cover,
            dir.join("missing.mp3"),
            dir.clone(),
        ];
        assert_eq!(audio_file_arguments(arguments.into_iter()), vec![song]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
