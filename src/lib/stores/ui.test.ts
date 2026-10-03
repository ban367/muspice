import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getLastPage, saveLastPage } from './ui';

/** Node環境にはlocalStorageがないため、メモリ上の実装に差し替える */
function createMemoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => {
      items.delete(key);
    },
    setItem: (key, value) => {
      items.set(key, String(value));
    }
  };
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('前回開いていた画面', () => {
  it('記録がなければnullを返す', () => {
    expect(getLastPage()).toBeNull();
  });

  it('ライブラリ・プレイリストの画面を記録して返す', () => {
    saveLastPage('/library/albums');
    expect(getLastPage()).toBe('/library/albums');

    saveLastPage('/playlists/abc');
    expect(getLastPage()).toBe('/playlists/abc');
  });

  it('復元できない画面（ルート・設定画面）は記録しない', () => {
    saveLastPage('/library/songs');
    saveLastPage('/');
    saveLastPage('/settings');
    expect(getLastPage()).toBe('/library/songs');
  });

  it('不正な値が保存されていてもnullを返す', () => {
    localStorage.setItem('muspice:lastPage', 'https://example.com');
    expect(getLastPage()).toBeNull();
  });

  it('localStorageが使えない環境でも例外にしない', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(() => saveLastPage('/library/songs')).not.toThrow();
    expect(getLastPage()).toBeNull();
  });
});
