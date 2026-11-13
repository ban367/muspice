/**
 * 画面（WebView）で起きたエラーを、ログファイルに残す（ADR-089）
 *
 * リリースビルドでは、WebViewのコンソールを見られない。`handleError`に渡ったエラーと、どこにも
 * 受け止められなかったエラー（イベントの処理の中の例外・待っていないPromiseの失敗）を、Rust側の
 * コマンド（`log_frontend_message`）へ送り、アプリのログ（「ヘルプ」メニューの「ログのフォルダを開く」）に
 * 残す。ログの送信の失敗は、何も知らせずに捨てる（エラーの記録が、次のエラーを起こさないようにする）。
 */
import { commands } from '#lib/bindings.js';
import type { FrontendLogLevel } from '#lib/bindings.js';

/** 同じ文を続けて送らない時間（ms。描画のたびに起きるエラーなどで、ログを埋めない） */
const REPEAT_WINDOW_MS = 10_000;

let lastMessage = '';
let lastSentAt = 0;

/** エラーを、ログに残す文にする（`AppError`はコードとメッセージ、`Error`は種類・メッセージ・スタック） */
export function describeError(error: unknown): string {
  if (error instanceof Error) {
    const summary = `${error.name}: ${error.message}`;
    const stack = error.stack ?? '';
    // スタックがメッセージから始まる環境（V8）と、始まらない環境（WebKit）がある
    if (stack.startsWith(summary)) return stack;
    return stack ? `${summary}\n${stack}` : summary;
  }
  if (typeof error === 'string') return error;
  if (typeof error === 'object' && error !== null) {
    const record = error as Record<string, unknown>;
    if (typeof record.code === 'string' && typeof record.message === 'string') {
      return `[${record.code}] ${record.message}`;
    }
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  return String(error);
}

/**
 * 文を、ログファイルに残す（失敗しても何もしない）
 *
 * 直前と同じ文は、`REPEAT_WINDOW_MS`の間は送らない。Rust側でも、短い時間に多すぎる分は書かない。
 */
export function logToFile(level: FrontendLogLevel, message: string): void {
  const now = Date.now();
  if (message === lastMessage && now - lastSentAt < REPEAT_WINDOW_MS) return;
  lastMessage = message;
  lastSentAt = now;
  try {
    // モック・テストなど、Tauriのない環境では、呼び出しそのものが例外になることがある
    void commands.logFrontendMessage(level, message).catch(() => {});
  } catch {
    // 記録できなくても、元の処理は続ける
  }
}

/**
 * エラーを、ログファイルに残す
 * @param error - 起きたエラー
 * @param context - どこで起きたか（英語。ログの文の先頭に付ける）
 */
export function reportError(error: unknown, context?: string): void {
  const description = describeError(error);
  logToFile('error', context ? `${context}: ${description}` : description);
}

/**
 * どこにも受け止められなかったエラーを、ログファイルに残し始める（ウィンドウごとに、起動した時に1回呼ぶ）
 *
 * イベントの処理・タイマーの中の例外（`error`）と、待っていないPromiseの失敗（`unhandledrejection`）。
 * 画面には何も出さない（どの操作の失敗かが分からないため。利用者に見せる失敗は`handleError`を通す）。
 */
export function watchUnhandledErrors(target: Pick<Window, 'addEventListener'> = window): void {
  target.addEventListener('error', (event) => {
    const where = event.filename ? ` (${event.filename}:${event.lineno}:${event.colno})` : '';
    reportError(event.error ?? event.message, `Uncaught error${where}`);
  });
  target.addEventListener('unhandledrejection', (event) => {
    reportError(event.reason, 'Unhandled promise rejection');
  });
}

/** 直前に送った文の記録を消す（テスト用） */
export function resetErrorLogForTest(): void {
  lastMessage = '';
  lastSentAt = 0;
}
