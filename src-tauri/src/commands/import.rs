//! 音楽ファイルのインポート関連コマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::{ImportProgress, LibraryChanged};
use crate::library::{
    DuplicateAction, ImportResult, get_default_title, get_file_format, get_file_modified_at,
    get_file_size, scan_directory,
};
use crate::library_sync::LibrarySync;
use crate::metadata::extract_all_file_info;
use crate::models::Track;
use crate::state::AppState;
use crate::track_relink::MissingTrackIndex;
use crate::validation::validate_file_path;
use chrono::Utc;
use std::path::Path;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use uuid::Uuid;

/// フォルダから音楽ファイルをインポート（バッチ処理最適化版）
#[tauri::command]
#[specta::specta]
pub async fn import_folder(
    folder_path: String,
    duplicate_action: DuplicateAction,
    app_handle: AppHandle,
) -> AppResult<ImportResult> {
    // ファイルパスをバリデーション
    validate_file_path(&folder_path)?;

    // スキャン・メタデータ抽出・バッチ書き込みはブロッキング処理用スレッドで行う
    run_blocking(move || {
        let state = app_handle.state::<AppState>();
        let result = {
            // 自動の再スキャンと同じファイルを同時に登録しないよう、終わるまで待ってから始める
            let _scan = state.lock_library_scan();
            import_folder_blocking(
                Path::new(&folder_path),
                duplicate_action,
                state.inner(),
                &app_handle,
            )?
        };
        // 記録したフォルダを監視する（監視が有効な場合）
        app_handle
            .state::<LibrarySync>()
            .refresh_watches(&app_handle);
        // 設定ウィンドウのライブラリフォルダの一覧にも反映させる
        if let Err(e) = LibraryChanged.emit(&app_handle) {
            log::warn!("ライブラリの変更の通知に失敗しました: {}", e);
        }
        Ok(result)
    })
    .await
}

/// `import_folder`の本体（同期処理）
fn import_folder_blocking(
    path: &Path,
    duplicate_action: DuplicateAction,
    state: &AppState,
    app_handle: &AppHandle,
) -> AppResult<ImportResult> {
    // ディレクトリをスキャン
    let audio_files = scan_directory(path)?;

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

            let progress = ImportProgress {
                current: processed_count,
                total: total_files,
                current_file,
            };
            if let Err(e) = progress.emit(app_handle) {
                log::warn!("進捗イベントの送信に失敗しました: {}", e);
            }

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

    // インポートしたフォルダをライブラリフォルダとして記録する（再スキャンの対象になる）
    // 音楽ファイルが見つからなかったフォルダは記録しない（中のライブラリフォルダの記録を
    // まとめて消さないため）。記録に失敗してもインポート自体は成功しているため、結果は返す
    if !audio_files.is_empty()
        && let Some(folder_path) = path.to_str()
        && let Err(e) = state.with_db(|db| crate::library_folder::register_folder(db, folder_path))
    {
        log::error!("ライブラリフォルダの記録に失敗しました: {}", e);
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
        is_missing: false,
    })
}
