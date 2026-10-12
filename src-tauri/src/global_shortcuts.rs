//! グローバルホットキー（ほかのアプリを使っている間も効くショートカット）
//!
//! 設定「グローバルホットキーを使う」がオンの間、決まった組み合わせをOSに登録する。押されたら、
//! 再生の操作を`PlaybackControl`イベントでフロントエンドへ送る（メニューバーの「再生」メニューと
//! 同じ）。登録・解除はRust側だけで行い、プラグインのJS APIはWebViewに公開しない（ADR-005）。
//!
//! 割り当ては変えられない。ほかのアプリが使っていて登録できなかった組み合わせは、設定画面で
//! 知らせる（`status`）。

use crate::events::PlaybackControl;
use serde::Serialize;
use specta::Type;
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_specta::Event;

/// グローバルホットキーで行う操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GlobalShortcutAction {
    /// 再生と一時停止を切り替える
    Toggle,
    /// 次の曲へ進む
    Next,
    /// 前の曲へ戻る
    Previous,
}

impl GlobalShortcutAction {
    fn control(self) -> PlaybackControl {
        match self {
            Self::Toggle => PlaybackControl::Toggle,
            Self::Next => PlaybackControl::Next,
            Self::Previous => PlaybackControl::Previous,
        }
    }
}

/// 割り当て（操作・キー・表示用のキーの名前）
///
/// 修飾キーは、どれもControl+Alt（macOSではControl+Option）。macOSで入力ソースの切り替えに
/// 使われるControl+Option+Spaceと、ウィンドウを整列するアプリがよく使うControl+Option+矢印は
/// 避けている。
const BINDINGS: [(GlobalShortcutAction, Code, &str); 3] = [
    (GlobalShortcutAction::Toggle, Code::KeyP, "P"),
    (GlobalShortcutAction::Next, Code::Period, "."),
    (GlobalShortcutAction::Previous, Code::Comma, ","),
];

/// 表示用の、修飾キーの名前
#[cfg(target_os = "macos")]
const MODIFIER_NAMES: [&str; 2] = ["Control", "Option"];
#[cfg(not(target_os = "macos"))]
const MODIFIER_NAMES: [&str; 2] = ["Ctrl", "Alt"];

fn shortcut_for(code: Code) -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), code)
}

/// 押された組み合わせに割り当てた操作
fn action_for(shortcut: &Shortcut) -> Option<GlobalShortcutAction> {
    BINDINGS
        .iter()
        .find(|(_, code, _)| shortcut_for(*code) == *shortcut)
        .map(|(action, _, _)| *action)
}

/// グローバルホットキーの割り当てと、登録の状態（設定画面に出す）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GlobalShortcutInfo {
    pub action: GlobalShortcutAction,
    /// キーの名前（押す順。例: `["Control", "Option", "P"]`）
    pub keys: Vec<String>,
    /// OSに登録できているか（設定がオフの間は、どれも`false`）
    pub registered: bool,
}

/// 登録の状態（Tauriの管理状態。`BINDINGS`と同じ順）
#[derive(Default)]
pub struct GlobalShortcutState {
    registered: Mutex<[bool; BINDINGS.len()]>,
}

/// 割り当ての一覧（登録の状態を付ける）
fn infos(registered: &[bool; BINDINGS.len()]) -> Vec<GlobalShortcutInfo> {
    BINDINGS
        .iter()
        .zip(registered)
        .map(|((action, _, key), registered)| GlobalShortcutInfo {
            action: *action,
            keys: MODIFIER_NAMES
                .iter()
                .chain(std::iter::once(key))
                .map(|name| name.to_string())
                .collect(),
            registered: *registered,
        })
        .collect()
}

/// プラグイン（押された時の処理を登録する。組み合わせの登録は`apply`で行う）
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            // 押した時だけ（離した時は何もしない）
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Some(action) = action_for(shortcut)
                && let Err(e) = action.control().emit(app)
            {
                log::warn!("再生の操作を送れませんでした: {}", e);
            }
        })
        .build()
}

/// 設定に合わせて、組み合わせを登録する・解除する
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    let state = app.state::<GlobalShortcutState>();
    let Ok(mut registered) = state.registered.lock() else {
        return;
    };
    let shortcuts = app.global_shortcut();

    for (index, (_, code, key)) in BINDINGS.iter().enumerate() {
        let shortcut = shortcut_for(*code);
        if registered[index] == enabled {
            continue;
        }
        if enabled {
            match shortcuts.register(shortcut) {
                Ok(()) => registered[index] = true,
                // ほかのアプリが使っているなど（設定画面で知らせる）
                Err(e) => log::warn!(
                    "グローバルホットキー（{}）を登録できませんでした: {}",
                    key,
                    e
                ),
            }
        } else {
            if let Err(e) = shortcuts.unregister(shortcut) {
                log::warn!(
                    "グローバルホットキー（{}）を解除できませんでした: {}",
                    key,
                    e
                );
            }
            registered[index] = false;
        }
    }
}

/// 割り当てと、登録の状態
pub fn status<R: Runtime>(app: &AppHandle<R>) -> Vec<GlobalShortcutInfo> {
    let registered = app
        .state::<GlobalShortcutState>()
        .registered
        .lock()
        .map(|registered| *registered)
        .unwrap_or_default();
    infos(&registered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_binding_maps_to_its_action() {
        for (action, code, _) in BINDINGS {
            assert_eq!(action_for(&shortcut_for(code)), Some(action));
        }
        // 修飾キーの違う組み合わせ・割り当てのないキーは、操作なし
        assert_eq!(
            action_for(&Shortcut::new(Some(Modifiers::CONTROL), Code::KeyP)),
            None
        );
        assert_eq!(action_for(&shortcut_for(Code::KeyQ)), None);
    }

    #[test]
    fn test_bindings_avoid_combinations_used_by_the_system() {
        let codes: Vec<Code> = BINDINGS.iter().map(|(_, code, _)| *code).collect();

        // 入力ソースの切り替え（Control+Option+Space）・ウィンドウの整列（Control+Option+矢印）
        for reserved in [Code::Space, Code::ArrowLeft, Code::ArrowRight] {
            assert!(!codes.contains(&reserved));
        }
        // 同じキーを2つの操作に割り当てない
        let mut unique = codes.clone();
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
    }

    #[test]
    fn test_infos_list_the_keys_and_the_registration_state() {
        let list = infos(&[true, false, true]);

        assert_eq!(list.len(), 3);
        assert_eq!(list[0].action, GlobalShortcutAction::Toggle);
        assert_eq!(list[0].keys.len(), 3);
        assert_eq!(list[0].keys[2], "P");
        assert_eq!(list[1].keys[2], ".");
        assert_eq!(list[2].keys[2], ",");
        assert_eq!(
            list.iter().map(|info| info.registered).collect::<Vec<_>>(),
            [true, false, true]
        );
    }

    #[test]
    fn test_actions_map_to_playback_controls() {
        assert_eq!(
            GlobalShortcutAction::Toggle.control(),
            PlaybackControl::Toggle
        );
        assert_eq!(GlobalShortcutAction::Next.control(), PlaybackControl::Next);
        assert_eq!(
            GlobalShortcutAction::Previous.control(),
            PlaybackControl::Previous
        );
    }
}
