import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
  return import('./playlistSidebar.svelte.js');
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('playlistSidebar', () => {
  it('最初は、作成日の新しい順で、どのフォルダも開いている', async () => {
    const { playlistSidebar } = await importFresh();

    expect(playlistSidebar.sort).toBe('createdAt');
    expect(playlistSidebar.isCollapsed('f1')).toBe(false);
  });

  it('並び順と、閉じているフォルダを保存し、次に読み込んだ時に戻す', async () => {
    const first = (await importFresh()).playlistSidebar;
    first.sort = 'manual';
    first.toggleFolder('f1');
    first.toggleFolder('f2');
    first.toggleFolder('f2');

    const second = (await importFresh()).playlistSidebar;
    expect(second.sort).toBe('manual');
    expect(second.isCollapsed('f1')).toBe(true);
    expect(second.isCollapsed('f2')).toBe(false);

    second.setCollapsed('f1', false);
    expect((await importFresh()).playlistSidebar.isCollapsed('f1')).toBe(false);
  });

  it('保存してある値が読めない場合は、既定に戻す', async () => {
    localStorage.setItem('muspice:playlistSort', '"rating"');
    localStorage.setItem('muspice:collapsedPlaylistFolders', '{broken');

    const { playlistSidebar } = await importFresh();
    expect(playlistSidebar.sort).toBe('createdAt');
    expect(playlistSidebar.isCollapsed('f1')).toBe(false);
  });

  it('localStorageが使えなくても動く', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('unavailable');
      },
      setItem: () => {
        throw new Error('unavailable');
      }
    });

    const { playlistSidebar } = await importFresh();
    playlistSidebar.sort = 'name';
    playlistSidebar.toggleFolder('f1');
    expect(playlistSidebar.sort).toBe('name');
    expect(playlistSidebar.isCollapsed('f1')).toBe(true);
  });
});

describe('playlistInfoDialog', () => {
  it('開く・閉じる', async () => {
    const { playlistInfoDialog } = await importFresh();

    expect(playlistInfoDialog.playlistId).toBeNull();
    playlistInfoDialog.open('p1');
    expect(playlistInfoDialog.playlistId).toBe('p1');
    playlistInfoDialog.close();
    expect(playlistInfoDialog.playlistId).toBeNull();
  });
});
