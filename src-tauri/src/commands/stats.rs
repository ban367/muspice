//! お気に入り・レーティング・再生統計コマンド

use crate::models::Track;
use crate::state::AppState;
use crate::validation::validate_track_id;
use tauri::State;

/// お気に入りを切り替え
#[tauri::command]
pub async fn toggle_favorite(track_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // 現在のお気に入り状態を取得
    let current: i32 = db
        .query_row(
            "SELECT COALESCE(is_favorite, 0) FROM tracks WHERE id = ?1",
            [&track_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "トラックが見つかりません".to_string(),
            _ => format!("お気に入り状態の取得に失敗しました: {}", e),
        })?;

    let new_value = if current == 0 { 1 } else { 0 };

    db.execute(
        "UPDATE tracks SET is_favorite = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![new_value, track_id],
    )
    .map_err(|e| format!("お気に入りの更新に失敗しました: {}", e))?;

    Ok(new_value == 1)
}

/// レーティングを設定
#[tauri::command]
pub async fn set_rating(
    track_id: String,
    rating: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_track_id(&track_id)?;

    if !(0..=5).contains(&rating) {
        return Err("レーティングは0から5の間で指定してください".to_string());
    }

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    db.execute(
        "UPDATE tracks SET rating = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![rating, track_id],
    )
    .map_err(|e| format!("レーティングの更新に失敗しました: {}", e))?;

    Ok(())
}

/// 再生回数をインクリメント
#[tauri::command]
pub async fn increment_play_count(
    track_id: String,
    state: State<'_, AppState>,
) -> Result<i32, String> {
    validate_track_id(&track_id)?;

    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    // 再生回数をインクリメントし、最終再生日時を更新
    db.execute(
        "UPDATE tracks SET
            play_count = COALESCE(play_count, 0) + 1,
            last_played_at = datetime('now'),
            updated_at = datetime('now')
         WHERE id = ?1",
        [&track_id],
    )
    .map_err(|e| format!("再生回数の更新に失敗しました: {}", e))?;

    // 再生履歴に追加
    db.execute(
        "INSERT INTO play_history (track_id, played_at) VALUES (?1, datetime('now'))",
        [&track_id],
    )
    .map_err(|e| format!("再生履歴の追加に失敗しました: {}", e))?;

    // 新しい再生回数を取得
    let new_count: i32 = db
        .query_row(
            "SELECT play_count FROM tracks WHERE id = ?1",
            [&track_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("再生回数の取得に失敗しました: {}", e))?;

    Ok(new_count)
}

/// お気に入りトラック一覧を取得
#[tauri::command]
pub async fn get_favorite_tracks(state: State<'_, AppState>) -> Result<Vec<Track>, String> {
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::repository::find_favorite_tracks(&db)
}

/// 最も再生されたトラック一覧を取得
#[tauri::command]
pub async fn get_most_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let limit = limit.unwrap_or(50);
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::repository::find_most_played_tracks(&db, limit)
}

/// 最近再生されたトラック一覧を取得
#[tauri::command]
pub async fn get_recently_played_tracks(
    limit: Option<i32>,
    state: State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let limit = limit.unwrap_or(50);
    let db = state
        .db
        .lock()
        .map_err(|e| format!("データベースロックの取得に失敗しました: {}", e))?;

    crate::repository::find_recently_played_tracks(&db, limit)
}
