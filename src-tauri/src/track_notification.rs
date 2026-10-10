//! 曲が変わった時の通知
//!
//! 設定「曲が変わった時に通知する」がオンの間、再生する曲が変わったら、OSの通知で曲名と
//! アーティストを知らせる。メインウィンドウが前面にある間は出さない（見れば分かるため）。
//! 通知はRust側から出し、プラグインのJS APIはWebViewに公開しない（ADR-005）。

use crate::models::Track;
use crate::settings::SettingsState;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

/// 最後に再生していた曲（Tauriの管理状態。曲が変わったかどうかの判定に使う）
#[derive(Default)]
pub struct TrackNotifier {
    last_track_id: Mutex<Option<String>>,
}

impl TrackNotifier {
    /// 再生している曲を記録し、前の曲から変わったかを返す
    ///
    /// 一時停止中の曲（起動して復元しただけの曲など）は記録しない。一時停止と再開・シークでは、
    /// 変わったことにならない。`None`（再生している曲がない）で、記録を消す。
    pub fn observe(&self, playing_track_id: Option<&str>, playing: bool) -> bool {
        let Ok(mut last) = self.last_track_id.lock() else {
            return false;
        };
        match playing_track_id {
            None => {
                *last = None;
                false
            }
            Some(_) if !playing => false,
            Some(track_id) if last.as_deref() == Some(track_id) => false,
            Some(track_id) => {
                *last = Some(track_id.to_string());
                true
            }
        }
    }
}

/// 通知の本文（「アーティスト — アルバム」。あるものだけを並べる）
fn notification_body(artist: Option<&str>, album: Option<&str>) -> String {
    [artist, album]
        .into_iter()
        .flatten()
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" — ")
}

/// 通知の見出し（曲名。なければファイル名）
fn notification_title<'a>(title: Option<&'a str>, file_name: &'a str) -> &'a str {
    title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(file_name)
}

/// 曲が変わったことを、OSの通知で知らせる（設定がオフ・メインウィンドウが前面にある場合は出さない）
pub fn notify_track_change<R: Runtime>(app: &AppHandle<R>, track: &Track) {
    let enabled = app
        .try_state::<SettingsState>()
        .and_then(|state| state.get().ok())
        .is_some_and(|settings| settings.notify_track_change);
    if !enabled {
        return;
    }
    let is_focused = app
        .get_webview_window(crate::tray::MAIN_WINDOW)
        .is_some_and(|window| window.is_focused().unwrap_or(false));
    if is_focused {
        return;
    }

    let title = notification_title(track.title.as_deref(), &track.file_name);
    let mut notification = app.notification().builder().title(title);
    let body = notification_body(track.artist.as_deref(), track.album.as_deref());
    if !body.is_empty() {
        notification = notification.body(body);
    }
    if let Err(e) = notification.show() {
        log::warn!("曲の変更の通知を出せませんでした: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observe_reports_a_change_only_when_a_new_track_starts_playing() {
        let notifier = TrackNotifier::default();

        // 起動して復元しただけの曲（一時停止中）は、変わったことにしない
        assert!(!notifier.observe(Some("a"), false));
        assert!(notifier.observe(Some("a"), true));
        // 一時停止・再開・シークでは、変わったことにならない
        assert!(!notifier.observe(Some("a"), false));
        assert!(!notifier.observe(Some("a"), true));
        assert!(notifier.observe(Some("b"), true));
        // 再生を終えてから同じ曲を再生し直したら、もう一度知らせる
        assert!(!notifier.observe(None, false));
        assert!(notifier.observe(Some("b"), true));
    }

    #[test]
    fn test_notification_text() {
        assert_eq!(notification_title(Some("曲"), "file.mp3"), "曲");
        assert_eq!(
            notification_body(Some("歌手"), Some("アルバム")),
            "歌手 — アルバム"
        );

        // ないものは並べない。曲名がなければファイル名
        assert_eq!(notification_title(None, "file.mp3"), "file.mp3");
        assert_eq!(notification_title(Some("  "), "file.mp3"), "file.mp3");
        assert_eq!(notification_body(Some("歌手"), None), "歌手");
        assert_eq!(notification_body(None, Some(" ")), "");
    }
}
