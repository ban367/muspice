import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EMPTY_FILTERS, EMPTY_SELECTION } from '#lib/utils/trackFilter.js';

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

/** 保存した値の読み込みを確かめるため、モジュールを読み込み直す */
async function importFresh() {
  vi.resetModules();
  return (await import('./libraryBrowser.svelte.js')).libraryBrowser;
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('libraryBrowser', () => {
  it('最初は、カラムブラウザを隠し、絞り込みなし', async () => {
    const browser = await importFresh();

    expect(browser.isVisible).toBe(false);
    expect(browser.selection).toEqual(EMPTY_SELECTION);
    expect(browser.filters).toEqual(EMPTY_FILTERS);
    expect(browser.isActive).toBe(false);
  });

  it('カラムブラウザを出すかどうかを保存する', async () => {
    const browser = await importFresh();
    browser.isVisible = true;

    expect(localStorage.getItem('muspice:columnBrowserVisible')).toBe('true');
    expect((await importFresh()).isVisible).toBe(true);
  });

  it('選択・フィルタは保存しない（起動し直すと、すべての曲から始まる）', async () => {
    const browser = await importFresh();
    browser.isVisible = true;
    browser.selection = { ...EMPTY_SELECTION, genres: ['Rock'] };
    browser.filters = { ...EMPTY_FILTERS, minRating: 4 };
    expect(browser.isActive).toBe(true);

    const reopened = await importFresh();
    expect(reopened.isVisible).toBe(true);
    expect(reopened.isActive).toBe(false);
  });

  it('カラムブラウザを隠すと、選択を解除する（フィルタは残す）', async () => {
    const browser = await importFresh();
    browser.isVisible = true;
    browser.selection = { ...EMPTY_SELECTION, artists: ['A'] };
    browser.filters = { ...EMPTY_FILTERS, favoritesOnly: true };

    browser.isVisible = false;

    expect(browser.selection).toEqual(EMPTY_SELECTION);
    expect(browser.filters.favoritesOnly).toBe(true);
    expect(browser.isActive).toBe(true);
  });

  it('選択とフィルタを、まとめて解除できる', async () => {
    const browser = await importFresh();
    browser.isVisible = true;
    browser.selection = { ...EMPTY_SELECTION, albums: ['x'] };
    browser.filters = { ...EMPTY_FILTERS, yearFrom: 2000 };

    browser.clear();

    expect(browser.isActive).toBe(false);
    // カラムブラウザは出したまま
    expect(browser.isVisible).toBe(true);
  });
});
