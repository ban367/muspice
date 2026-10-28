//! 音量（ラウドネス）の解析のコマンド

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::events::{LibraryChanged, LoudnessScanFinished, LoudnessScanProgress};
use crate::loudness_scan::{
    LoudnessScanResult, LoudnessScanState, ScanScope, plan_units, scan_units,
};
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 音量の解析の状態
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessScanStatus {
    /// 解析を実行中か
    pub running: bool,
    /// 読み終わった曲の数と、読む曲の総数（実行中でなければ0）
    pub current: u32,
    pub total: u32,
    /// 音量の情報が欠けている曲の数（見つからない曲を除く）
    pub pending_count: u32,
}

/// 音量の解析の状態を取得する（設定の画面を開いた時と、解析が終わった時に読む）
#[tauri::command]
#[specta::specta]
pub async fn get_loudness_scan_status(
    state: State<'_, AppState>,
    scan: State<'_, LoudnessScanState>,
) -> AppResult<LoudnessScanStatus> {
    let (running, current, total) = scan.progress();
    let pending_count =
        state.with_db(|db| crate::repository::count_tracks_without_replay_gain(db))?;
    Ok(LoudnessScanStatus {
        running,
        current: if running { current } else { 0 },
        total: if running { total } else { 0 },
        pending_count,
    })
}

/// 曲のラウドネスを解析して、ゲインとピークをファイルのタグ（ReplayGain）へ書き込む
///
/// `track_ids`を省くと、ライブラリの、音量の情報が欠けている曲を解析する（すでにあるゲインは変えない）。
/// 指定すると、その曲を解析し直す。アルバムのゲインは、ライブラリにあるそのアルバムの全曲から求め、
/// 全曲に書く（アルバムのほかの曲も読む）。
///
/// 終わるまで返らない（`LoudnessScanProgress`を送り、終わると`LoudnessScanFinished`を送る）。
/// 同時に実行できる解析は1つで、実行中は`LOCK`。解析できなかった・書き込めなかった曲は、
/// 結果の`errors`へ理由を入れて残りを続ける。
#[tauri::command]
#[specta::specta]
pub async fn start_loudness_scan(
    track_ids: Option<Vec<String>>,
    app: AppHandle,
) -> AppResult<LoudnessScanResult> {
    let scope = match track_ids {
        None => ScanScope::Missing,
        Some(track_ids) => {
            if track_ids.is_empty() {
                return Err(AppError::Validation(
                    "解析する曲が指定されていません".to_string(),
                ));
            }
            for track_id in &track_ids {
                validate_track_id(track_id)?;
            }
            ScanScope::Tracks(track_ids.into_iter().collect())
        }
    };

    run_blocking(move || {
        let scan = app.state::<LoudnessScanState>();
        let _running = scan
            .begin()
            .ok_or_else(|| AppError::Lock("音量の解析を実行中です".to_string()))?;
        let state = app.state::<AppState>();

        let tracks = state.with_db(|db| crate::repository::find_all_tracks(db))?;
        let units = plan_units(&tracks, &scope);
        drop(tracks);

        let result = scan_units(state.inner(), &units, scan.cancel_flag(), &|progress| {
            scan.set_progress(progress.current, progress.total);
            let event = LoudnessScanProgress {
                current: progress.current,
                total: progress.total,
                current_file: progress.current_file,
            };
            if let Err(e) = event.emit(&app) {
                log::warn!("Failed to emit loudness analysis progress event: {}", e);
            }
        })?;

        log::info!(
            "Analyzed loudness: written={}, unchanged={}, errors={}, cancelled={}",
            result.written_count,
            result.unchanged_count,
            result.error_count,
            result.cancelled
        );
        if result.written_count > 0
            && let Err(e) = LibraryChanged.emit(&app)
        {
            log::warn!("Failed to emit library change event: {}", e);
        }
        if let Err(e) = LoudnessScanFinished(result.clone()).emit(&app) {
            log::warn!("Failed to emit loudness analysis completion event: {}", e);
        }
        Ok(result)
    })
    .await
}

/// 実行中の音量の解析を中断する（実行中でなければ何もしない）
///
/// 解析の途中のアルバムには何も書かずに終わる。次に解析すると、残りの曲から続く。
#[tauri::command]
#[specta::specta]
pub async fn cancel_loudness_scan(scan: State<'_, LoudnessScanState>) -> AppResult<()> {
    scan.request_cancel();
    Ok(())
}
