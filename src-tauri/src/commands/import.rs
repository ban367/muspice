//! 音楽ファイルのインポート関連コマンド

use crate::error::{AppError, AppResult};
use crate::library::{
    get_default_title, get_file_format, get_file_size, scan_directory, DuplicateAction,
    ImportResult,
};
use crate::metadata::extract_all_file_info;
use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_file_path;
use chrono::Utc;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

/// インポート進捗イベントのペイロード
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    /// 処理済みファイル数
    current: usize,
    /// 総ファイル数
    total: usize,
    /// 現在処理中のファイル名
    current_file: String,
}

/// フォルダから音楽ファイルをインポート（バッチ処理最適化版）
#[tauri::command]
pub async fn import_folder(
    folder_path: String,
    duplicate_action: DuplicateAction,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> AppResult<ImportResult> {
    // ファイルパスをバリデーション
    validate_file_path(&folder_path)?;

    let path = Path::new(&folder_path);

    // ディレクトリをスキャン
    let audio_files = scan_directory(path)?;

    state.with_db(|db| {
        let mut imported_count = 0;
        let mut skipped_count = 0;
        let mut error_count = 0;
        let mut errors = Vec::new();

        // バッチサイズ（一度にコミットするファイル数）
        const BATCH_SIZE: usize = 50;
        let total_files = audio_files.len();
        let mut processed_count = 0;

        // バッチ処理でインポート
        for chunk in audio_files.chunks(BATCH_SIZE) {
            // トランザクション開始
            let tx = db.transaction().map_err(|e| {
                AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
            })?;

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

                if let Err(e) = app_handle.emit(
                    "import-progress",
                    ImportProgress {
                        current: processed_count,
                        total: total_files,
                        current_file,
                    },
                ) {
                    crate::logger::warning(&format!("進捗イベントの送信に失敗しました: {}", e));
                }

                // 重複チェック
                let is_duplicate =
                    match crate::repository::track_exists_by_file_path(&tx, file_path_str) {
                        Ok(dup) => dup,
                        Err(e) => {
                            errors.push(format!("{}: {}", file_path_str, e));
                            error_count += 1;
                            continue;
                        }
                    };

                if is_duplicate {
                    match duplicate_action {
                        DuplicateAction::Skip => {
                            skipped_count += 1;
                            continue;
                        }
                        DuplicateAction::Replace => {
                            // 既存のトラックを更新
                            match process_and_update_track_in_tx(&tx, file_path) {
                                Ok(_) => imported_count += 1,
                                Err(e) => {
                                    errors.push(format!("{}: {}", file_path_str, e));
                                    error_count += 1;
                                }
                            }
                        }
                    }
                } else {
                    // 新しいトラックを追加
                    match process_and_save_track_in_tx(&tx, file_path) {
                        Ok(_) => imported_count += 1,
                        Err(e) => {
                            errors.push(format!("{}: {}", file_path_str, e));
                            error_count += 1;
                        }
                    }
                }
            }

            // バッチをコミット
            tx.commit().map_err(|e| {
                AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
            })?;

            // 進行状況をログ出力
            crate::logger::info(&format!(
                "インポート進行状況: {}/{} ファイル処理完了",
                processed_count, total_files
            ));
        }

        Ok(ImportResult {
            imported_count,
            skipped_count,
            error_count,
            errors,
        })
    })
}

/// トラックを処理してトランザクション内で保存
fn process_and_save_track_in_tx(tx: &rusqlite::Transaction, file_path: &Path) -> AppResult<()> {
    let track = create_track_from_file(file_path)?;
    crate::repository::insert_track(tx, &track)
}

/// トラックを処理してトランザクション内で更新
fn process_and_update_track_in_tx(tx: &rusqlite::Transaction, file_path: &Path) -> AppResult<()> {
    let track = create_track_from_file(file_path)?;
    crate::repository::update_track_by_file_path(tx, &track)
}

/// ファイルからトラック情報を作成
fn create_track_from_file(file_path: &Path) -> AppResult<Track> {
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
        rating: 0,
        play_count: 0,
        last_played_at: None,
        created_at: now.clone(),
        updated_at: now,
    })
}
