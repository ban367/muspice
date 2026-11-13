//! 不具合を調べるための記録（panicと、画面（WebView）で起きたエラーを、ログファイルに残す。ADR-089）
//!
//! - Rustのpanicは、標準エラーにしか出ない。Finderから起動したアプリでは捨てられるため、panicフックで
//!   ログにも書く（裏のスレッドのpanicは、そのスレッドが止まるだけで、アプリは動き続ける）
//! - 画面のエラーは、WebViewのコンソールにしか出ない。リリースビルドでは開発者ツールを開けないため、
//!   画面からコマンドで受け取ってログに書く（ログのプラグインのJSのAPIは、WebViewに公開しない。ADR-005）

use serde::Deserialize;
use specta::Type;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// panicの内容（スレッドの名前・場所・メッセージ）を、ログにも書くようにする
///
/// 元のフック（標準エラーへの出力）は、そのまま呼ぶ。ログのプラグインを登録する前のpanicは、
/// ログには書かれない（ロガーがまだない）。
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|location| location.to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        log::error!(
            "Panic in thread '{}' at {}: {}",
            thread.name().unwrap_or("<unnamed>"),
            location,
            info.payload_as_str().unwrap_or("<non-string payload>")
        );
        previous(info);
    }));
}

/// 画面から受け取るログの重さ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FrontendLogLevel {
    Error,
    Warn,
}

/// 画面から受け取るログの1件の長さの上限（文字数。超えた分は切る）
const MAX_FRONTEND_MESSAGE_CHARS: usize = 4000;

/// 画面から受け取るログの数の上限（`FRONTEND_LOG_WINDOW`の間に、この数まで書く）
const MAX_FRONTEND_MESSAGES_PER_WINDOW: u32 = 30;

/// 画面から受け取るログの数を数える間隔
const FRONTEND_LOG_WINDOW: Duration = Duration::from_secs(60);

/// 画面から受け取るログの量の制限（Tauriの管理状態）
///
/// 同じエラーが繰り返し起きる画面（描画のたびに失敗する、など）が、ログファイルを埋めないようにする。
/// 上限を超えた分は書かず、次の間隔の最初に、書かなかった数を1行で残す。
pub struct FrontendLogLimiter(Mutex<LimiterState>);

struct LimiterState {
    window_started: Instant,
    written: u32,
    suppressed: u32,
}

/// 1件のログの扱い
#[derive(Debug, PartialEq, Eq)]
enum Admission {
    /// 書く（`suppressed`は、前の間隔で書かなかった数）
    Write { suppressed: u32 },
    /// 書かない
    Suppress,
}

impl Default for FrontendLogLimiter {
    fn default() -> Self {
        Self(Mutex::new(LimiterState {
            window_started: Instant::now(),
            written: 0,
            suppressed: 0,
        }))
    }
}

impl FrontendLogLimiter {
    fn admit(&self, now: Instant) -> Admission {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut suppressed = 0;
        if now.duration_since(state.window_started) >= FRONTEND_LOG_WINDOW {
            suppressed = state.suppressed;
            state.window_started = now;
            state.written = 0;
            state.suppressed = 0;
        }
        if state.written >= MAX_FRONTEND_MESSAGES_PER_WINDOW {
            state.suppressed += 1;
            return Admission::Suppress;
        }
        state.written += 1;
        Admission::Write { suppressed }
    }

    /// 画面から受け取ったログを、ログファイルに書く（量の上限を超えた分は書かない）
    pub fn log(&self, level: FrontendLogLevel, message: &str) {
        let Admission::Write { suppressed } = self.admit(Instant::now()) else {
            return;
        };
        if suppressed > 0 {
            log::warn!(
                target: FRONTEND_LOG_TARGET,
                "Suppressed {} frontend log messages (too many in a short time)",
                suppressed
            );
        }
        let message = sanitize_message(message);
        match level {
            FrontendLogLevel::Error => log::error!(target: FRONTEND_LOG_TARGET, "{}", message),
            FrontendLogLevel::Warn => log::warn!(target: FRONTEND_LOG_TARGET, "{}", message),
        }
    }
}

/// 画面から受け取ったログの、ログの出どころの名前
const FRONTEND_LOG_TARGET: &str = "frontend";

/// 画面から受け取った文を、ログの1行にする
///
/// 改行は` | `に、ほかの制御文字は空白にする（画面から渡された文で、ログの行を偽れないようにする）。
/// 長すぎる文は切る。
fn sanitize_message(message: &str) -> String {
    let mut line = String::new();
    for (count, c) in message.trim().chars().enumerate() {
        if count >= MAX_FRONTEND_MESSAGE_CHARS {
            line.push('…');
            break;
        }
        match c {
            '\n' => line.push_str(" | "),
            '\r' => {}
            c if c.is_control() => line.push(' '),
            c => line.push(c),
        }
    }
    line.truncate(line.trim_end().len());
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_message_keeps_one_line() {
        assert_eq!(
            sanitize_message("  TypeError: x is undefined\r\n    at f (a.js:1:2)\n\tat g\u{0}  "),
            "TypeError: x is undefined |     at f (a.js:1:2) |  at g"
        );
    }

    #[test]
    fn test_sanitize_message_truncates_long_messages() {
        let long = "あ".repeat(MAX_FRONTEND_MESSAGE_CHARS + 10);
        let line = sanitize_message(&long);
        assert_eq!(line.chars().count(), MAX_FRONTEND_MESSAGE_CHARS + 1);
        assert!(line.ends_with('…'));

        let exact = "a".repeat(MAX_FRONTEND_MESSAGE_CHARS);
        assert_eq!(sanitize_message(&exact), exact);
    }

    /// 間隔の中の上限までを書き、超えた分は書かない。次の間隔で、書かなかった数を知らせて、また書く
    #[test]
    fn test_limiter_suppresses_bursts_and_reports_them() {
        let limiter = FrontendLogLimiter::default();
        let start = Instant::now();
        for _ in 0..MAX_FRONTEND_MESSAGES_PER_WINDOW {
            assert_eq!(limiter.admit(start), Admission::Write { suppressed: 0 });
        }
        assert_eq!(limiter.admit(start), Admission::Suppress);
        assert_eq!(
            limiter.admit(start + Duration::from_secs(30)),
            Admission::Suppress
        );

        let next = start + FRONTEND_LOG_WINDOW;
        assert_eq!(limiter.admit(next), Admission::Write { suppressed: 2 });
        assert_eq!(limiter.admit(next), Admission::Write { suppressed: 0 });
    }
}
