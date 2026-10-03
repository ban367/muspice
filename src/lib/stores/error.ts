import { writable } from 'svelte/store';
import type { AppError } from '#lib/types/models.js';

export interface ErrorNotification {
  id: string;
  message: string;
  type: 'error' | 'warning' | 'info';
  timestamp: Date;
}

function createErrorStore() {
  const { subscribe, update } = writable<ErrorNotification[]>([]);

  return {
    subscribe,
    addError: (message: string, type: 'error' | 'warning' | 'info' = 'error') => {
      const notification: ErrorNotification = {
        id: crypto.randomUUID(),
        message,
        type,
        timestamp: new Date()
      };

      update((errors) => [...errors, notification]);

      // 5秒後に自動削除
      setTimeout(() => {
        update((errors) => errors.filter((e) => e.id !== notification.id));
      }, 5000);
    },
    removeError: (id: string) => {
      update((errors) => errors.filter((e) => e.id !== id));
    },
    clear: () => {
      update(() => []);
    }
  };
}

export const errorStore = createErrorStore();

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
 * 技術的詳細をユーザーに見せないエラーコードの汎用メッセージ
 *
 * NOT_FOUND / VALIDATION はバックエンドのメッセージ自体がユーザー向けの
 * 日本語文言のため、このマップに含めずそのまま表示する。
 * （Partialにより、コードを追加してもここへの追加は任意）
 */
const GENERIC_MESSAGES_BY_CODE: Partial<Record<AppError['code'], string>> = {
  LOCK: '処理が競合しています。しばらく待ってからもう一度お試しください。',
  DATABASE: 'データベースの操作中にエラーが発生しました。もう一度お試しください。',
  IO: 'ファイル操作中にエラーが発生しました。ファイルの状態を確認してください。',
  METADATA: 'メタデータの処理中にエラーが発生しました。ファイルが破損している可能性があります。'
};

/**
 * エラーをユーザー向けのメッセージに変換する
 *
 * バックエンドの構造化エラーはcodeで分類してユーザー向けメッセージに変換し、
 * それ以外（フロントエンド内で発生したエラー等）はメッセージをそのまま使う。
 * 画面内にエラーを表示する場合も`String(error)`ではなくこれを使うこと
 * （`AppError`はオブジェクトのため、文字列化すると"[object Object]"になる）。
 */
export function toErrorMessage(error: unknown): string {
  if (isAppError(error)) {
    return GENERIC_MESSAGES_BY_CODE[error.code] ?? error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return 'エラーが発生しました';
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
    message = `${context}: ${message}`;
  }

  // エラーストアに追加
  errorStore.addError(message, 'error');

  // コンソールにも出力（開発用）
  console.error('[Error]', context || '', error);
}

/**
 * 成功メッセージを表示
 */
export function showSuccess(message: string): void {
  errorStore.addError(message, 'info');
}

/**
 * 警告メッセージを表示
 */
export function showWarning(message: string): void {
  errorStore.addError(message, 'warning');
}
