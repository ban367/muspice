//! お気に入り・レーティング・再生統計コマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::metadata::{update_file_rating, validate_rating};
use crate::models::{PlayHistoryEntry, Track};
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::path::Path;
use tauri::State;

/// お気に入りを切り替え
#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite(track_id: String, state: State<'_, AppState>) -> AppResult<bool> {
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::toggle_track_favorite(db, &track_id))
}

/// レーティングを設定（ファイルのタグへ書き込み、同じ値をデータベースに記録する）
///
/// ファイルへ書き込めない場合はエラーにし、データベースも変えない。
#[tauri::command]
#[specta::specta]
pub async fn set_rating(
    track_id: String,
    rating: i32,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_track_id(&track_id)?;
    validate_rating(rating)?;

    let file_path =
        state.with_db(|db| crate::repository::find_file_path_by_track_id(db, &track_id))?;

    // ファイルへの書き込みはDBロックの外・ブロッキング処理用スレッドで行う。書き込み後のサイズと
    // 更新日時も取得し、再スキャンで自分の書き込みを変更として検出しないようにする
    let (file_size, file_modified_at) = run_blocking(move || {
        let path = Path::new(&file_path);
        update_file_rating(path, rating)?;
        Ok((
            crate::library::get_file_size(path).ok(),
            crate::library::get_file_modified_at(path),
        ))
    })
    .await?;

    state.with_db(|db| {
        let tx = db.transaction().map_err(|e| {
            AppError::Database(format!("トランザクションの開始に失敗しました: {}", e))
        })?;
        crate::repository::set_track_rating(&tx, &track_id, rating)?;
        if let Some(file_size) = file_size {
            crate::repository::set_track_file_state(&tx, &track_id, file_size, file_modified_at)?;
        }
        tx.commit().map_err(|e| {
            AppError::Database(format!("トランザクションのコミットに失敗しました: {}", e))
        })
    })
}

/// 再生回数をインクリメントし、再生履歴に追加する（新しい再生回数を返す）
///
/// いつ数えるか（曲の半分か4分を聴いた時）は、フロントエンドの再生コントローラーが決める。
#[tauri::command]
#[specta::specta]
pub async fn increment_play_count(track_id: String, state: State<'_, AppState>) -> AppResult<i32> {
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::increment_track_play_count(db, &track_id))
}

/// スキップ回数をインクリメントする（新しいスキップ回数を返す）
///
/// いつ数えるか（再生回数に数える前に、別の曲へ移った時）は、フロントエンドの再生コントローラーが決める。
#[tauri::command]
#[specta::specta]
pub async fn increment_skip_count(track_id: String, state: State<'_, AppState>) -> AppResult<i32> {
    validate_track_id(&track_id)?;

    state.with_db(|db| crate::repository::increment_track_skip_count(db, &track_id))
}

/// お気に入りトラック一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_favorite_tracks(state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_favorite_tracks(db))
}

/// 最も再生されたトラック一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_most_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    let limit = limit.unwrap_or(50);
    state.with_db(|db| crate::repository::find_most_played_tracks(db, limit))
}

/// 再生履歴を取得（新しい順。同じ曲が何度も出る。件数の上限はない）
///
/// 曲の情報は含めない（フロントエンドが、全曲の一覧からトラックIDで引く）。
#[tauri::command]
#[specta::specta]
pub async fn get_play_history(state: State<'_, AppState>) -> AppResult<Vec<PlayHistoryEntry>> {
    state.with_db(|db| crate::repository::find_play_history(db))
}
