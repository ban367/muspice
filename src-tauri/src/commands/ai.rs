//! 曲の解析（テンポ・キー）と、AIシャッフルの選曲のコマンド（ADR-104）

use super::run_blocking;
use crate::ai_shuffle::{self, Random};
use crate::error::{AppError, AppResult};
use crate::models::Track;
use crate::music_analysis::{self, MusicAnalysisState, MusicAnalysisStatus};
use crate::settings::SettingsState;
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::collections::HashSet;
use tauri::{AppHandle, Manager, State};

/// AIシャッフルで避ける曲として受け取る、曲の数の上限（キューの前後の件数の2倍と、今の曲）
const MAX_EXCLUDED_TRACKS: usize = ai_shuffle::EXCLUSION_WINDOW as usize * 2 + 1;

/// 曲の解析の状態を取得する（このMacで使えるか・実行中か・解析した曲の数）
#[tauri::command]
#[specta::specta]
pub async fn get_music_analysis_status(
    state: State<'_, AppState>,
    analysis: State<'_, MusicAnalysisState>,
) -> AppResult<MusicAnalysisStatus> {
    music_analysis::status(&state, &analysis)
}

/// 曲の解析を、バックグラウンドで始める（解析していない曲を1曲ずつ。実行中なら何もしない）
///
/// すぐに返る。進み具合と終わりは`MusicAnalysisProgress`で知らせる。止めるまでに終わらなければ、
/// 次の起動で続きから再開する。このMacで使えない時（macOS 27より古い）は`VALIDATION`。
#[tauri::command]
#[specta::specta]
pub async fn start_music_analysis(app: AppHandle) -> AppResult<()> {
    music_analysis::start(&app)
}

/// 実行中の曲の解析を止める（解析中の曲が終わってから止まる）。次の起動でも再開しない
#[tauri::command]
#[specta::specta]
pub async fn stop_music_analysis(app: AppHandle) -> AppResult<()> {
    music_analysis::stop(&app)
}

/// 曲の解析の結果をすべて消す（実行中は`LOCK`）
#[tauri::command]
#[specta::specta]
pub async fn delete_music_analysis(
    state: State<'_, AppState>,
    analysis: State<'_, MusicAnalysisState>,
) -> AppResult<MusicAnalysisStatus> {
    if analysis.is_running() {
        return Err(AppError::Lock(
            "曲の解析を実行中です。止めてから削除してください".to_string(),
        ));
    }
    state.with_db(|db| music_analysis::delete_all(db))?;
    log::info!("Deleted the music analysis data");
    music_analysis::status(&state, &analysis)
}

/// AIシャッフルで、今の曲の次に足す曲を選ぶ（選び方は設定の`aiShuffleMode`）
///
/// `excluded_track_ids`の曲（キューの今の位置の前後の曲）は選ばない。ほかに選べる曲がなければ、
/// 今の曲だけを避けて選ぶ。選べる曲がない（ライブラリに今の曲しかない）時はnull。
#[tauri::command]
#[specta::specta]
pub async fn pick_ai_shuffle_track(
    current_track_id: String,
    excluded_track_ids: Vec<String>,
    app: AppHandle,
) -> AppResult<Option<Track>> {
    validate_track_id(&current_track_id)?;
    if excluded_track_ids.len() > MAX_EXCLUDED_TRACKS {
        return Err(AppError::Validation("避ける曲が多すぎます".to_string()));
    }
    for track_id in &excluded_track_ids {
        validate_track_id(track_id)?;
    }
    let excluded: HashSet<String> = excluded_track_ids.into_iter().collect();

    run_blocking(move || {
        let mode = app.state::<SettingsState>().get()?.ai_shuffle_mode;
        let state = app.state::<AppState>();
        state.with_db(|db| {
            let candidates = ai_shuffle::load_candidates(db)?;
            let picked = ai_shuffle::pick(
                mode,
                &current_track_id,
                &candidates,
                &excluded,
                &mut Random::new(),
            );
            picked
                .map(|track_id| crate::repository::find_track_by_id(db, &track_id))
                .transpose()
        })
    })
    .await
}
