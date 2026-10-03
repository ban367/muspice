import { m } from '#lib/i18n/i18n.svelte.js';
import type { AppError } from '#lib/types/models.js';

export interface ErrorNotification {
  id: string;
  message: string;
  type: 'error' | 'warning' | 'info';
  timestamp: Date;
}

/** 通知を自動で消すまでの時間（ms） */
const NOTIFICATION_DURATION_MS = 5000;

/**
 * トースト通知の一覧
 *
 * 通知の追加は`handleError` / `showSuccess` / `showWarning`を使い、
 * 表示（`Toast`）は`notifications.items`を読む。
 */
class Notifications {
  /** 表示中の通知（古い順）。中身は書き換えず配列ごと置き換える */
  items = $state.raw<ErrorNotification[]>([]);

  add(message: string, type: ErrorNotification['type'] = 'error'): void {
    const notification: ErrorNotification = {
      id: crypto.randomUUID(),
      message,
      type,
      timestamp: new Date()
    };

    this.items = [...this.items, notification];

    // 一定時間後に自動削除
    setTimeout(() => this.remove(notification.id), NOTIFICATION_DURATION_MS);
  }

  remove(id: string): void {
    this.items = this.items.filter((n) => n.id !== id);
  }

  clear(): void {
    this.items = [];
  }
}

export const notifications = new Notifications();

/**
 * 構造化エラー（Rust側の`AppError`）かどうかを判定する型ガード
 *
 * `AppError`はtauri-spectaが生成した型で、codeの一覧はそちらが正となる。
 */
function isAppError(value: unknown): value is AppError {
  if (typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return typeof record.code === 'string' && typeof record.message === 'string';
}

/**
 * エラーをユーザー向けのメッセージに変換する
 *
 * バックエンドの構造化エラーはcodeで分類してユーザー向けメッセージ（`m.errors.byCode`）に
 * 変換し、それ以外（フロントエンド内で発生したエラー等）はメッセージをそのまま使う。
 * 技術的な詳細を見せないコード（LOCK・DATABASEなど）は常に汎用メッセージにする。
 * NOT_FOUND / VALIDATION のバックエンドのメッセージは日本語のユーザー向けの文言のため、
 * 日本語ではそのまま表示し、英語では汎用メッセージにする。
 * 画面内にエラーを表示する場合も`String(error)`ではなくこれを使うこと
 * （`AppError`はオブジェクトのため、文字列化すると"[object Object]"になる）。
 */
export function toErrorMessage(error: unknown): string {
  if (isAppError(error)) {
    return m.errors.byCode[error.code] ?? error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return m.errors.unknown;
}

/**
 * グローバルエラーハンドラー
 *
 * `toErrorMessage`で変換したメッセージをトーストで通知する。
 */
export function handleError(error: unknown, context?: string): void {
  let message = toErrorMessage(error);

  // コンテキストがある場合は追加
  if (context) {
    message = m.errors.withContext(context, message);
  }

  // トーストで通知
  notifications.add(message, 'error');

  // コンソールにも出力（開発用）
  console.error('[Error]', context || '', error);
}

/**
 * 成功メッセージを表示
 */
export function showSuccess(message: string): void {
  notifications.add(message, 'info');
}

/**
 * 警告メッセージを表示
 */
export function showWarning(message: string): void {
  notifications.add(message, 'warning');
}
