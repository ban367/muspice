//! トラック取得・検索・フィルタリング・グループ化コマンド

use crate::library::{delete_tracks, delete_tracks_with_files, DeleteResult};
use crate::models::{AlbumGroup, ArtistGroup, GenreGroup, Track};
use crate::state::AppState;
use crate::validation::{sanitize_search_query, validate_track_id};
use tauri::State;

/// フィルタオプション（repository::FilterOptionsの再エクスポート）
pub type FilterOptions = crate::repository::FilterOptions;

/// すべてのトラックを取得
#[tauri::command]
pub async fn get_all_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    state.with_db(|db| crate::repository::find_all_tracks(db))
}

/// トラックを検索（FTS5 + LIKEフォールバック）
#[tauri::command]
pub async fn search_tracks(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    // 検索クエリをサニタイズ
    let sanitized = sanitize_search_query(&query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    state.with_db(|db| crate::repository::search_tracks_by_query(db, &sanitized))
}

/// トラックをフィルタリング
#[tauri::command]
pub async fn filter_tracks(
    filters: FilterOptions,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    state.with_db(|db| crate::repository::find_tracks_by_filter(db, &filters))
}

/// ユニークなアーティスト一覧を取得
#[tauri::command]
pub async fn get_unique_artists(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.with_db(|db| crate::repository::find_unique_artists(db))
}

/// ユニークなアルバム一覧を取得
#[tauri::command]
pub async fn get_unique_albums(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.with_db(|db| crate::repository::find_unique_albums(db))
}

/// ユニークなジャンル一覧を取得
#[tauri::command]
pub async fn get_unique_genres(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.with_db(|db| crate::repository::find_unique_genres(db))
}

/// アルバム一覧（グループ化）を取得
#[tauri::command]
pub async fn get_albums_grouped(state: State<'_, AppState>) -> Result<Vec<AlbumGroup>, String> {
    state.with_db(|db| crate::repository::find_albums_grouped(db))
}

/// アーティスト一覧（グループ化）を取得
#[tauri::command]
pub async fn get_artists_grouped(state: State<'_, AppState>) -> Result<Vec<ArtistGroup>, String> {
    state.with_db(|db| crate::repository::find_artists_grouped(db))
}

/// ジャンル一覧（グループ化）を取得
#[tauri::command]
pub async fn get_genres_grouped(state: State<'_, AppState>) -> Result<Vec<GenreGroup>, String> {
    state.with_db(|db| crate::repository::find_genres_grouped(db))
}

/// トラックをライブラリから削除（データベースのみ）
/// ファイルは削除せず、データベースからのみ削除
#[tauri::command]
pub async fn delete_tracks_command(
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    if track_ids.is_empty() {
        return Err("削除するトラックが指定されていません".to_string());
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    state.with_db(|db| delete_tracks(db, &track_ids))
}

/// トラックをライブラリとファイルシステムから削除
/// データベースとファイル両方を削除
#[tauri::command]
pub async fn delete_tracks_with_files_command(
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<DeleteResult, String> {
    if track_ids.is_empty() {
        return Err("削除するトラックが指定されていません".to_string());
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    state.with_db(|db| delete_tracks_with_files(db, &track_ids))
}
