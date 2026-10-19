//! 音楽ファイルのインポート関連コマンド
//!
//! 取り込み方は4つあり、どれも同じ処理（`import_audio_files`）で曲を登録する（ADR-050）。
//!
//! - フォルダを選ぶ（`import_folder`）: 中の曲をすべて登録し、フォルダをライブラリフォルダとして記録する
//! - ファイルを選ぶ（`import_files`）: 選んだ曲だけを登録する（フォルダは記録しない）
//! - ウィンドウへドロップする（`import_dropped_files`。macOS）: ファイルは曲だけ、フォルダはフォルダとして
//! - 「このアプリで開く」・Dockのアイコンへのドロップ（`opened_files`）: 曲だけを登録して、再生する

use super::{file_dialog, pick_folder, run_blocking};
use crate::error::{AppError, AppResult};
use crate::events::{ImportProgress, LibraryChanged};
use crate::library::{
    DuplicateAction, ImportResult, get_default_title, get_file_format, get_file_modified_at,
    get_file_size, is_supported_audio_file, scan_directory, supported_extensions, to_count,
};
use crate::library_sync::LibrarySync;
use crate::metadata::extract_all_file_info;
use crate::models::Track;
use crate::settings::Language;
use crate::state::AppState;
use crate::track_relink::MissingTrackIndex;
use chrono::Utc;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use uuid::Uuid;

/// フォルダを選ぶダイアログを開き、選んだフォルダの音楽ファイルをインポートする
///
/// ダイアログはRust側で開く（WebViewからパスを受け取らない。ADR-065）。中の曲をすべて登録し、
/// フォルダをライブラリフォルダとして記録する。
///
/// @returns インポートの結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn import_folder(
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<Option<ImportResult>> {
    // ダイアログ・スキャン・メタデータ抽出・バッチ書き込みはブロッキング処理用スレッドで行う
    run_blocking(move || {
        let title = match crate::menu::current_language(&app_handle) {
            Language::Ja => "インポートするフォルダを選択",
            Language::En => "Select a folder to import",
        };
        let Some(folder) = pick_folder(&app_handle, title)? else {
            return Ok(None);
        };
        import_paths(&app_handle, &[folder], duplicate_action).map(Some)
    })
    .await
}

/// ファイルを選ぶダイアログを開き、選んだ音楽ファイルをインポートする
///
/// ダイアログはRust側で開く（WebViewへパスを渡さない）。選んだ曲だけを登録し、ファイルのある
/// フォルダは、ライブラリフォルダとして記録しない（再スキャンの対象にならない）。
///
/// @returns インポートの結果（選ばずに閉じた場合はnull）
#[tauri::command]
#[specta::specta]
pub async fn import_files(
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<Option<ImportResult>> {
    run_blocking(move || {
        let english = crate::menu::current_language(&app_handle) == crate::settings::Language::En;
        let (title, filter) = if english {
            ("Select the music files to import", "Music files")
        } else {
            ("インポートする音楽ファイルを選択", "音楽ファイル")
        };
        let Some(files) = file_dialog(&app_handle)
            .set_title(title)
            .add_filter(filter, supported_extensions())
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        let paths: Vec<PathBuf> = files
            .into_iter()
            .filter_map(|file| file.into_path().ok())
            .collect();
        import_paths(&app_handle, &paths, duplicate_action).map(Some)
    })
    .await
}

/// ウィンドウへドロップされたファイル・フォルダをインポートする（macOS）
///
/// WebViewは、ドロップされたファイルのパスを知らない（HTML5のドロップでは得られず、Tauriの
/// ファイルドロップは曲のドラッグ&ドロップのために無効にしている。ADR-018）。WebViewからは
/// 「ドロップされた」ことと名前だけを受け取り、パスは、OSのドラッグ用のペーストボードから
/// Rust側で読む（`dropped_files`）。ファイルは曲だけを登録し、フォルダは`import_folder`と同じく
/// ライブラリフォルダとして記録する。登録済みのファイルはスキップする。
#[tauri::command]
#[specta::specta]
pub async fn import_dropped_files(
    names: Vec<String>,
    app_handle: AppHandle,
) -> AppResult<ImportResult> {
    run_blocking(move || {
        let paths = crate::dropped_files::take_dropped_paths(
            &app_handle,
            &app_handle.state::<crate::dropped_files::DroppedFiles>(),
            &names,
        )?;
        import_paths(&app_handle, &paths, DuplicateAction::Skip)
    })
    .await
}

/// 「このアプリで開く」などで開かれ、インポートが済んだ曲を受け取る（受け取った分は、手放す）
///
/// フロントエンドは、起動した時と、`TracksOpened`が届いた時に呼び、受け取った曲を再生する。
#[tauri::command]
#[specta::specta]
pub fn take_opened_files(
    opened: tauri::State<'_, crate::opened_files::OpenedFilesState>,
) -> crate::opened_files::OpenedFiles {
    opened.take()
}

/// ファイル・フォルダをインポートし、ライブラリの変更を知らせる（同期処理）
///
/// 音楽ファイルは曲だけを登録し、フォルダは中の曲を登録してライブラリフォルダとして記録する。
/// 音楽ファイルでもフォルダでもないものは、読み飛ばす。取り込めるものが1つもなければエラー。
pub(crate) fn import_paths(
    app_handle: &AppHandle,
    paths: &[PathBuf],
    duplicate_action: DuplicateAction,
) -> AppResult<ImportResult> {
    let targets = ImportTargets::from_paths(paths);
    if targets.is_empty() {
        return Err(AppError::Validation(
            "インポートできる音楽ファイルがありません".to_string(),
        ));
    }

    let state = app_handle.state::<AppState>();
    let result = import_targets(&state, &targets, duplicate_action, &|progress| {
        if let Err(e) = progress.emit(app_handle) {
            log::warn!("進捗イベントの送信に失敗しました: {}", e);
        }
    })?;

    // 記録したフォルダを監視する（監視が有効な場合）
    app_handle
        .state::<LibrarySync>()
        .refresh_watches(app_handle);
    // 曲の一覧と、設定ウィンドウのライブラリフォルダの一覧に反映させる
    if let Err(e) = LibraryChanged.emit(app_handle) {
        log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
    }
    Ok(result)
}

/// インポートする対象（音楽ファイルと、フォルダ）
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ImportTargets {
    /// 音楽ファイル（曲だけを登録する）
    pub files: Vec<PathBuf>,
    /// フォルダ（中の曲を登録し、ライブラリフォルダとして記録する）
    pub folders: Vec<PathBuf>,
}

impl ImportTargets {
    /// パスを、音楽ファイルとフォルダに分ける（どちらでもないもの・重複は除く）
    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let mut targets = Self::default();
        for path in paths {
            if path.is_dir() {
                if !targets.folders.contains(path) {
                    targets.folders.push(path.clone());
                }
            } else if path.is_file()
                && is_supported_audio_file(path)
                && !targets.files.contains(path)
            {
                targets.files.push(path.clone());
            }
        }
        targets
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }
}

/// 音楽ファイルとフォルダをインポートする（同期処理）
///
/// `on_progress`には、ファイルを1つ読むたびに進み具合を渡す（ファイルの分と、フォルダごとに、
/// 件数を数え直す）。
pub(super) fn import_targets(
    state: &AppState,
    targets: &ImportTargets,
    duplicate_action: DuplicateAction,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportResult> {
    // 自動の再スキャンと同じファイルを同時に登録しないよう、終わるまで待ってから始める
    let _scan = state.lock_library_scan();
    let mut result = ImportResult::default();
    if !targets.files.is_empty() {
        result.merge(import_audio_files(
            &targets.files,
            duplicate_action,
            state,
            on_progress,
        )?);
    }
    for folder in &targets.folders {
        result.merge(import_folder_blocking(
            folder,
            duplicate_action,
            state,
            on_progress,
        )?);
    }
    Ok(result)
}

/// フォルダの中の音楽ファイルをインポートし、フォルダをライブラリフォルダとして記録する（同期処理）
///
/// 読み込まないフォルダ（登録済みのフォルダの除外、新しく登録するフォルダなら直下の既知のアプリの
/// フォルダ）は、中を読まない（ADR-062）。
fn import_folder_blocking(
    path: &Path,
    duplicate_action: DuplicateAction,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportResult> {
    // 既定の除外はファイルシステムを読むため、DBロックの外で求める
    let exclusions = match path.to_str() {
        Some(folder_path) => {
            let defaults = crate::library_folder::default_exclusions(path);
            state.with_db(|db| {
                crate::library_folder::import_exclusions(db, folder_path, &defaults)
            })?
        }
        None => crate::library_folder::ImportExclusions::default(),
    };

    // ディレクトリをスキャン
    let scanned = scan_directory(path, &exclusions.scan)?;
    let audio_files = &scanned.audio_files;
    let mut result = import_audio_files(audio_files, duplicate_action, state, on_progress)?;
    // 読めなかったフォルダは、取り込めなかったものとして結果に出す
    let unreadable = scanned.unreadable_dir_errors();
    result.error_count += to_count(unreadable.len())?;
    result.errors.extend(unreadable);

    // インポートしたフォルダをライブラリフォルダとして記録する（再スキャンの対象になる）
    // 音楽ファイルが見つからなかったフォルダは記録しない（中のライブラリフォルダの記録を
    // まとめて消さないため）。記録に失敗してもインポート自体は成功しているため、結果は返す
    if !audio_files.is_empty()
        && let Some(folder_path) = path.to_str()
        && let Err(e) = state.with_db(|db| {
            crate::library_folder::register_folder(db, folder_path, &exclusions.to_register)
        })
    {
        log::error!("ライブラリフォルダの記録に失敗しました: {}", e);
    }

    Ok(result)
}

/// 音楽ファイルの一覧を、ライブラリへ登録する（同期処理。フォルダ・ファイルのインポートで共通）
fn import_audio_files(
    audio_files: &[PathBuf],
    duplicate_action: DuplicateAction,
    state: &AppState,
    on_progress: &dyn Fn(ImportProgress),
) -> AppResult<ImportResult> {
    let mut imported_count = 0;
    let mut skipped_count = 0;
    let mut relinked_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    // バッチサイズ（一度にコミットするファイル数）
    const BATCH_SIZE: usize = 50;
    // 進捗イベントの件数はu32で送る（TypeScript側でnumberとして扱うため）
    let total_files = u32::try_from(audio_files.len())
        .map_err(|_| AppError::Validation("ファイル数が多すぎます".to_string()))?;
    let mut processed_count: u32 = 0;

    // 登録済みファイルパスを一度だけ取得する（ファイルごとの重複クエリを避ける）
    //
    // 処理済みのパスも随時追加していくため、同一インポート内で同じパスが
    // 複数回現れた場合も2回目以降は重複として扱われる（file_pathはUNIQUE）。
    // 見つからない曲は、新しいファイルの対応付けの候補にする（ライブラリフォルダの外へ移動した
    // ファイルをインポートした場合などに、新しい曲として登録せず、同じ曲として引き継ぐ）
    let (mut known_paths, mut missing_tracks) = state.with_db(|db| {
        Ok((
            crate::repository::find_all_file_paths(db)?,
            MissingTrackIndex::new(crate::repository::find_missing_tracks(db)?),
        ))
    })?;

    // バッチ処理でインポート
    //
    // ファイル読み取り（メタデータ抽出）はDBロックの外で行い、
    // ロックはバッチ単位の書き込みの間だけ保持する。
    // これにより大量インポート中も再生などの他操作がブロックされない。
    for chunk in audio_files.chunks(BATCH_SIZE) {
        // 1. ロック外: ファイルからトラック情報を抽出する
        // （トラック, 書き込み方, ファイルの更新日時）
        let mut pending: Vec<(Track, ImportAction, Option<i64>)> = Vec::new();
        // スキップした登録済みのファイルのパス（見つからない曲にしていた場合は、見つかる曲に戻す）
        let mut found_paths: Vec<String> = Vec::new();

        for file_path in chunk {
            let file_path_str = file_path
                .to_str()
                .ok_or_else(|| AppError::Io("ファイルパスの変換に失敗しました".to_string()))?;

            // 進捗イベントを送信
            processed_count += 1;
            let current_file = file_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("不明なファイル")
                .to_string();

            on_progress(ImportProgress {
                current: processed_count,
                total: total_files,
                current_file,
            });

            // 重複チェック
            // insertは未登録のパスの場合にtrueを返すため、falseなら重複
            // （DBに存在するか、このインポートで既に処理済みのいずれか）
            let is_duplicate = !known_paths.insert(file_path_str.to_string());
            if is_duplicate && matches!(duplicate_action, DuplicateAction::Skip) {
                skipped_count += 1;
                found_paths.push(file_path_str.to_string());
                continue;
            }

            match create_track_from_file(file_path) {
                Ok(track) => {
                    let modified_at = get_file_modified_at(file_path);
                    let action = if is_duplicate {
                        ImportAction::Replace
                    } else if let Some(moved) = missing_tracks.take_match(&track, modified_at) {
                        ImportAction::Relink(moved.id)
                    } else {
                        ImportAction::Insert
                    };
                    pending.push((track, action, modified_at));
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path_str, e));
                    error_count += 1;
                }
            }
        }

        if pending.is_empty() && found_paths.is_empty() {
            continue;
        }

        // 2. ロック内: バッチをまとめて書き込む
        state.with_db(|db| {
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;

            for path in &found_paths {
                crate::repository::restore_missing_track_by_file_path(&tx, path)?;
            }

            for (track, action, modified_at) in &pending {
                let result = match action {
                    // 既存のトラックを更新
                    ImportAction::Replace => {
                        crate::repository::update_track_by_file_path(&tx, track, *modified_at)
                    }
                    // 新しいトラックを追加
                    ImportAction::Insert => {
                        crate::repository::insert_track(&tx, track, *modified_at)
                    }
                    // 見つからない曲を、このファイルに結び付ける
                    ImportAction::Relink(track_id) => crate::repository::relink_track(
                        &tx,
                        track_id,
                        &track.file_path,
                        &track.file_name,
                        *modified_at,
                    ),
                };

                match (result, action) {
                    (Ok(()), ImportAction::Relink(_)) => relinked_count += 1,
                    (Ok(()), _) => imported_count += 1,
                    (Err(e), _) => {
                        errors.push(format!("{}: {}", track.file_path, e));
                        error_count += 1;
                    }
                }
            }

            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })
        })?;

        // 進行状況をログ出力
        log::info!(
            "インポート進行状況: {}/{} ファイル処理完了",
            processed_count,
            total_files
        );
    }

    // 同じパスのファイルが差し替わった可能性があるため、アルバムアートを読み直させる
    if let Ok(mut cache) = state.album_art_cache.lock() {
        cache.clear();
    }

    Ok(ImportResult {
        imported_count,
        skipped_count,
        relinked_count,
        error_count,
        errors,
    })
}

/// 読み込んだファイルの書き込み方
enum ImportAction {
    /// 新しいトラックとして追加する
    Insert,
    /// 登録済みの同じパスのトラックを、読み直した内容で置き換える
    Replace,
    /// 見つからない曲（このID）を、移動・改名の先のこのファイルに結び付ける
    Relink(String),
}

/// ファイルからトラック情報を作成（再スキャン・「メタデータを更新」でも使う）
///
/// 評価はファイルのタグから読む。お気に入り・再生回数は初期値（更新時は使われない）。
pub(super) fn create_track_from_file(file_path: &Path) -> AppResult<Track> {
    let file_name = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Io("ファイル名の取得に失敗しました".to_string()))?
        .to_string();

    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| AppError::Io("ファイルパスの変換に失敗しました".to_string()))?
        .to_string();

    // 1回のファイルオープンで全情報を一括抽出
    let file_info = extract_all_file_info(file_path)?;
    let sort_tags = file_info.metadata.sort_tags();

    // タイトルがない場合はファイル名をデフォルトとして使用
    let title = file_info
        .metadata
        .title
        .or_else(|| Some(get_default_title(file_path)));

    let file_size = get_file_size(file_path)?;
    let format = get_file_format(file_path);

    let now = Utc::now().to_rfc3339();

    Ok(Track {
        id: Uuid::new_v4().to_string(),
        file_path: file_path_str,
        file_name,
        title,
        artist: file_info.metadata.artist,
        album: file_info.metadata.album,
        album_artist: file_info.metadata.album_artist,
        genre: file_info.metadata.genre,
        year: file_info.metadata.year,
        track_number: file_info.metadata.track_number,
        disc_number: file_info.metadata.disc_number,
        duration: file_info.duration,
        file_size,
        format,
        bitrate: file_info.bitrate,
        sample_rate: file_info.sample_rate,
        is_favorite: false,
        rating: file_info.rating,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        created_at: now.clone(),
        updated_at: now,
        replay_gain: file_info.replay_gain,
        sort_tags,
        is_missing: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::write_wav;
    use std::cell::RefCell;

    fn test_state() -> AppState {
        let conn = crate::db::open_in_memory();
        AppState::new(conn)
    }

    /// 音楽ファイル（WAV）を2つ、フォルダ（中にWAVが1つ）を1つ、音楽ではないファイルを1つ作る
    fn fixture() -> (PathBuf, [PathBuf; 4]) {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", Uuid::new_v4()));
        let album = dir.join("album");
        std::fs::create_dir_all(&album).unwrap();
        let first = dir.join("first.wav");
        let second = dir.join("second.WAV");
        let cover = dir.join("cover.jpg");
        write_wav(&first);
        write_wav(&second);
        write_wav(&album.join("inside.wav"));
        std::fs::write(&cover, b"not music").unwrap();
        (dir, [first, second, album, cover])
    }

    fn track_paths(state: &AppState) -> Vec<String> {
        let mut paths: Vec<String> = state
            .with_db(|db| crate::repository::find_all_tracks(db))
            .unwrap()
            .into_iter()
            .map(|track| track.file_path)
            .collect();
        paths.sort();
        paths
    }

    fn folder_paths(state: &AppState) -> Vec<String> {
        state
            .with_db(|db| crate::library_folder::find_all_folders(db))
            .unwrap()
            .into_iter()
            .map(|folder| folder.path)
            .collect()
    }

    #[test]
    fn test_targets_are_split_into_audio_files_and_folders() {
        let (dir, [first, second, album, cover]) = fixture();

        let targets = ImportTargets::from_paths(&[
            first.clone(),
            cover.clone(),
            album.clone(),
            second.clone(),
            first.clone(),
            dir.join("missing.wav"),
        ]);
        // 音楽ではないファイル・ないファイル・重複は除く（拡張子の大文字と小文字は区別しない）
        assert_eq!(targets.files, vec![first, second]);
        assert_eq!(targets.folders, vec![album]);
        assert!(!targets.is_empty());
        assert!(ImportTargets::from_paths(&[cover, dir.join("missing.wav")]).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ファイルを選んだインポートは、曲だけを登録する（フォルダは、ライブラリフォルダにしない）
    #[test]
    fn test_importing_files_registers_only_the_tracks() {
        let (dir, [first, second, _album, cover]) = fixture();
        let state = test_state();
        let progress = RefCell::new(Vec::new());
        let targets = ImportTargets::from_paths(&[first.clone(), cover, second.clone()]);

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|event| {
            progress
                .borrow_mut()
                .push((event.current, event.total, event.current_file));
        })
        .unwrap();

        assert_eq!((result.imported_count, result.skipped_count), (2, 0));
        assert_eq!(
            track_paths(&state),
            [first.to_str().unwrap(), second.to_str().unwrap()]
        );
        // 同じフォルダにある、選んでいない曲（album/inside.wav）は登録しない
        assert!(folder_paths(&state).is_empty());
        assert_eq!(
            *progress.borrow(),
            [
                (1, 2, "first.wav".to_string()),
                (2, 2, "second.WAV".to_string())
            ]
        );

        // もう一度インポートすると、登録済みとしてスキップする
        let again = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();
        assert_eq!((again.imported_count, again.skipped_count), (0, 2));
        // 「このアプリで開く」では、開いた順に曲を受け取る
        let opened = state
            .with_db(|db| {
                crate::repository::find_tracks_by_file_paths(
                    db,
                    &[
                        second.to_str().unwrap().to_string(),
                        "/no/such/file.wav".to_string(),
                        first.to_str().unwrap().to_string(),
                    ],
                )
            })
            .unwrap();
        assert_eq!(
            opened
                .iter()
                .map(|track| track.file_name.as_str())
                .collect::<Vec<_>>(),
            ["second.WAV", "first.wav"]
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// フォルダを選んだインポート（`import_folder`）は、中の曲を登録し、フォルダを記録する
    #[test]
    fn test_importing_a_folder_registers_its_tracks_and_the_folder() {
        let (dir, [_first, _second, album, _cover]) = fixture();
        let state = test_state();
        let targets = ImportTargets::from_paths(std::slice::from_ref(&album));

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!(result.imported_count, 1);
        assert_eq!(
            track_paths(&state),
            [album.join("inside.wav").to_str().unwrap()]
        );
        assert_eq!(folder_paths(&state), [album.to_str().unwrap()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ファイルとフォルダを一緒に渡すと、フォルダの方はライブラリフォルダとして記録する
    #[test]
    fn test_importing_a_folder_with_files_registers_the_folder() {
        let (dir, [first, _second, album, _cover]) = fixture();
        let state = test_state();
        let targets = ImportTargets::from_paths(&[first, album.clone()]);

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!(result.imported_count, 2);
        assert_eq!(track_paths(&state).len(), 2);
        assert_eq!(folder_paths(&state), [album.to_str().unwrap()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// フォルダのインポートは、読み飛ばす場所を読まず、直下の既知のアプリのフォルダを、
    /// 読み込まないフォルダとして記録する
    #[test]
    fn test_importing_a_folder_skips_hidden_package_and_app_folders() {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", Uuid::new_v4()));
        let music = dir.join("Music");
        for path in [
            "Artist/Album/01.wav",
            "Artist/Album/._01.wav",
            ".Trashes/501/a.wav",
            "Projects/Song.band/Media/b.wav",
            "Music/Media.localized/Music/c.wav",
            "MusicBee/Podcasts/Show/d.wav",
        ] {
            let path = music.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            write_wav(&path);
        }
        let state = test_state();
        let targets = ImportTargets::from_paths(std::slice::from_ref(&music));

        let result = import_targets(&state, &targets, DuplicateAction::Skip, &|_| {}).unwrap();

        assert_eq!((result.imported_count, result.error_count), (1, 0));
        assert_eq!(
            track_paths(&state),
            [music.join("Artist/Album/01.wav").to_str().unwrap()]
        );
        assert_eq!(folder_paths(&state), [music.to_str().unwrap()]);
        let exclusions: Vec<String> = state
            .with_db(|db| {
                let folder = crate::library_folder::find_all_folders(db)?.remove(0);
                crate::library_folder::find_exclusions(db, &folder.id)
            })
            .unwrap()
            .into_iter()
            .map(|exclusion| exclusion.path)
            .collect();
        assert_eq!(exclusions, ["Music", "MusicBee"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
