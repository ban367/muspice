//! トラック取得・検索・フィルタリング・一覧（アルバム・アーティスト・ジャンル）コマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::library::{DeleteResult, delete_tracks, delete_tracks_with_files};
use crate::models::{AlbumGroup, AlbumSummary, ArtistSummary, GenreSummary, Track};
use crate::state::AppState;
use crate::validation::{sanitize_search_query, validate_track_id};
use tauri::{AppHandle, Manager, State};

/// フィルタオプション（repository::FilterOptionsの再エクスポート）
pub type FilterOptions = crate::repository::FilterOptions;

/// すべてのトラックを取得
#[tauri::command]
#[specta::specta]
pub async fn get_all_tracks(state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_all_tracks(db))
}

/// トラックを検索（部分一致。大文字と小文字・全角と半角・ひらがなとカタカナを同じとみなす）
#[tauri::command]
#[specta::specta]
pub async fn search_tracks(query: String, state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    // 検索クエリをサニタイズ
    let sanitized = sanitize_search_query(&query);
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    state.with_db(|db| crate::repository::search_tracks_by_query(db, &sanitized))
}

/// トラックをフィルタリング
#[tauri::command]
#[specta::specta]
pub async fn filter_tracks(
    filters: FilterOptions,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_tracks_by_filter(db, &filters))
}

/// ユニークなアーティスト一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_unique_artists(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.with_db(|db| crate::repository::find_unique_artists(db))
}

/// ユニークなアルバム一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_unique_albums(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.with_db(|db| crate::repository::find_unique_albums(db))
}

/// ユニークなジャンル一覧を取得
#[tauri::command]
#[specta::specta]
pub async fn get_unique_genres(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.with_db(|db| crate::repository::find_unique_genres(db))
}

/// アルバムの一覧を取得（曲は含めない）
#[tauri::command]
#[specta::specta]
pub async fn get_albums(state: State<'_, AppState>) -> AppResult<Vec<AlbumSummary>> {
    state.with_db(|db| crate::repository::find_album_summaries(db))
}

/// アルバムの曲を取得
///
/// `artist`は、アルバムをまとめたアーティスト（`AlbumSummary`の`artist`）。
#[tauri::command]
#[specta::specta]
pub async fn get_album_tracks(
    album: String,
    artist: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_album_tracks(db, &album, artist.as_deref()))
}

/// アーティストの一覧を取得（アルバムと曲は含めない）
#[tauri::command]
#[specta::specta]
pub async fn get_artists(state: State<'_, AppState>) -> AppResult<Vec<ArtistSummary>> {
    state.with_db(|db| crate::repository::find_artist_summaries(db))
}

/// アーティストのアルバムと曲を取得
#[tauri::command]
#[specta::specta]
pub async fn get_artist_albums(
    artist: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<AlbumGroup>> {
    state.with_db(|db| crate::repository::find_artist_albums(db, &artist))
}

/// ジャンルの一覧を取得（曲は含めない）
#[tauri::command]
#[specta::specta]
pub async fn get_genres(state: State<'_, AppState>) -> AppResult<Vec<GenreSummary>> {
    state.with_db(|db| crate::repository::find_genre_summaries(db))
}

/// ジャンルの曲を取得
#[tauri::command]
#[specta::specta]
pub async fn get_genre_tracks(genre: String, state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    state.with_db(|db| crate::repository::find_genre_tracks(db, &genre))
}

/// トラックをライブラリから削除（データベースのみ）
/// ファイルは削除せず、データベースからのみ削除
#[tauri::command]
#[specta::specta]
pub async fn delete_tracks_command(
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<u32> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "削除するトラックが指定されていません".to_string(),
        ));
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
#[specta::specta]
pub async fn delete_tracks_with_files_command(
    track_ids: Vec<String>,
    app: AppHandle,
) -> AppResult<DeleteResult> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "削除するトラックが指定されていません".to_string(),
        ));
    }

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    // ファイル削除を伴うため、ブロッキング処理用スレッドで実行する
    run_blocking(move || {
        app.state::<AppState>()
            .with_db(|db| delete_tracks_with_files(db, &track_ids))
    })
    .await
}
