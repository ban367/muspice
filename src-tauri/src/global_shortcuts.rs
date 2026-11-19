//! グローバルホットキー（ほかのアプリを使っている間も効くショートカット）
//!
//! 設定「グローバルホットキーを使う」がオンの間、設定の割り当て（`Settings::global_shortcut_keys`）を
//! OSに登録する。押されたら、再生の操作を`PlaybackControl`イベントでフロントエンドへ送る（メニューバーの
//! 「再生」メニューと同じ）。登録・解除はRust側だけで行い、プラグインのJS APIはWebViewに公開しない
//! （ADR-005）。
//!
//! 割り当ては、設定の画面でキーを押して変えられる（ADR-122）。ほかのアプリが使っていて登録できなかった
//! 組み合わせは、設定画面で知らせる（`status`）。

use crate::events::PlaybackControl;
use crate::settings::GlobalShortcutKey;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_specta::Event;

/// グローバルホットキーで行う操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GlobalShortcutAction {
    /// 再生と一時停止を切り替える
    Toggle,
    /// 次の曲へ進む
    Next,
    /// 前の曲へ戻る
    Previous,
    /// 音量を1段階上げる・下げる
    VolumeUp,
    VolumeDown,
    /// ミュートを切り替える
    ToggleMute,
}

impl GlobalShortcutAction {
    /// すべての操作（設定画面に出す順）
    pub const ALL: [Self; 6] = [
        Self::Toggle,
        Self::Next,
        Self::Previous,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::ToggleMute,
    ];

    fn control(self) -> PlaybackControl {
        match self {
            Self::Toggle => PlaybackControl::Toggle,
            Self::Next => PlaybackControl::Next,
            Self::Previous => PlaybackControl::Previous,
            Self::VolumeUp => PlaybackControl::VolumeUp,
            Self::VolumeDown => PlaybackControl::VolumeDown,
            Self::ToggleMute => PlaybackControl::ToggleMute,
        }
    }
}

/// 既定の割り当て（再生 / 一時停止・次の曲・前の曲。音量とミュートは割り当てない）
///
/// 修飾キーは、どれもControl+Alt（macOSではControl+Option）。macOSで入力ソースの切り替えに
/// 使われるControl+Option+Spaceと、ウィンドウを整列するアプリがよく使うControl+Option+矢印は
/// 避けている。
pub fn default_keys() -> Vec<GlobalShortcutKey> {
    [
        (GlobalShortcutAction::Toggle, "control+alt+KeyP"),
        (GlobalShortcutAction::Next, "control+alt+Period"),
        (GlobalShortcutAction::Previous, "control+alt+Comma"),
    ]
    .into_iter()
    .map(|(action, shortcut)| GlobalShortcutKey {
        action,
        shortcut: shortcut.to_string(),
    })
    .collect()
}

/// 割り当ての文字列（`control+alt+KeyP`）を読む
///
/// Control・Alt（Option）・Super（Command）のどれかを含む組み合わせだけを認める（修飾キーのない
/// キー・Shiftだけのキーは、ほかのアプリでの入力を奪うため）。
pub fn parse_shortcut(text: &str) -> Option<Shortcut> {
    let shortcut = Shortcut::from_str(text).ok()?;
    shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
        .then_some(shortcut)
}

/// 割り当ての一覧を確かめる（読めるか・同じ操作・同じ組み合わせが重ならないか）
pub fn validate_keys(keys: &[GlobalShortcutKey]) -> Result<(), String> {
    let mut actions = Vec::new();
    let mut shortcuts = Vec::new();
    for key in keys {
        let Some(shortcut) = parse_shortcut(&key.shortcut) else {
            return Err(
                "ホットキーは、Control・Option・Commandのどれかと、ほかのキーの組み合わせにしてください"
                    .to_string(),
            );
        };
        if actions.contains(&key.action) {
            return Err("同じ操作に、2つのホットキーは割り当てられません".to_string());
        }
        if shortcuts.contains(&shortcut) {
            return Err("同じホットキーを、2つの操作に割り当てられません".to_string());
        }
        actions.push(key.action);
        shortcuts.push(shortcut);
    }
    Ok(())
}

/// 表示用の、修飾キーの名前（押す順）
fn modifier_names(mods: Modifiers) -> Vec<&'static str> {
    #[cfg(target_os = "macos")]
    let names = [
        (Modifiers::CONTROL, "Control"),
        (Modifiers::ALT, "Option"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Command"),
    ];
    #[cfg(not(target_os = "macos"))]
    let names = [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Super"),
    ];
    names
        .into_iter()
        .filter(|(modifier, _)| mods.contains(*modifier))
        .map(|(_, name)| name)
        .collect()
}

/// 表示用の、キーの名前（`KeyP` → `P`、`Digit1` → `1`、`Period` → `.`、`ArrowUp` → `↑`）
fn key_name(code: Code) -> String {
    let name = code.to_string();
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return digit.to_string();
    }
    match code {
        Code::Period => ".",
        Code::Comma => ",",
        Code::Slash => "/",
        Code::Semicolon => ";",
        Code::Quote => "'",
        Code::BracketLeft => "[",
        Code::BracketRight => "]",
        Code::Backslash => "\\",
        Code::Minus => "-",
        Code::Equal => "=",
        Code::Backquote => "`",
        Code::ArrowUp => "↑",
        Code::ArrowDown => "↓",
        Code::ArrowLeft => "←",
        Code::ArrowRight => "→",
        _ => return name,
    }
    .to_string()
}

/// グローバルホットキーの割り当てと、登録の状態（設定画面に出す）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GlobalShortcutInfo {
    pub action: GlobalShortcutAction,
    /// キーの名前（押す順。例: `["Control", "Option", "P"]`。割り当てがなければ空）
    pub keys: Vec<String>,
    /// 割り当て（`control+alt+KeyP`の形。割り当てがなければnull）
    pub shortcut: Option<String>,
    /// OSに登録できているか（設定がオフの間・割り当てがない操作は`false`）
    pub registered: bool,
}

/// 登録した割り当て（操作・組み合わせ・登録できたか）
#[derive(Default)]
pub struct GlobalShortcutState {
    bindings: Mutex<Vec<(GlobalShortcutAction, Shortcut, bool)>>,
}

/// 押された組み合わせに割り当てた操作
fn action_for(
    bindings: &[(GlobalShortcutAction, Shortcut, bool)],
    shortcut: &Shortcut,
) -> Option<GlobalShortcutAction> {
    bindings
        .iter()
        .find(|(_, bound, registered)| *registered && bound == shortcut)
        .map(|(action, _, _)| *action)
}

/// 割り当ての一覧（すべての操作。登録の状態を付ける）
fn infos(bindings: &[(GlobalShortcutAction, Shortcut, bool)]) -> Vec<GlobalShortcutInfo> {
    GlobalShortcutAction::ALL
        .into_iter()
        .map(
            |action| match bindings.iter().find(|(bound, _, _)| *bound == action) {
                Some((_, shortcut, registered)) => GlobalShortcutInfo {
                    action,
                    keys: modifier_names(shortcut.mods)
                        .into_iter()
                        .map(str::to_string)
                        .chain(std::iter::once(key_name(shortcut.key)))
                        .collect(),
                    shortcut: Some(shortcut.into_string()),
                    registered: *registered,
                },
                None => GlobalShortcutInfo {
                    action,
                    keys: Vec::new(),
                    shortcut: None,
                    registered: false,
                },
            },
        )
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
            let action = app.try_state::<GlobalShortcutState>().and_then(|state| {
                state
                    .bindings
                    .lock()
                    .ok()
                    .and_then(|bindings| action_for(&bindings, shortcut))
            });
            if let Some(action) = action
                && let Err(e) = action.control().emit(app)
            {
                log::warn!("Failed to send playback control: {}", e);
            }
        })
        .build()
}

/// 設定に合わせて、組み合わせを登録し直す（オフなら、登録をすべて解除する）
///
/// 読めない割り当ては飛ばす（保存する時に確かめているため、ほかの版で保存した値など）。
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool, keys: &[GlobalShortcutKey]) {
    let state = app.state::<GlobalShortcutState>();
    let Ok(mut bindings) = state.bindings.lock() else {
        return;
    };
    let shortcuts = app.global_shortcut();

    for (_, shortcut, registered) in bindings.iter() {
        if *registered && let Err(e) = shortcuts.unregister(*shortcut) {
            log::warn!(
                "Failed to unregister global hotkey ({}): {}",
                shortcut.into_string(),
                e
            );
        }
    }
    *bindings = keys
        .iter()
        .filter_map(|key| Some((key.action, parse_shortcut(&key.shortcut)?)))
        .map(|(action, shortcut)| {
            let registered = enabled
                && match shortcuts.register(shortcut) {
                    Ok(()) => true,
                    // ほかのアプリが使っているなど（設定画面で知らせる）
                    Err(e) => {
                        log::warn!(
                            "Failed to register global hotkey ({}): {}",
                            shortcut.into_string(),
                            e
                        );
                        false
                    }
                };
            (action, shortcut, registered)
        })
        .collect();
}

/// 割り当てと、登録の状態
pub fn status<R: Runtime>(app: &AppHandle<R>) -> Vec<GlobalShortcutInfo> {
    let bindings = app
        .state::<GlobalShortcutState>()
        .bindings
        .lock()
        .map(|bindings| bindings.clone())
        .unwrap_or_default();
    infos(&bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(action: GlobalShortcutAction, shortcut: &str) -> GlobalShortcutKey {
        GlobalShortcutKey {
            action,
            shortcut: shortcut.to_string(),
        }
    }

    fn bindings(keys: &[GlobalShortcutKey]) -> Vec<(GlobalShortcutAction, Shortcut, bool)> {
        keys.iter()
            .map(|key| (key.action, parse_shortcut(&key.shortcut).unwrap(), true))
            .collect()
    }

    #[test]
    fn test_default_keys_are_valid_and_avoid_system_combinations() {
        let keys = default_keys();
        assert!(validate_keys(&keys).is_ok());
        for key in &keys {
            let shortcut = parse_shortcut(&key.shortcut).unwrap();
            assert_eq!(shortcut.mods, Modifiers::CONTROL | Modifiers::ALT);
            // 入力ソースの切り替え（Control+Option+Space）・ウィンドウの整列（Control+Option+矢印）
            assert!(![Code::Space, Code::ArrowLeft, Code::ArrowRight].contains(&shortcut.key));
        }
    }

    #[test]
    fn test_parse_shortcut_requires_a_modifier() {
        assert!(parse_shortcut("control+alt+KeyP").is_some());
        assert!(parse_shortcut("super+shift+F5").is_some());
        // 修飾キーなし・Shiftだけ・読めない文字列
        assert!(parse_shortcut("KeyP").is_none());
        assert!(parse_shortcut("shift+KeyP").is_none());
        assert!(parse_shortcut("control+").is_none());
        assert!(parse_shortcut("control+alt+Nope").is_none());
        // 文字列は、読んだ形に戻せる
        assert_eq!(
            parse_shortcut("Ctrl+Option+P").unwrap().into_string(),
            "control+alt+KeyP"
        );
    }

    #[test]
    fn test_validate_keys_rejects_duplicates() {
        use GlobalShortcutAction::*;
        assert!(validate_keys(&[]).is_ok());
        assert!(
            validate_keys(&[
                key(Toggle, "control+alt+KeyP"),
                key(Next, "control+alt+KeyP")
            ])
            .is_err()
        );
        assert!(
            validate_keys(&[
                key(Toggle, "control+alt+KeyP"),
                key(Toggle, "control+alt+KeyO")
            ])
            .is_err()
        );
        assert!(validate_keys(&[key(VolumeUp, "KeyU")]).is_err());
    }

    #[test]
    fn test_each_binding_maps_to_its_action() {
        let keys = default_keys();
        let bound = bindings(&keys);
        for key in &keys {
            let shortcut = parse_shortcut(&key.shortcut).unwrap();
            assert_eq!(action_for(&bound, &shortcut), Some(key.action));
        }
        // 割り当てのない組み合わせ・登録できなかった組み合わせは、操作なし
        assert_eq!(
            action_for(&bound, &Shortcut::new(Some(Modifiers::CONTROL), Code::KeyP)),
            None
        );
        let mut not_registered = bound.clone();
        not_registered[0].2 = false;
        assert_eq!(
            action_for(&not_registered, &parse_shortcut(&keys[0].shortcut).unwrap()),
            None
        );
    }

    #[test]
    fn test_infos_list_every_action_with_the_keys_and_the_registration_state() {
        let mut bound = bindings(&default_keys());
        bound[1].2 = false;
        let list = infos(&bound);

        assert_eq!(
            list.iter().map(|info| info.action).collect::<Vec<_>>(),
            GlobalShortcutAction::ALL
        );
        assert_eq!(list[0].keys.len(), 3);
        assert_eq!(list[0].keys[2], "P");
        assert_eq!(list[0].shortcut.as_deref(), Some("control+alt+KeyP"));
        assert_eq!(list[1].keys[2], ".");
        assert_eq!(list[2].keys[2], ",");
        assert_eq!(
            list.iter().map(|info| info.registered).collect::<Vec<_>>(),
            [true, false, true, false, false, false]
        );
        // 割り当てのない操作
        assert!(list[3].keys.is_empty() && list[3].shortcut.is_none());
    }

    #[test]
    fn test_key_names() {
        assert_eq!(key_name(Code::KeyA), "A");
        assert_eq!(key_name(Code::Digit7), "7");
        assert_eq!(key_name(Code::ArrowUp), "↑");
        assert_eq!(key_name(Code::F5), "F5");
        assert_eq!(key_name(Code::Space), "Space");
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
        assert_eq!(
            GlobalShortcutAction::VolumeUp.control(),
            PlaybackControl::VolumeUp
        );
        assert_eq!(
            GlobalShortcutAction::ToggleMute.control(),
            PlaybackControl::ToggleMute
        );
    }
}
