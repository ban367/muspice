//! ミニプレーヤー（メインウィンドウを、小さな表示に切り替える）
//!
//! 別のウィンドウは作らず、メインウィンドウの大きさを変える（再生キューと再生の制御を持つ
//! WebViewを、そのまま使うため）。小さくしている間に出す画面は、フロントエンドが切り替える。
//!
//! 小さくする前の大きさ・位置は、ファイル（`mini-player.json`）にも記録する。ミニプレーヤーの
//! ままアプリを終了すると、ウィンドウの大きさの記憶（`tauri-plugin-window-state`）には小さい方が
//! 残るため、次の起動時にこの記録から元の大きさへ戻す。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// ミニプレーヤーの大きさ（論理ピクセル。macOSではタイトルバーの分を含むため、画面に使えるのは
/// これより30pxほど低い）
const MINI_SIZE: (f64, f64) = (420.0, 150.0);

/// 通常の表示での、ウィンドウの大きさの下限（`tauri.conf.json`に指定がない場合に使う）
const DEFAULT_MIN_SIZE: (f64, f64) = (640.0, 480.0);

/// 小さくする前のウィンドウの状態（物理ピクセル）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NormalBounds {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    /// 最大化していたか（戻す時に、もう一度最大化する）
    maximized: bool,
}

/// ミニプレーヤーの状態（Tauriの管理状態）
pub struct MiniPlayerState {
    /// 小さくする前の状態の記録先
    path: PathBuf,
    /// 小さくする前の状態（通常の表示の間は`None`）
    normal: Mutex<Option<NormalBounds>>,
}

impl MiniPlayerState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            normal: Mutex::new(None),
        }
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Option<NormalBounds>>> {
        self.normal
            .lock()
            .map_err(|e| AppError::Lock(format!("ミニプレーヤーの状態を読めません: {}", e)))
    }
}

/// 記録を読む（ない・読めない場合は`None`）
fn read_bounds(path: &PathBuf) -> Option<NormalBounds> {
    let json = fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

fn write_bounds(path: &PathBuf, bounds: &NormalBounds) {
    let result = serde_json::to_string(bounds)
        .map_err(|e| e.to_string())
        .and_then(|json| fs::write(path, json).map_err(|e| e.to_string()));
    if let Err(e) = result {
        log::warn!(
            "ミニプレーヤーにする前の大きさを記録できませんでした: {}",
            e
        );
    }
}

fn window_error(e: tauri::Error) -> AppError {
    AppError::Io(format!("ウィンドウの大きさを変えられませんでした: {}", e))
}

/// 通常の表示での、ウィンドウの大きさの下限（`tauri.conf.json`の値）
fn normal_min_size<R: Runtime>(window: &WebviewWindow<R>) -> LogicalSize<f64> {
    let config = window.app_handle().config();
    let main = config.app.windows.first();
    LogicalSize::new(
        main.and_then(|w| w.min_width).unwrap_or(DEFAULT_MIN_SIZE.0),
        main.and_then(|w| w.min_height)
            .unwrap_or(DEFAULT_MIN_SIZE.1),
    )
}

/// ミニプレーヤーにする（すでにミニプレーヤーなら、「常に手前に表示」だけを変える）
pub fn enter<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &MiniPlayerState,
    always_on_top: bool,
) -> AppResult<()> {
    let mut normal = state.lock()?;
    if normal.is_none() {
        // フルスクリーンは、大きさを変えられないため、先に解除する
        if window.is_fullscreen().unwrap_or(false) {
            window.set_fullscreen(false).map_err(window_error)?;
        }
        let maximized = window.is_maximized().unwrap_or(false);
        if maximized {
            window.unmaximize().map_err(window_error)?;
        }
        let size = window.inner_size().map_err(window_error)?;
        let position = window.outer_position().map_err(window_error)?;
        let bounds = NormalBounds {
            width: size.width,
            height: size.height,
            x: position.x,
            y: position.y,
            maximized,
        };
        write_bounds(&state.path, &bounds);
        *normal = Some(bounds);

        let mini = LogicalSize::new(MINI_SIZE.0, MINI_SIZE.1);
        window.set_min_size(Some(mini)).map_err(window_error)?;
        window.set_size(mini).map_err(window_error)?;
        window.set_resizable(false).map_err(window_error)?;
    }
    window
        .set_always_on_top(always_on_top)
        .map_err(window_error)
}

/// 小さくする前の大きさ・位置に戻す
fn restore<R: Runtime>(window: &WebviewWindow<R>, bounds: &NormalBounds) -> AppResult<()> {
    window.set_always_on_top(false).map_err(window_error)?;
    window.set_resizable(true).map_err(window_error)?;
    window
        .set_min_size(Some(normal_min_size(window)))
        .map_err(window_error)?;
    window
        .set_size(PhysicalSize::new(bounds.width, bounds.height))
        .map_err(window_error)?;
    window
        .set_position(PhysicalPosition::new(bounds.x, bounds.y))
        .map_err(window_error)?;
    if bounds.maximized {
        window.maximize().map_err(window_error)?;
    }
    Ok(())
}

/// 通常の表示に戻す（ミニプレーヤーでなければ、何もしない）
pub fn exit<R: Runtime>(window: &WebviewWindow<R>, state: &MiniPlayerState) -> AppResult<()> {
    let mut normal = state.lock()?;
    let Some(bounds) = normal.take() else {
        return Ok(());
    };
    let _ = fs::remove_file(&state.path);
    restore(window, &bounds)
}

/// 起動時: 前回、ミニプレーヤーのまま終了していたら、元の大きさ・位置に戻す
///
/// 画面は通常の表示で始まるため、ウィンドウも通常の大きさにそろえる。
pub fn recover<R: Runtime>(window: &WebviewWindow<R>, state: &MiniPlayerState) {
    let Some(bounds) = read_bounds(&state.path) else {
        return;
    };
    let _ = fs::remove_file(&state.path);
    if let Err(e) = restore(window, &bounds) {
        log::warn!("ウィンドウを元の大きさに戻せませんでした: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muspice-mini-player-test-{}-{}",
            name,
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("mini-player.json")
    }

    #[test]
    fn test_bounds_round_trip_through_the_file() {
        let path = temp_path("round-trip");
        let bounds = NormalBounds {
            width: 2560,
            height: 1600,
            x: -120,
            y: 48,
            maximized: true,
        };

        write_bounds(&path, &bounds);

        assert_eq!(read_bounds(&path), Some(bounds));
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn test_missing_or_broken_file_is_ignored() {
        let path = temp_path("broken");
        fs::remove_file(&path).ok();
        assert_eq!(read_bounds(&path), None);

        fs::write(&path, "{ not json").unwrap();
        assert_eq!(read_bounds(&path), None);
        fs::write(&path, r#"{ "width": 100 }"#).unwrap();
        assert_eq!(read_bounds(&path), None);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn test_the_mini_size_is_smaller_than_the_normal_minimum() {
        assert!(MINI_SIZE.0 < DEFAULT_MIN_SIZE.0);
        assert!(MINI_SIZE.1 < DEFAULT_MIN_SIZE.1);
    }
}
