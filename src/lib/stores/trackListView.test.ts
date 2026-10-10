import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  TrackListView,
  forgetTrackListView,
  playlistSortViewId,
  trackListView
} from './trackListView.svelte.js';

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

const stored = (id: string) =>
  JSON.parse(localStorage.getItem(`muspice:trackListView:${id}`) ?? 'null');

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('TrackListView', () => {
  it('保存がなければ、既定の列・並び順を使う', () => {
    const view = new TrackListView('a');

    expect(view.columns).toEqual(['title', 'artist', 'favorite', 'rating', 'duration']);
    expect(view.sort).toEqual({ field: 'createdAt', direction: 'desc' });
    expect(view.isDefault).toBe(true);
    // 既定のままなら、保存しない
    expect(stored('a')).toBeNull();
  });

  it('画面ごとの既定を受け取る（並び順のnullは、渡された順のまま）', () => {
    const view = new TrackListView('b', { columns: ['title', 'playCount'], sort: null });

    expect(view.columns).toEqual(['title', 'playCount']);
    expect(view.sort).toBeNull();
    expect(view.isDefault).toBe(true);
  });

  it('列・並び順を変えると保存し、次に開いた時に使う', () => {
    const view = new TrackListView('c');
    view.columns = ['title', 'album', 'year'];
    view.sort = { field: 'year', direction: 'asc' };

    expect(stored('c')).toEqual({
      columns: ['title', 'album', 'year'],
      sort: { field: 'year', direction: 'asc' }
    });
    expect(view.isDefault).toBe(false);

    const reopened = new TrackListView('c');
    expect(reopened.columns).toEqual(['title', 'album', 'year']);
    expect(reopened.sort).toEqual({ field: 'year', direction: 'asc' });
  });

  it('「渡された順」を選んだことも保存する', () => {
    const view = new TrackListView('d');
    view.sort = null;

    expect(new TrackListView('d').sort).toBeNull();
  });

  it('画面ごとに別々に覚える', () => {
    new TrackListView('e1').columns = ['title'];

    expect(new TrackListView('e2').columns).toHaveLength(5);
    expect(new TrackListView('e1').columns).toEqual(['title']);
  });

  it('壊れた保存・知らない列・不正な並び順は、既定で補う', () => {
    localStorage.setItem('muspice:trackListView:f1', '{');
    localStorage.setItem(
      'muspice:trackListView:f2',
      JSON.stringify({ columns: ['status', 'album'], sort: { field: 'status', direction: 'asc' } })
    );
    localStorage.setItem(
      'muspice:trackListView:f3',
      JSON.stringify({ columns: 'title', sort: { field: 'year', direction: 'up' } })
    );

    expect(new TrackListView('f1').isDefault).toBe(true);
    const f2 = new TrackListView('f2');
    expect(f2.columns).toEqual(['album']);
    expect(f2.sort).toEqual({ field: 'createdAt', direction: 'desc' });
    expect(new TrackListView('f3').isDefault).toBe(true);
  });

  it('並び順だけ・列と並び順を、既定に戻せる', () => {
    const view = new TrackListView('g', { sort: null });
    view.columns = ['title', 'genre'];
    view.sort = { field: 'title', direction: 'asc' };
    expect(view.isDefaultSort).toBe(false);

    view.resetSort();
    expect(view.sort).toBeNull();
    expect(view.isDefaultSort).toBe(true);
    expect(view.columns).toEqual(['title', 'genre']);
    expect(view.isDefault).toBe(false);

    view.reset();
    expect(view.isDefault).toBe(true);
    expect(new TrackListView('g', { sort: null }).columns).toHaveLength(5);
  });

  it('列を空にはできない（既定の列に戻る）', () => {
    const view = new TrackListView('h');
    view.columns = [];

    expect(view.columns).toHaveLength(5);
  });

  it('名前がなければ、保存しない', () => {
    const view = new TrackListView(null);
    view.columns = ['title'];

    expect(view.columns).toEqual(['title']);
    expect(localStorage.length).toBe(0);
  });
});

describe('trackListView', () => {
  it('同じ名前の画面は、同じ設定を共有する', () => {
    const first = trackListView('shared', { columns: ['title', 'album'] });
    const second = trackListView('shared');

    expect(second).toBe(first);
    first.columns = ['title'];
    expect(second.columns).toEqual(['title']);
    expect(trackListView('other')).not.toBe(first);
  });
});

describe('列と並び順を別の単位で覚える（プレイリスト）', () => {
  it('列が既定のままかは、並び順と別に分かる', () => {
    const view = new TrackListView('split', { columns: ['title', 'album'], sort: null });
    view.sort = { field: 'title', direction: 'asc' };

    expect(view.isDefaultColumns).toBe(true);
    expect(view.isDefault).toBe(false);

    view.columns = ['title'];
    expect(view.isDefaultColumns).toBe(false);
  });

  it('プレイリストごとの並び順の名前は、プレイリストのIDで決まる', () => {
    expect(playlistSortViewId('p1')).toBe('playlist:p1');
    expect(playlistSortViewId('p1')).not.toBe(playlistSortViewId('p2'));
  });

  it('設定を忘れると、保存していた内容も消える', () => {
    const id = playlistSortViewId('gone');
    const view = trackListView(id, { sort: null });
    view.sort = { field: 'artist', direction: 'desc' };
    expect(localStorage.getItem(`muspice:trackListView:${id}`)).not.toBeNull();

    forgetTrackListView(id);

    expect(localStorage.getItem(`muspice:trackListView:${id}`)).toBeNull();
    // 同じ名前でもう一度開くと、既定から始まる
    const reopened = trackListView(id, { sort: null });
    expect(reopened).not.toBe(view);
    expect(reopened.sort).toBeNull();
  });
});
