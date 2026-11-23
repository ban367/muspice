//! オートプレイリスト（条件で曲を集めるプレイリスト）のコマンド（ADR-042・ADR-127）
//!
//! 通常のプレイリストのコマンド（`playlist_cmd`）とは分けている。曲の追加・削除・並べ替え・フォルダは
//! ない（曲は条件で決まる）。

use crate::error::AppResult;
use crate::models::{Playlist, SmartPlaylist, Track};
use crate::smart_playlist::{SmartRules, validate_rules};
use crate::state::AppState;
use crate::validation::{
    validate_playlist_description, validate_playlist_name, validate_smart_playlist_id,
};
use tauri::State;

/// すべてのオートプレイリストを取得（手動の並び順）
#[tauri::command]
#[specta::specta]
pub async fn get_smart_playlists(state: State<'_, AppState>) -> AppResult<Vec<SmartPlaylist>> {
    state.with_db(|db| crate::smart_playlist::get_all_smart_playlists(db))
}

/// オートプレイリストを作成
#[tauri::command]
#[specta::specta]
pub async fn create_smart_playlist(
    name: String,
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<SmartPlaylist> {
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;
    validate_rules(&rules)?;

    state.with_db(|db| crate::smart_playlist::create_smart_playlist(db, &name, &rules))
}

/// オートプレイリストの名前と条件を変更（条件の編集画面の保存）
#[tauri::command]
#[specta::specta]
pub async fn update_smart_playlist(
    smart_playlist_id: String,
    name: String,
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_smart_playlist_id(&smart_playlist_id)?;
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;
    validate_rules(&rules)?;

    state.with_db(|db| {
        crate::smart_playlist::update_smart_playlist(db, &smart_playlist_id, &name, &rules)
    })
}

/// オートプレイリストの名前と説明を変更（`description`がnull・空なら、説明を消す）
#[tauri::command]
#[specta::specta]
pub async fn update_smart_playlist_info(
    smart_playlist_id: String,
    name: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_smart_playlist_id(&smart_playlist_id)?;
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;
    if let Some(description) = &description {
        validate_playlist_description(description)?;
    }

    state.with_db(|db| {
        crate::smart_playlist::update_smart_playlist_info(
            db,
            &smart_playlist_id,
            &name,
            description.as_deref(),
        )
    })
}

/// オートプレイリストを、渡した順に並べる（サイドバーの手動の並び順）
#[tauri::command]
#[specta::specta]
pub async fn reorder_smart_playlists(
    smart_playlist_ids: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    for smart_playlist_id in &smart_playlist_ids {
        validate_smart_playlist_id(smart_playlist_id)?;
    }

    state.with_db(|db| crate::smart_playlist::reorder_smart_playlists(db, &smart_playlist_ids))
}

/// オートプレイリストを削除
#[tauri::command]
#[specta::specta]
pub async fn delete_smart_playlist(
    smart_playlist_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_smart_playlist_id(&smart_playlist_id)?;

    state.with_db(|db| crate::smart_playlist::delete_smart_playlist(db, &smart_playlist_id))
}

/// オートプレイリストの曲を取得（条件に合う曲を、条件の並び順で）
#[tauri::command]
#[specta::specta]
pub async fn get_smart_playlist_tracks(
    smart_playlist_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    validate_smart_playlist_id(&smart_playlist_id)?;

    let context = state.smart_playlist_context();
    state.with_db(|db| {
        crate::smart_playlist::get_smart_playlist_tracks(db, &smart_playlist_id, context)
    })
}

/// 条件に合う曲数を数える（条件の編集画面で、保存する前に出す。上限があれば、それを超えない）
#[tauri::command]
#[specta::specta]
pub async fn count_smart_playlist_tracks(
    rules: SmartRules,
    state: State<'_, AppState>,
) -> AppResult<u32> {
    validate_rules(&rules)?;

    let context = state.smart_playlist_context();
    state.with_db(|db| crate::smart_playlist::count_tracks(db, &rules, context))
}

/// ランダムな並びのオートプレイリストを、選び直す
///
/// ランダムな並びは、一覧を取り直しても変わらない（聴いている途中で並びが変わらないようにするため）。
/// 別の曲・別の順にしたい時に呼ぶ（すべてのオートプレイリストの並びが変わる）。
#[tauri::command]
#[specta::specta]
pub async fn reshuffle_smart_playlists(state: State<'_, AppState>) -> AppResult<()> {
    state.reshuffle_smart_playlists();
    Ok(())
}

/// オートプレイリストの今の曲（並び順のまま）を入れた、通常のプレイリストを作る
///
/// 作ったプレイリストは、その後の条件・曲の変更に追随しない。
#[tauri::command]
#[specta::specta]
pub async fn save_smart_playlist_as_playlist(
    smart_playlist_id: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<Playlist> {
    validate_smart_playlist_id(&smart_playlist_id)?;
    let name = name.trim().to_string();
    validate_playlist_name(&name)?;

    let context = state.smart_playlist_context();
    state.with_db(|db| {
        crate::smart_playlist::save_as_playlist(db, &smart_playlist_id, &name, context)
    })
}
