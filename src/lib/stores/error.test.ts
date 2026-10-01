import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { errorStore, handleError, showSuccess } from './error';

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  errorStore.clear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

/** 直近の通知 */
function lastNotification() {
  const notifications = get(errorStore);
  return notifications[notifications.length - 1];
}

describe('handleError', () => {
  it('技術的なエラーコードは汎用メッセージに置き換える', () => {
    handleError({ code: 'DATABASE', message: 'SQLITE_BUSY: database is locked' });

    expect(lastNotification().message).toBe(
      'データベースの操作中にエラーが発生しました。もう一度お試しください。'
    );
    expect(lastNotification().type).toBe('error');
  });

  it('NOT_FOUND・VALIDATIONはバックエンドのメッセージをそのまま表示する', () => {
    handleError({ code: 'VALIDATION', message: 'プレイリスト名を入力してください' });
    expect(lastNotification().message).toBe('プレイリスト名を入力してください');

    handleError({ code: 'NOT_FOUND', message: '指定されたトラックが見つかりません' });
    expect(lastNotification().message).toBe('指定されたトラックが見つかりません');
  });

  it('文字列・Errorはメッセージを使い、コンテキストを前に付ける', () => {
    handleError('通信に失敗しました', 'トラック一覧の取得');
    expect(lastNotification().message).toBe('トラック一覧の取得: 通信に失敗しました');

    handleError(new Error('boom'));
    expect(lastNotification().message).toBe('boom');
  });

  it('不明な値は既定のメッセージにする', () => {
    handleError(42);
    expect(lastNotification().message).toBe('エラーが発生しました');
  });
});

describe('errorStore', () => {
  it('通知は5秒後に自動で消える', () => {
    showSuccess('保存しました');
    expect(get(errorStore)).toHaveLength(1);
    expect(lastNotification().type).toBe('info');

    vi.advanceTimersByTime(4999);
    expect(get(errorStore)).toHaveLength(1);

    vi.advanceTimersByTime(1);
    expect(get(errorStore)).toHaveLength(0);
  });
});
