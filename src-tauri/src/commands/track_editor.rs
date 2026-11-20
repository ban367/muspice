//! 曲の情報の編集画面（別のウィンドウ）を開く・閉じる（ADR-094）
//!
//! 編集画面は、メインウィンドウを親にした別のウィンドウ（`TRACK_EDITOR_WINDOW`。親と一緒に動き、親より
//! 手前に出る）で開き、開いている間はメインウィンドウを操作できないようにする。メインウィンドウは、
//! `TrackEditorChanged`を受けて画面に覆いを重ね、メインウィンドウが前面に来ると、編集画面へ戻す
//! （`focus_track_editor`）。編集する曲のIDはRust側が預かり、編集画面のウィンドウは
//! `get_track_editor_tracks`で受け取る（WebViewどうしでデータを渡さない）。編集画面を閉じると、
//! 預かった曲と選んだ画像を捨て、メインウィンドウを戻す。
//!
//! 1曲の編集では、開いた一覧の曲の並びも預かり、編集画面の「前の曲」「次の曲」で、その並びの前後の曲へ
//! 移れる（`move_track_editor`。ミュージックの「情報を見る」の「<」「>」にあたる。ADR-119）。
//!
//! `set_enabled(false)`は使わない。macOSでは、メインウィンドウに半透明のシートを付けて止めるが、
//! シートは親の子のウィンドウ（編集画面）より手前に重なり、編集画面まで覆って操作できなくなる。

use super::album_art::PendingAlbumArt;
use crate::error::{AppError, AppResult};
use crate::events::TrackEditorChanged;
use crate::models::Track;
use crate::settings::Language;
use crate::state::AppState;
use crate::validation::validate_track_id;
use serde::Serialize;
use specta::Type;
use std::sync::Mutex;
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_specta::Event;

/// 編集画面のウィンドウのラベル
pub const TRACK_EDITOR_WINDOW: &str = "track-editor";

/// 編集画面のウィンドウを開く時の大きさ（幅・高さ。論理ピクセル）
///
/// 編集画面の項目（2列に並べる入力欄）とアルバムアートの見本が入り、13インチの画面にも収まる大きさ。
const WINDOW_SIZE: (f64, f64) = (720.0, 680.0);

/// 編集画面のウィンドウの最小の大きさ（入力欄が1列に詰まりすぎない大きさ）
const MIN_WINDOW_SIZE: (f64, f64) = (560.0, 480.0);

/// 一度に編集できる曲の数（ライブラリの全曲を選んでも収まる、十分に大きい数）
const MAX_TRACKS: usize = 100_000;

/// 編集画面で扱う曲（開いている間だけ持つ）
#[derive(Default)]
pub struct TrackEditorState {
    inner: Mutex<EditorTracks>,
}

/// 編集する曲と、前後の曲へ移るための一覧の並び
#[derive(Debug, Default, Clone, PartialEq)]
struct EditorTracks {
    track_ids: Vec<String>,
    /// 開いた一覧の曲の並び（1曲の編集で、その曲を含む場合だけ。ほかは空）
    sequence: Vec<String>,
}

/// 編集している曲の、一覧の並びでの位置（前後の曲へ移るボタンに使う。ADR-119）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackEditorPosition {
    /// 0から数えた位置
    pub index: u32,
    /// 一覧の曲数
    pub count: u32,
}

impl TrackEditorState {
    /// 編集する曲を預かる（`sequence`は、1曲の編集で、その曲を含む時だけ預かる）
    fn set(&self, track_ids: Vec<String>, sequence: Vec<String>) {
        let sequence = match track_ids.as_slice() {
            [track_id] if sequence.contains(track_id) => sequence,
            _ => Vec::new(),
        };
        if let Ok(mut current) = self.inner.lock() {
            *current = EditorTracks {
                track_ids,
                sequence,
            };
        }
    }

    fn get(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|inner| inner.track_ids.clone())
            .unwrap_or_default()
    }

    fn clear(&self) {
        self.set(Vec::new(), Vec::new());
    }

    /// 編集している曲の、一覧の並びでの位置（並びを預かっていなければNone）
    fn position(&self) -> Option<TrackEditorPosition> {
        let inner = self.inner.lock().ok()?;
        let [track_id] = inner.track_ids.as_slice() else {
            return None;
        };
        let index = inner.sequence.iter().position(|id| id == track_id)?;
        Some(TrackEditorPosition {
            index: u32::try_from(index).ok()?,
            count: u32::try_from(inner.sequence.len()).ok()?,
        })
    }

    /// 一覧の並びの、`offset`だけ前後の曲へ移る（`exists`がfalseを返す曲は飛ばす）
    ///
    /// 移ったらtrue。並びの端・並びを預かっていない時はfalse（編集する曲は変えない）。
    fn step(&self, offset: i32, exists: impl Fn(&str) -> bool) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let current = match inner.track_ids.as_slice() {
            [track_id] => inner.sequence.iter().position(|id| id == track_id),
            _ => None,
        };
        let (Some(mut index), true) = (current, offset != 0) else {
            return false;
        };
        let step = if offset > 0 { 1 } else { -1 };
        let mut remaining = offset.unsigned_abs();
        loop {
            let Some(next) = index.checked_add_signed(step) else {
                return false;
            };
            let Some(next_id) = inner.sequence.get(next) else {
                return false;
            };
            index = next;
            // ライブラリから外された曲は、数えずに飛ばす
            if exists(next_id) {
                remaining -= 1;
                if remaining == 0 {
                    inner.track_ids = vec![next_id.clone()];
                    return true;
                }
            }
        }
    }
}

/// 編集画面のウィンドウのタイトル（設定の言語に合わせる）
fn window_title(language: Language, track_count: usize) -> String {
    match (language, track_count) {
        (Language::Ja, 1) => "曲の情報".to_string(),
        (Language::Ja, count) => format!("{}曲の情報", count),
        (Language::En, 1) => "Song Info".to_string(),
        (Language::En, count) => format!("Info for {} Songs", count),
    }
}

fn validate_track_ids(track_ids: &[String]) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err(AppError::Validation(
            "トラックIDが指定されていません".to_string(),
        ));
    }
    if track_ids.len() > MAX_TRACKS {
        return Err(AppError::Validation(format!(
            "一度に編集できるのは{}曲までです",
            MAX_TRACKS
        )));
    }
    track_ids.iter().try_for_each(|id| validate_track_id(id))
}

/// 曲の情報の編集画面を、別のウィンドウで開く
///
/// 開いている間は、メインウィンドウを操作できないようにする（`TrackEditorChanged`）。すでに開いている
/// 場合は、編集中の内容を失わないよう、曲を入れ替えずに前面へ出す。
///
/// `sequence`は、開いた一覧の曲の並び（1曲の編集で、編集画面から前後の曲へ移るのに使う。ADR-119）。
/// 一括編集・並びのない所から開く時は、空にする。
#[tauri::command]
#[specta::specta]
pub async fn open_track_editor(
    track_ids: Vec<String>,
    sequence: Vec<String>,
    app: AppHandle,
) -> AppResult<()> {
    validate_track_ids(&track_ids)?;
    // 一覧の並びは、編集する曲数の上限とは別（大きなライブラリの全曲の一覧からも開ける）。IDの形だけを確かめる
    sequence.iter().try_for_each(|id| validate_track_id(id))?;
    super::reject_cue_tracks(&app, &track_ids)?;

    if let Some(window) = app.get_webview_window(TRACK_EDITOR_WINDOW) {
        let _ = window.set_focus();
        return Ok(());
    }
    let main = app
        .get_webview_window(crate::tray::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("メインウィンドウが見つかりません".to_string()))?;

    let title = window_title(crate::menu::current_language(&app), track_ids.len());
    app.state::<TrackEditorState>().set(track_ids, sequence);
    // 前に選んで、使わなかった画像は捨てる
    app.state::<PendingAlbumArt>().clear();

    let built = WebviewWindowBuilder::new(
        &app,
        TRACK_EDITOR_WINDOW,
        tauri::WebviewUrl::App("/track-editor".into()),
    )
    .title(title)
    .inner_size(WINDOW_SIZE.0, WINDOW_SIZE.1)
    .min_inner_size(MIN_WINDOW_SIZE.0, MIN_WINDOW_SIZE.1)
    .resizable(true)
    .minimizable(false)
    .maximizable(false)
    .focused(true)
    .parent(&main)
    .and_then(|builder| builder.build());
    if let Err(e) = built {
        app.state::<TrackEditorState>().clear();
        return Err(AppError::Io(format!("編集画面を開けませんでした: {}", e)));
    }

    // 編集している間は、メインウィンドウに覆いを重ねて操作させない（閉じた時に`on_track_editor_closed`で戻す）
    notify_editor_changed(&app, true);
    Ok(())
}

fn notify_editor_changed<R: Runtime>(app: &AppHandle<R>, open: bool) {
    if let Err(e) = (TrackEditorChanged { open }).emit(app) {
        log::warn!("Failed to emit the editor window change: {}", e);
    }
}

/// 編集画面を開いていれば、前面に出す（メインウィンドウが前面に来た時に呼ぶ。`WindowEvent::Focused`）
pub fn focus_track_editor<R: Runtime>(app: &AppHandle<R>) {
    if let Some(editor) = app.get_webview_window(TRACK_EDITOR_WINDOW) {
        let _ = editor.set_focus();
    }
}

/// 編集画面で扱う曲を、開いた時の順に返す（ライブラリから外された曲は除く）
#[tauri::command]
#[specta::specta]
pub async fn get_track_editor_tracks(
    editor: State<'_, TrackEditorState>,
    state: State<'_, AppState>,
) -> AppResult<Vec<Track>> {
    let track_ids = editor.get();
    let mut found = state.with_db(|db| crate::repository::find_tracks_by_ids(db, &track_ids))?;
    Ok(track_ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// 編集している曲の、開いた一覧の並びでの位置（並びのない編集ではnull。ADR-119）
#[tauri::command]
#[specta::specta]
pub async fn get_track_editor_position(
    editor: State<'_, TrackEditorState>,
) -> AppResult<Option<TrackEditorPosition>> {
    Ok(editor.position())
}

/// 編集する曲を、開いた一覧の並びの前（`offset`が負）・後ろの曲へ移す（ADR-119）
///
/// 移ったらtrue（編集画面は、`get_track_editor_tracks`で曲を取り直す）。並びの端ではfalse。ライブラリから
/// 外された曲は飛ばす。前の曲で選んで使わなかった画像は捨てる。
#[tauri::command]
#[specta::specta]
pub async fn move_track_editor(
    offset: i32,
    editor: State<'_, TrackEditorState>,
    state: State<'_, AppState>,
    pending_art: State<'_, PendingAlbumArt>,
) -> AppResult<bool> {
    let moved = editor.step(offset, |track_id| {
        state
            .with_db(|db| crate::repository::find_track_by_id(db, track_id))
            .is_ok()
    });
    if moved {
        pending_art.clear();
    }
    Ok(moved)
}

/// 編集画面のウィンドウが閉じた時に呼ぶ（`WindowEvent::Destroyed`）
///
/// 預かった曲と選んだ画像を捨て、メインウィンドウを操作できるように戻して、前面に出す。
pub fn on_track_editor_closed<R: Runtime>(app: &AppHandle<R>) {
    if let Some(editor) = app.try_state::<TrackEditorState>() {
        editor.clear();
    }
    if let Some(pending) = app.try_state::<PendingAlbumArt>() {
        pending.clear();
    }
    notify_editor_changed(app, false);
    if let Some(main) = app.get_webview_window(crate::tray::MAIN_WINDOW) {
        let _ = main.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(count: usize) -> Vec<String> {
        (0..count)
            .map(|n| format!("a1000000-0000-4000-8000-{:012}", n))
            .collect()
    }

    #[test]
    fn test_validate_track_ids() {
        assert!(validate_track_ids(&ids(1)).is_ok());
        assert!(validate_track_ids(&ids(3)).is_ok());
        assert!(matches!(
            validate_track_ids(&[]),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_track_ids(&["../etc/passwd".to_string()]),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn test_window_title_counts_tracks() {
        assert_eq!(window_title(Language::Ja, 1), "曲の情報");
        assert_eq!(window_title(Language::Ja, 12), "12曲の情報");
        assert_eq!(window_title(Language::En, 1), "Song Info");
        assert_eq!(window_title(Language::En, 2), "Info for 2 Songs");
    }

    #[test]
    fn test_editor_state_keeps_tracks_until_cleared() {
        let state = TrackEditorState::default();
        assert!(state.get().is_empty());
        state.set(ids(2), Vec::new());
        assert_eq!(state.get(), ids(2));
        state.clear();
        assert!(state.get().is_empty());
    }

    #[test]
    fn test_editor_moves_through_the_sequence() {
        let all = ids(5);
        let state = TrackEditorState::default();
        state.set(vec![all[1].clone()], all.clone());
        assert_eq!(
            state.position(),
            Some(TrackEditorPosition { index: 1, count: 5 })
        );

        let exists = |id: &str| id != all[3];
        assert!(state.step(1, exists));
        assert_eq!(state.get(), [all[2].clone()]);
        // ライブラリから外された曲（3番目）は飛ばす
        assert!(state.step(1, exists));
        assert_eq!(state.get(), [all[4].clone()]);
        // 端では移らない
        assert!(!state.step(1, exists));
        assert_eq!(state.get(), [all[4].clone()]);
        assert!(state.step(-2, exists));
        assert_eq!(state.get(), [all[1].clone()]);
        assert!(state.step(-1, exists));
        assert!(!state.step(-1, exists));
        assert_eq!(
            state.position(),
            Some(TrackEditorPosition { index: 0, count: 5 })
        );
    }

    #[test]
    fn test_editor_keeps_the_sequence_only_for_one_track_in_it() {
        let all = ids(3);
        let state = TrackEditorState::default();
        // 一括編集では、並びを預からない
        state.set(all[..2].to_vec(), all.clone());
        assert_eq!(state.position(), None);
        assert!(!state.step(1, |_| true));
        // 並びにない曲
        state.set(vec![ids(4)[3].clone()], all.clone());
        assert_eq!(state.position(), None);
        // 並びのない所から開いた
        state.set(vec![all[0].clone()], Vec::new());
        assert!(!state.step(1, |_| true));
    }
}
