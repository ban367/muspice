//! メニューバー（トレイ）への常駐
//!
//! 設定「メニューバーに常駐する」がオンの間、メニューバーにアイコンを出し、メインウィンドウを
//! 閉じてもアプリを終了しない（ウィンドウを隠すだけにして、再生を続ける）。アイコンのメニューから、
//! 再生中の曲の確認・再生の操作・ウィンドウの表示・終了ができる。
//!
//! 再生キューと再生の制御はフロントエンドが持つため、再生の操作は`PlaybackControl`イベントで
//! フロントエンドへ送る（メニューバーの「再生」メニューと同じ）。ウィンドウを隠している間も、
//! WebViewは動き続ける（`tauri.conf.json`の`backgroundThrottling`を無効にしている）。

use crate::events::PlaybackControl;
use crate::settings::{Language, SettingsState};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event;

/// メインウィンドウのラベル
pub const MAIN_WINDOW: &str = "main";

/// トレイのアイコンのID
const TRAY_ID: &str = "main";

/// メニューの項目のID（`lib.rs`の`on_menu_event`から`handle_menu_event`へ渡る）
const ITEM_TOGGLE: &str = "tray_toggle";
const ITEM_NEXT: &str = "tray_next";
const ITEM_PREVIOUS: &str = "tray_previous";
const ITEM_SHOW: &str = "tray_show";
const ITEM_QUIT: &str = "tray_quit";

/// メニューに出す曲名の長さの上限（文字数。長い曲名でメニューが広がりすぎないようにする）
const MAX_TITLE_CHARS: usize = 60;

/// メニューに出す、再生中の曲
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayNowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub playing: bool,
}

/// メニューの文言
struct TrayLabels {
    not_playing: &'static str,
    play: &'static str,
    pause: &'static str,
    next: &'static str,
    previous: &'static str,
    show: &'static str,
    quit: &'static str,
}

impl TrayLabels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Ja => Self {
                not_playing: "再生していません",
                play: "再生",
                pause: "一時停止",
                next: "次の曲",
                previous: "前の曲",
                show: "ウィンドウを表示",
                quit: "Muspice を終了",
            },
            Language::En => Self {
                not_playing: "Not Playing",
                play: "Play",
                pause: "Pause",
                next: "Next Track",
                previous: "Previous Track",
                show: "Show Window",
                quit: "Quit Muspice",
            },
        }
    }
}

/// 表示中のトレイのアイコンと、後から文言を変えるメニューの項目
struct ActiveTray<R: Runtime> {
    // 持っている間だけ、アイコンが表示される
    _icon: TrayIcon<R>,
    now_playing: MenuItem<R>,
    toggle: MenuItem<R>,
}

/// トレイの状態（Tauriの管理状態）
pub struct TrayState<R: Runtime> {
    active: Mutex<Option<ActiveTray<R>>>,
    /// 最後に伝えられた、再生中の曲（アイコンを出し直した時・言語を変えた時に、メニューへ出す）
    now_playing: Mutex<Option<TrayNowPlaying>>,
}

impl<R: Runtime> Default for TrayState<R> {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
            now_playing: Mutex::new(None),
        }
    }
}

/// 長い文字列を、上限の文字数で切る（切った場合は、末尾に「…」を付ける）
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

/// メニューの先頭に出す、再生中の曲の文言（「曲名 — アーティスト」。なければ「再生していません」）
fn now_playing_text(now_playing: Option<&TrayNowPlaying>, labels: &TrayLabels) -> String {
    match now_playing {
        None => labels.not_playing.to_string(),
        Some(track) => {
            let text = match track.artist.as_deref().filter(|artist| !artist.is_empty()) {
                Some(artist) => format!("{} — {}", track.title, artist),
                None => track.title.clone(),
            };
            truncate(&text, MAX_TITLE_CHARS)
        }
    }
}

/// 再生 / 一時停止の項目の文言（再生中は「一時停止」、それ以外は「再生」）
fn toggle_text(now_playing: Option<&TrayNowPlaying>, labels: &TrayLabels) -> &'static str {
    if now_playing.is_some_and(|track| track.playing) {
        labels.pause
    } else {
        labels.play
    }
}

/// メニューバーのアイコン（22ptの2倍の大きさ。輪の中に、再生の三角形）
///
/// 色を持たないテンプレート画像として使う（不透明度だけを見て、メニューバーの明るさに合わせて
/// OSが塗る）。画像のファイルを持たずに、ここで描く。
fn tray_icon() -> Image<'static> {
    const SIZE: usize = 44;
    let center = (SIZE as f32 - 1.0) / 2.0;
    let outer = SIZE as f32 / 2.0 - 2.0;
    let inner = outer - 3.5;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let in_ring = distance <= outer && distance >= inner;
            // 再生の三角形（右向き。重心が輪の中心に来るよう、少し左へ寄せる）
            let px = dx + 2.0;
            let in_triangle = (-6.0..=10.0).contains(&px) && dy.abs() <= (10.0 - px) * 0.56;
            if in_ring || in_triangle {
                let index = (y * SIZE + x) * 4;
                rgba[index + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

/// メニューを作る（後から文言を変える項目も返す）
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    labels: &TrayLabels,
    now_playing: Option<&TrayNowPlaying>,
) -> tauri::Result<(Menu<R>, MenuItem<R>, MenuItem<R>)> {
    // 再生中の曲は、表示するだけ（選べない）
    let now_playing_item =
        MenuItemBuilder::with_id("tray_now_playing", now_playing_text(now_playing, labels))
            .enabled(false)
            .build(app)?;
    let toggle_item =
        MenuItemBuilder::with_id(ITEM_TOGGLE, toggle_text(now_playing, labels)).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&now_playing_item)
        .separator()
        .item(&toggle_item)
        .item(&MenuItemBuilder::with_id(ITEM_NEXT, labels.next).build(app)?)
        .item(&MenuItemBuilder::with_id(ITEM_PREVIOUS, labels.previous).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(ITEM_SHOW, labels.show).build(app)?)
        .item(&MenuItemBuilder::with_id(ITEM_QUIT, labels.quit).build(app)?)
        .build()?;
    Ok((menu, now_playing_item, toggle_item))
}

/// 設定に合わせて、トレイのアイコンを出す・消す（言語を変えた時も呼び、メニューを作り直す）
///
/// アイコンの作成・削除は、メインスレッドで行う（macOSのAppKitは、メニューバーの項目をメインスレッド
/// 以外から操作すると異常終了する。コマンドは別のスレッドで動くため、ここでメインスレッドへ渡す）。
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool, language: Language) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || apply_on_main_thread(&handle, enabled, language))
    {
        log::error!("メニューバーのアイコンを切り替えられませんでした: {}", e);
    }
}

/// `apply`の本体（メインスレッドで呼ぶ）
fn apply_on_main_thread<R: Runtime>(app: &AppHandle<R>, enabled: bool, language: Language) {
    let state = app.state::<TrayState<R>>();
    let Ok(mut active) = state.active.lock() else {
        return;
    };
    // 作り直す場合も、いったん消す（メニューの文言を、今の言語にする）
    if active.take().is_some() {
        let _ = app.remove_tray_by_id(TRAY_ID);
        log::info!("メニューバーのアイコンを消しました");
    }
    if !enabled {
        return;
    }

    let labels = TrayLabels::for_language(language);
    let now_playing = state
        .now_playing
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let built = build_menu(app, &labels, now_playing.as_ref()).and_then(|(menu, now, toggle)| {
        let icon = TrayIconBuilder::with_id(TRAY_ID)
            .icon(tray_icon())
            .icon_as_template(true)
            .tooltip("Muspice")
            .menu(&menu)
            .show_menu_on_left_click(true)
            .build(app)?;
        Ok(ActiveTray {
            _icon: icon,
            now_playing: now,
            toggle,
        })
    });
    match built {
        Ok(tray) => {
            *active = Some(tray);
            log::info!("メニューバーのアイコンを表示しました");
        }
        Err(e) => log::error!("メニューバーのアイコンを作れませんでした: {}", e),
    }
}

/// 再生中の曲を、メニューへ反映する（`None`は、再生している曲がない）
pub fn set_now_playing<R: Runtime>(app: &AppHandle<R>, now_playing: Option<TrayNowPlaying>) {
    let state = app.state::<TrayState<R>>();
    {
        let Ok(mut current) = state.now_playing.lock() else {
            return;
        };
        if *current == now_playing {
            return;
        }
        *current = now_playing.clone();
    }

    let Ok(active) = state.active.lock() else {
        return;
    };
    if let Some(tray) = active.as_ref() {
        let labels = TrayLabels::for_language(crate::menu::current_language(app));
        let _ = tray
            .now_playing
            .set_text(now_playing_text(now_playing.as_ref(), &labels));
        let _ = tray
            .toggle
            .set_text(toggle_text(now_playing.as_ref(), &labels));
    }
}

/// メニューバーに常駐する設定か（読めない場合は、常駐しない）
pub fn stays_in_menu_bar<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<SettingsState>()
        .and_then(|state| state.get().ok())
        .is_some_and(|settings| settings.stay_in_menu_bar)
}

/// メインウィンドウを表示して、前面に出す（隠している・最小化している場合も）
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// トレイのメニューの項目が選ばれた時の処理（トレイの項目でなければ`false`）
pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    let control = match id {
        ITEM_TOGGLE => PlaybackControl::Toggle,
        ITEM_NEXT => PlaybackControl::Next,
        ITEM_PREVIOUS => PlaybackControl::Previous,
        ITEM_SHOW => {
            show_main_window(app);
            return true;
        }
        ITEM_QUIT => {
            app.exit(0);
            return true;
        }
        _ => return false,
    };
    if let Err(e) = control.emit(app) {
        log::warn!("再生の操作を送れませんでした: {}", e);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: Option<&str>, playing: bool) -> TrayNowPlaying {
        TrayNowPlaying {
            title: title.to_string(),
            artist: artist.map(str::to_string),
            playing,
        }
    }

    #[test]
    fn test_now_playing_text() {
        let labels = TrayLabels::for_language(Language::Ja);

        assert_eq!(now_playing_text(None, &labels), "再生していません");
        assert_eq!(
            now_playing_text(Some(&track("青い地平線", Some("Aoi Sora"), true)), &labels),
            "青い地平線 — Aoi Sora"
        );
        // アーティストがない・空の場合は、曲名だけ
        assert_eq!(
            now_playing_text(Some(&track("曲", None, true)), &labels),
            "曲"
        );
        assert_eq!(
            now_playing_text(Some(&track("曲", Some(""), false)), &labels),
            "曲"
        );
    }

    #[test]
    fn test_long_titles_are_truncated_by_characters() {
        let labels = TrayLabels::for_language(Language::Ja);
        let long = "あ".repeat(100);

        let text = now_playing_text(Some(&track(&long, Some("歌手"), true)), &labels);

        assert_eq!(text.chars().count(), MAX_TITLE_CHARS);
        assert!(text.ends_with('…'));
        assert_eq!(truncate("短い", 60), "短い");
    }

    #[test]
    fn test_toggle_text_follows_the_playing_state() {
        let ja = TrayLabels::for_language(Language::Ja);
        let en = TrayLabels::for_language(Language::En);

        assert_eq!(toggle_text(None, &ja), "再生");
        assert_eq!(toggle_text(Some(&track("曲", None, false)), &ja), "再生");
        assert_eq!(toggle_text(Some(&track("曲", None, true)), &ja), "一時停止");
        assert_eq!(toggle_text(Some(&track("曲", None, true)), &en), "Pause");
    }

    #[test]
    fn test_tray_icon_is_a_ring_with_a_triangle() {
        let icon = tray_icon();
        let (width, height) = (icon.width() as usize, icon.height() as usize);
        let alpha = |x: usize, y: usize| icon.rgba()[(y * width + x) * 4 + 3];

        assert_eq!((width, height), (44, 44));
        // 角は透明、輪の上・三角形の中は不透明、輪と三角形の間は透明
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(22, 3), 255);
        assert_eq!(alpha(22, 22), 255);
        assert_eq!(alpha(22, 8), 0);
        // 色は持たない（不透明度だけ）
        assert!(icon.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
    }
}
