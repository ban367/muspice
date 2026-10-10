//! プレイリスト管理コマンド

use crate::error::{AppError, AppResult};
use crate::models::PlaylistFolder;
use crate::smart_playlist::{SmartRules, validate_rules};
use crate::state::AppState;
use crate::validation::{
    validate_playlist_description, validate_playlist_folder_id, validate_playlist_folder_name,
    validate_playlist_id, validate_playlist_name, validate_track_id,
};
use chrono::Utc;
use tauri::State;

/// プレイリストの操作のエラーを、`AppError`にする（行が見つからない場合は`NotFound`）
fn playlist_error(error: rusqlite::Error, not_found: &str, operation: &str) -> AppError {
    match error {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(not_found.to_string()),
        _ => AppError::Database(format!("{}に失敗しました: {}", operation, error)),
    }
}

/// プレイリストを作成
#[tauri::command]
#[specta::specta]
pub async fn create_playlist(
    name: String,
    state: State<'_, AppState>,
) -> AppResult<crate::models::Playlist> {
    // プレイリスト名をバリデーション
    validate_playlist_name(&name)?;

    state.with_db(|db| {
        crate::playlist::create_playlist(db, &name)
            .map_err(|e| AppError::Database(format!("プレイリストの作成に失敗しました: {}", e)))
    })
}

/// 自動プレイリスト（条件で曲を集めるプレイリスト）を作成
#[tauri::command]
#[specta::specta]
pub async fn create_smart_playlist(
    name: String,
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<crate::models::Playlist> {
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;
    validate_rules(&rules)?;

    state.with_db(|db| {
        crate::playlist::create_smart_playlist(db, &name, &rules)
            .map_err(|e| AppError::Database(format!("プレイリストの作成に失敗しました: {}", e)))
    })
}

/// 自動プレイリストの条件を変更（名前は`rename_playlist`で変える）
#[tauri::command]
#[specta::specta]
pub async fn update_smart_playlist(
    playlist_id: String,
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_playlist_id(&playlist_id)?;
    validate_rules(&rules)?;

    state.with_db(|db| crate::playlist::update_smart_playlist_rules(db, &playlist_id, &rules))
}

/// 条件に合う曲数を数える（条件の編集画面で、保存する前に出す。上限があれば、それを超えない）
#[tauri::command]
#[specta::specta]
pub async fn count_smart_playlist_tracks(
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<u32> {
    validate_rules(&rules)?;

    state.with_db(|db| crate::smart_playlist::count_tracks(db, &rules, Utc::now()))
}

/// すべてのプレイリストを取得
#[tauri::command]
#[specta::specta]
pub async fn get_playlists(state: State<'_, AppState>) -> AppResult<Vec<crate::models::Playlist>> {
    state.with_db(|db| {
        crate::playlist::get_all_playlists(db)
            .map_err(|e| AppError::Database(format!("プレイリストの取得に失敗しました: {}", e)))
    })
}

/// プレイリストの曲を取得（プレイリストの中の並び順。自動プレイリストは、条件に合う曲）
#[tauri::command]
#[specta::specta]
pub async fn get_playlist_tracks(
    playlist_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::models::Track>> {
    validate_playlist_id(&playlist_id)?;

    let context = state.smart_playlist_context();
    state.with_db(|db| crate::playlist::get_playlist_tracks(db, &playlist_id, context))
}

/// ランダムな並びの自動プレイリストを、選び直す
///
/// ランダムな並びは、一覧を取り直しても変わらない（聴いている途中で並びが変わらないようにするため）。
/// 別の曲・別の順にしたい時に呼ぶ（すべての自動プレイリストの並びが変わる）。
#[tauri::command]
#[specta::specta]
pub async fn reshuffle_smart_playlists(state: State<'_, AppState>) -> AppResult<()> {
    state.reshuffle_smart_playlists();
    Ok(())
}

/// プレイリストにトラックを追加（複数のトラックを、渡した順に追加する）
///
/// すでに入っているトラックは飛ばし、追加したトラック数を返す。
/// 見つからないトラックがある場合は、1曲も追加しない。
#[tauri::command]
#[specta::specta]
pub async fn add_tracks_to_playlist(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<u32> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "トラックIDが指定されていません".to_string(),
        ));
    }
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    let added = state.with_db(|db| {
        crate::playlist::ensure_manual_playlist(db, &playlist_id)?;
        crate::playlist::add_tracks_to_playlist(db, &playlist_id, &track_ids).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストまたはトラックが見つかりません".to_string())
            }
            _ => AppError::Database(format!("トラックの追加に失敗しました: {}", e)),
        })
    })?;
    crate::library::to_count(added)
}

/// プレイリストから複数のトラックを外す
///
/// 入っていないトラックは飛ばし、外したトラック数を返す。
#[tauri::command]
#[specta::specta]
pub async fn remove_tracks_from_playlist(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<u32> {
    // IDをバリデーション
    validate_playlist_id(&playlist_id)?;
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "トラックIDが指定されていません".to_string(),
        ));
    }
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    let removed = state.with_db(|db| {
        crate::playlist::ensure_manual_playlist(db, &playlist_id)?;
        crate::playlist::remove_tracks_from_playlist(db, &playlist_id, &track_ids)
            .map_err(|e| playlist_error(e, "プレイリストが見つかりません", "トラックの削除"))
    })?;
    crate::library::to_count(removed)
}

/// 渡した曲を入れた状態で、プレイリストを作成（再生キューをプレイリストとして保存する時に使う）
///
/// 同じ曲は、最初の1回だけを入れる。見つからない曲がある場合は、プレイリストを作らない。
#[tauri::command]
#[specta::specta]
pub async fn create_playlist_with_tracks(
    name: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<crate::models::Playlist> {
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    state.with_db(|db| {
        crate::playlist::create_playlist_from_tracks(db, &name, &track_ids)
            .map_err(|e| playlist_error(e, "トラックが見つかりません", "プレイリストの作成"))
    })
}

/// プレイリストの説明を変更（値なし・空の文字列は、説明なし）
#[tauri::command]
#[specta::specta]
pub async fn set_playlist_description(
    playlist_id: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_playlist_id(&playlist_id)?;
    if let Some(description) = &description {
        validate_playlist_description(description)?;
    }

    state.with_db(|db| {
        crate::playlist::set_playlist_description(db, &playlist_id, description.as_deref())
            .map_err(|e| playlist_error(e, "プレイリストが見つかりません", "説明の変更"))
    })
}

/// プレイリストのフォルダを取得（手動の並び順）
#[tauri::command]
#[specta::specta]
pub async fn get_playlist_folders(state: State<'_, AppState>) -> AppResult<Vec<PlaylistFolder>> {
    state.with_db(|db| {
        crate::playlist::get_playlist_folders(db)
            .map_err(|e| AppError::Database(format!("フォルダの取得に失敗しました: {}", e)))
    })
}

/// プレイリストのフォルダを作成
#[tauri::command]
#[specta::specta]
pub async fn create_playlist_folder(
    name: String,
    state: State<'_, AppState>,
) -> AppResult<PlaylistFolder> {
    let name = name.trim().to_string();
    validate_playlist_folder_name(&name)?;

    state.with_db(|db| {
        crate::playlist::create_playlist_folder(db, &name)
            .map_err(|e| AppError::Database(format!("フォルダの作成に失敗しました: {}", e)))
    })
}

/// プレイリストのフォルダの名前を変更
#[tauri::command]
#[specta::specta]
pub async fn rename_playlist_folder(
    folder_id: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_playlist_folder_id(&folder_id)?;
    let name = name.trim().to_string();
    validate_playlist_folder_name(&name)?;

    state.with_db(|db| {
        crate::playlist::rename_playlist_folder(db, &folder_id, &name)
            .map_err(|e| playlist_error(e, "フォルダが見つかりません", "フォルダ名の変更"))
    })
}

/// プレイリストのフォルダを削除（中のプレイリストは消さず、フォルダの外へ出す）
#[tauri::command]
#[specta::specta]
pub async fn delete_playlist_folder(
    folder_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_playlist_folder_id(&folder_id)?;

    state.with_db(|db| {
        crate::playlist::delete_playlist_folder(db, &folder_id)
            .map_err(|e| playlist_error(e, "フォルダが見つかりません", "フォルダの削除"))
    })
}

/// プレイリストを、フォルダへ移す（`folder_id`が値なしなら、フォルダの外へ出す）
///
/// 手動の並び順では、移した先のいちばん後ろに置く。
#[tauri::command]
#[specta::specta]
pub async fn move_playlist(
    playlist_id: String,
    folder_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_playlist_id(&playlist_id)?;
    if let Some(folder_id) = &folder_id {
        validate_playlist_folder_id(folder_id)?;
    }

    state.with_db(|db| {
        crate::playlist::move_playlist(db, &playlist_id, folder_id.as_deref()).map_err(|e| {
            playlist_error(
                e,
                "プレイリストまたはフォルダが見つかりません",
                "プレイリストの移動",
            )
        })
    })
}

/// プレイリストの手動の並び順を変更（渡した順に並べる）
///
/// 並び順は同じフォルダの中で比べるため、1つのフォルダ（またはフォルダの外）のプレイリストを渡す。
#[tauri::command]
#[specta::specta]
pub async fn reorder_playlists(
    playlist_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    for playlist_id in &playlist_ids {
        validate_playlist_id(playlist_id)?;
    }

    state.with_db(|db| {
        crate::playlist::reorder_playlists(db, &playlist_ids)
            .map_err(|e| AppError::Database(format!("プレイリストの並び替えに失敗しました: {}", e)))
    })
}

/// プレイリストのフォルダの手動の並び順を変更（渡した順に並べる）
#[tauri::command]
#[specta::specta]
pub async fn reorder_playlist_folders(
    folder_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    for folder_id in &folder_ids {
        validate_playlist_folder_id(folder_id)?;
    }

    state.with_db(|db| {
        crate::playlist::reorder_playlist_folders(db, &folder_ids)
            .map_err(|e| AppError::Database(format!("フォルダの並び替えに失敗しました: {}", e)))
    })
}

/// プレイリストの名前を変更
#[tauri::command]
#[specta::specta]
pub async fn rename_playlist(
    playlist_id: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 名前をバリデーション
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;

    state.with_db(|db| {
        crate::playlist::rename_playlist(db, &playlist_id, &name).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストが見つかりません".to_string())
            }
            _ => AppError::Database(format!("プレイリスト名の変更に失敗しました: {}", e)),
        })
    })
}

/// プレイリストを削除
#[tauri::command]
#[specta::specta]
pub async fn delete_playlist(playlist_id: String, state: State<'_, AppState>) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    state.with_db(|db| {
        crate::playlist::delete_playlist(db, &playlist_id).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound("プレイリストが見つかりません".to_string())
            }
            _ => AppError::Database(format!("プレイリストの削除に失敗しました: {}", e)),
        })
    })
}

/// プレイリスト内のトラックを並び替え
#[tauri::command]
#[specta::specta]
pub async fn reorder_playlist_tracks(
    playlist_id: String,
    track_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // プレイリストIDをバリデーション
    validate_playlist_id(&playlist_id)?;

    // 各トラックIDをバリデーション
    for track_id in &track_ids {
        validate_track_id(track_id)?;
    }

    state.with_db(|db| {
        crate::playlist::ensure_manual_playlist(db, &playlist_id)?;
        crate::playlist::reorder_playlist_tracks(db, &playlist_id, &track_ids).map_err(
            |e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound("プレイリストが見つかりません".to_string())
                }
                _ => AppError::Database(format!("トラックの並び替えに失敗しました: {}", e)),
            },
        )
    })
}
