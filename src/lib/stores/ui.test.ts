import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getLastPage, saveLastPage } from './ui.svelte.js';

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

/** localStorageを差し替えた後で読み込み直す（保存値からの初期化はモジュールの読み込み時に行うため） */
async function importFreshUi() {
  vi.resetModules();
  return (await import('./ui.svelte.js')).ui;
}

describe('右サイドバーの固定', () => {
  it('保存した値で初期化する', async () => {
    expect((await importFreshUi()).isRightSidebarPinned).toBe(false);

    localStorage.setItem('muspice:rightSidebarPinned', 'true');
    expect((await importFreshUi()).isRightSidebarPinned).toBe(true);
  });

  it('固定すると保存し、サイドバーを展開する', async () => {
    const ui = await importFreshUi();

    ui.isRightSidebarPinned = true;
    expect(ui.isRightSidebarExpanded).toBe(true);
    expect(localStorage.getItem('muspice:rightSidebarPinned')).toBe('true');

    // 解除しても展開状態は変えない
    ui.isRightSidebarPinned = false;
    expect(ui.isRightSidebarExpanded).toBe(true);
    expect(localStorage.getItem('muspice:rightSidebarPinned')).toBe('false');
  });
});

describe('列幅', () => {
  const DEFAULT_WIDTHS = { number: 48, title: 300, artist: 200, rating: 80, duration: 64 };

  it('保存がなければ既定値を使う', async () => {
    expect((await importFreshUi()).columnWidths).toEqual(DEFAULT_WIDTHS);
  });

  it('古いキーを除き、ない列・不正な値は既定値で補って保存し直す', async () => {
    localStorage.setItem(
      'muspice:columnWidths',
      JSON.stringify({ checkbox: 32, status: 24, title: 420, artist: 'wide' })
    );

    const ui = await importFreshUi();
    const expected = { ...DEFAULT_WIDTHS, title: 420 };
    expect(ui.columnWidths).toEqual(expected);
    expect(JSON.parse(localStorage.getItem('muspice:columnWidths') ?? 'null')).toEqual(expected);
  });

  it('壊れた値が保存されていても既定値を使う', async () => {
    localStorage.setItem('muspice:columnWidths', '{');
    expect((await importFreshUi()).columnWidths).toEqual(DEFAULT_WIDTHS);
  });

  it('代入すると保存する', async () => {
    const ui = await importFreshUi();

    ui.columnWidths = { ...ui.columnWidths, artist: 260 };
    expect(ui.columnWidths.artist).toBe(260);
    expect(JSON.parse(localStorage.getItem('muspice:columnWidths') ?? 'null')).toEqual({
      ...DEFAULT_WIDTHS,
      artist: 260
    });
  });
});
