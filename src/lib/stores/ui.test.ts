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

describe('情報パネル', () => {
  const album = {
    name: 'Album',
    sortName: null,
    artist: 'Artist',
    trackCount: 10,
    totalDuration: 2400,
    representativeTrackId: 't1'
  };
  const artist = {
    name: 'Artist',
    sortName: null,
    albumCount: 2,
    trackCount: 20,
    totalDuration: 4800,
    representativeTrackId: 't1'
  };

  it('項目を表示すると、情報パネルに切り替えて開く', async () => {
    const ui = await importFreshUi();
    ui.activeRightSidebarPanel = 'queue';

    ui.showInfo({ type: 'album', album });
    expect(ui.infoTarget).toEqual({ type: 'album', album });
    expect(ui.activeRightSidebarPanel).toBe('info');
    expect(ui.isRightSidebarExpanded).toBe(true);
    expect(ui.isInfoPanelOpen).toBe(true);
  });

  it('追随は、情報パネルを開いている時だけ項目を入れ替える', async () => {
    const ui = await importFreshUi();

    // 閉じている間は入れ替えない（開きもしない）
    ui.followInfo({ type: 'album', album });
    expect(ui.infoTarget).toBeNull();
    expect(ui.isRightSidebarExpanded).toBe(false);

    ui.showInfo({ type: 'album', album });
    ui.followInfo({ type: 'artist', artist });
    expect(ui.infoTarget).toEqual({ type: 'artist', artist });

    // 別のパネルを開いている間も入れ替えない
    ui.activeRightSidebarPanel = 'queue';
    ui.followInfo({ type: 'album', album });
    expect(ui.infoTarget).toEqual({ type: 'artist', artist });
  });

  it('情報パネルは、固定していなくても本文の横に並べる', async () => {
    const ui = await importFreshUi();
    expect(ui.isRightSidebarDocked).toBe(false);

    // 固定していないキューは本文に重ねる
    ui.activeRightSidebarPanel = 'queue';
    ui.isRightSidebarExpanded = true;
    expect(ui.isRightSidebarDocked).toBe(false);

    ui.showInfo({ type: 'album', album });
    expect(ui.isRightSidebarDocked).toBe(true);

    ui.isRightSidebarExpanded = false;
    expect(ui.isRightSidebarDocked).toBe(false);
  });

  it('固定した時は、どのパネルも本文の横に並べる', async () => {
    const ui = await importFreshUi();
    ui.activeRightSidebarPanel = 'equalizer';
    ui.isRightSidebarPinned = true;
    expect(ui.isRightSidebarDocked).toBe(true);
  });
});

describe('列幅', () => {
  // 番号の列と、曲の一覧に出せるすべての列の幅を持つ
  const DEFAULT_WIDTHS = {
    number: 48,
    title: 300,
    artist: 200,
    album: 200,
    albumArtist: 180,
    genre: 110,
    year: 56,
    trackNumber: 64,
    discNumber: 64,
    favorite: 28,
    rating: 80,
    playCount: 72,
    skipCount: 72,
    lastPlayedAt: 110,
    createdAt: 110,
    duration: 64,
    format: 64,
    bitrate: 88,
    sampleRate: 88,
    fileSize: 80,
    filePath: 360
  };

  it('保存がなければ既定値を使う（保存はしない）', async () => {
    expect((await importFreshUi()).columnWidths).toEqual(DEFAULT_WIDTHS);
    expect(localStorage.getItem('muspice:columnWidths')).toBeNull();
  });

  it('列を足す前に保存した幅（一部の列だけ）は、残りを既定値で補う', async () => {
    localStorage.setItem(
      'muspice:columnWidths',
      JSON.stringify({ number: 48, title: 360, artist: 150, rating: 80, duration: 64 })
    );

    expect((await importFreshUi()).columnWidths).toEqual({
      ...DEFAULT_WIDTHS,
      title: 360,
      artist: 150
    });
  });

  it('0以下・数値でない幅は、既定値にする', async () => {
    localStorage.setItem(
      'muspice:columnWidths',
      JSON.stringify({ title: 0, artist: -5, album: null, genre: 'wide' })
    );

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
