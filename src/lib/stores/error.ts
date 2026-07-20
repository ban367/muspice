import { writable } from 'svelte/store';

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
 * バックエンド（Tauriコマンド）から返される構造化エラー
 *
 * Rust側の`AppError`が`{ code, message }`形式でシリアライズされたもの。
 * codeの一覧はsrc-tauri/src/error.rsを参照。
 */
export interface ApiError {
  code: string;
  message: string;
}

/**
 * 構造化エラーかどうかを判定する型ガード
 */
function isApiError(value: unknown): value is ApiError {
  if (typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return typeof record.code === 'string' && typeof record.message === 'string';
}

/**
 * 技術的詳細をユーザーに見せないエラーコードの汎用メッセージ
 *
 * NOT_FOUND / VALIDATION はバックエンドのメッセージ自体がユーザー向けの
 * 日本語文言のため、このマップに含めずそのまま表示する。
 */
const GENERIC_MESSAGES_BY_CODE: Record<string, string> = {
  LOCK: '処理が競合しています。しばらく待ってからもう一度お試しください。',
  DATABASE: 'データベースの操作中にエラーが発生しました。もう一度お試しください。',
  IO: 'ファイル操作中にエラーが発生しました。ファイルの状態を確認してください。',
  METADATA: 'メタデータの処理中にエラーが発生しました。ファイルが破損している可能性があります。'
};

/**
 * グローバルエラーハンドラー
 *
 * バックエンドの構造化エラーはcodeで分類してユーザー向けメッセージに変換し、
 * それ以外（フロントエンド内で発生したエラー等）はメッセージをそのまま表示する。
 */
export function handleError(error: unknown, context?: string): void {
  let message: string;

  if (isApiError(error)) {
    message = GENERIC_MESSAGES_BY_CODE[error.code] ?? error.message;
  } else if (typeof error === 'string') {
    message = error;
  } else if (error instanceof Error) {
    message = error.message;
  } else {
    message = 'エラーが発生しました';
  }

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
