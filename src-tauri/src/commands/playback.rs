//! 再生エンジン（`playback`）のコマンド
//!
//! 再生キューはフロントエンドが持ち、ここでは「この曲を再生する」「続けてこの曲を再生する」を
//! エンジンへ伝える。ファイルのパスはWebViewから受け取らず、トラックIDからDBで解決する。
//! 再生位置・曲の切り替わり・終了は、`PlaybackEvent`でフロントエンドへ伝わる。

use super::run_blocking;
use crate::error::{AppError, AppResult};
use crate::playback::{OutputDevice, PlayRequest, PlaybackEngine, PlaybackTrackInfo};
use crate::state::AppState;
use crate::validation::validate_track_id;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

/// トラックIDから、再生するファイルと、音量の正規化に使う値を決める
fn play_request(state: &AppState, track_id: &str, token: u32) -> AppResult<PlayRequest> {
    validate_track_id(track_id)?;
    let track = state.with_db(|db| crate::repository::find_track_by_id(db, track_id))?;
    Ok(PlayRequest {
        token,
        path: PathBuf::from(track.file_path),
        replay_gain: track.replay_gain,
    })
}

/// トラックを頭から再生する（再生中の曲は止める）
///
/// `token`は、フロントエンドが再生ごとに振る番号。この曲についての`PlaybackEvent`に付いて返る。
#[tauri::command]
#[specta::specta]
pub async fn playback_play(
    track_id: String,
    token: u32,
    app: AppHandle,
) -> AppResult<PlaybackTrackInfo> {
    // ファイルを開く・出力デバイスを開くのに時間がかかる場合があるため、別スレッドで待つ
    run_blocking(move || {
        let request = play_request(&app.state::<AppState>(), &track_id, token)?;
        app.state::<PlaybackEngine>().play(request)
    })
    .await
}

/// 再生中の曲に続けて再生するトラックを用意する（nullで取り消す）
///
/// 用意しておくと、再生中の曲の終わりから切れ目なく続けて再生し（ギャップレス再生）、
/// 切り替わった時点で`PlaybackEvent`の`advanced`が届く。
#[tauri::command]
#[specta::specta]
pub async fn playback_set_next(
    track_id: Option<String>,
    token: u32,
    app: AppHandle,
) -> AppResult<()> {
    run_blocking(move || {
        let request = track_id
            .map(|track_id| play_request(&app.state::<AppState>(), &track_id, token))
            .transpose()?;
        app.state::<PlaybackEngine>().set_next(request)
    })
    .await
}

/// 再生を一時停止する
#[tauri::command]
#[specta::specta]
pub async fn playback_pause(engine: State<'_, PlaybackEngine>) -> AppResult<()> {
    engine.pause()
}

/// 一時停止した再生を再開する
#[tauri::command]
#[specta::specta]
pub async fn playback_resume(engine: State<'_, PlaybackEngine>) -> AppResult<()> {
    engine.resume()
}

/// 再生中の曲の、指定した位置（秒）へ移動する
#[tauri::command]
#[specta::specta]
pub async fn playback_seek(position: f64, engine: State<'_, PlaybackEngine>) -> AppResult<()> {
    if !position.is_finite() || position < 0.0 {
        return Err(AppError::Validation(
            "再生位置が正しくありません".to_string(),
        ));
    }
    engine.seek(position)
}

/// 音量（0.0〜1.0）を設定する
#[tauri::command]
#[specta::specta]
pub async fn playback_set_volume(volume: f32, engine: State<'_, PlaybackEngine>) -> AppResult<()> {
    if !volume.is_finite() {
        return Err(AppError::Validation("音量が正しくありません".to_string()));
    }
    engine.set_volume(volume)
}

/// 再生を止める（再生中の曲・続けて再生する曲を手放す）
#[tauri::command]
#[specta::specta]
pub async fn playback_stop(engine: State<'_, PlaybackEngine>) -> AppResult<()> {
    engine.stop()
}

/// イコライザの設定を変える（再生中の音にすぐに効く）
///
/// `gains`は、バンドごとのゲイン（dB。31Hz〜16kHzの10個。範囲外の値は-12〜12に収める）。
/// 設定はフロントエンドが保存しており（`equalizer.svelte.ts`）、起動時と変更のたびに送る。
#[tauri::command]
#[specta::specta]
pub async fn playback_set_equalizer(
    enabled: bool,
    gains: Vec<f32>,
    engine: State<'_, PlaybackEngine>,
) -> AppResult<()> {
    let gains: [f32; crate::playback::EQ_BANDS] = gains.try_into().map_err(|_| {
        AppError::Validation(format!(
            "イコライザのゲインは{}個で指定してください",
            crate::playback::EQ_BANDS
        ))
    })?;
    engine.set_equalizer(enabled, &gains);
    Ok(())
}

/// 出力デバイスの一覧を取得する
#[tauri::command]
#[specta::specta]
pub async fn get_output_devices() -> AppResult<Vec<OutputDevice>> {
    run_blocking(crate::playback::list_output_devices).await
}
