import { describe, expect, it } from 'vitest';
import type { Playlist, PlaylistFolder } from '#lib/types/models.js';
import {
  buildPlaylistTree,
  isPlaylistSort,
  placeRelative,
  playlistsIn,
  sortPlaylistItems
} from './playlistTree.js';

function playlist(
  id: string,
  name: string,
  overrides: Partial<Pick<Playlist, 'folderId' | 'position' | 'createdAt'>> = {}
): Playlist {
  return {
    id,
    name,
    description: null,
    tracks: [],
    rules: null,
    folderId: null,
    position: 0,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

function folder(id: string, name: string, position = 0, createdAt = '2026-01-01T00:00:00Z') {
  return { id, name, position, createdAt } satisfies PlaylistFolder;
}

const names = (items: readonly { name: string }[]) => items.map((item) => item.name);

describe('sortPlaylistItems', () => {
  const items = [
    playlist('1', 'ドライブ', { position: 2, createdAt: '2026-01-03T00:00:00Z' }),
    playlist('2', 'あさ', { position: 0, createdAt: '2026-01-01T00:00:00Z' }),
    playlist('3', 'Workout', { position: 1, createdAt: '2026-01-02T00:00:00Z' })
  ];

  it('名前の順（言語に合わせた比較）', () => {
    expect(names(sortPlaylistItems(items, 'name'))).toEqual(['Workout', 'あさ', 'ドライブ']);
  });

  it('作成日の新しい順', () => {
    expect(names(sortPlaylistItems(items, 'createdAt'))).toEqual(['ドライブ', 'Workout', 'あさ']);
  });

  it('手動の順（位置の小さい順）', () => {
    expect(names(sortPlaylistItems(items, 'manual'))).toEqual(['あさ', 'Workout', 'ドライブ']);
  });

  it('手動で並べたことがないもの（位置が同じ）は、作成日の新しい順', () => {
    const unplaced = items.map((item) => ({ ...item, position: 0 }));
    expect(names(sortPlaylistItems(unplaced, 'manual'))).toEqual(['ドライブ', 'Workout', 'あさ']);
  });

  it('元の配列は変えない', () => {
    const before = names(items);
    sortPlaylistItems(items, 'name');
    expect(names(items)).toEqual(before);
  });
});

describe('buildPlaylistTree', () => {
  const folders = [folder('f1', '外出', 1), folder('f2', 'いえ', 0)];
  const playlists = [
    playlist('1', 'あさ', { folderId: 'f1', position: 1 }),
    playlist('2', 'ドライブ', { folderId: 'f1', position: 0 }),
    playlist('3', '作業用'),
    playlist('4', '迷子', { folderId: 'deleted' })
  ];

  it('フォルダごとに分け、フォルダの外のものを分ける', () => {
    const tree = buildPlaylistTree(folders, playlists, 'manual');

    expect(tree.folders.map((node) => [node.folder.name, names(node.playlists)])).toEqual([
      ['いえ', []],
      ['外出', ['ドライブ', 'あさ']]
    ]);
    // 入っているフォルダが見つからないプレイリストは、フォルダの外に出す
    expect(names(tree.root).sort()).toEqual(['作業用', '迷子'].sort());
  });

  it('フォルダも、選んだ並び順に並べる', () => {
    const tree = buildPlaylistTree(folders, playlists, 'name');
    expect(tree.folders.map((node) => node.folder.name)).toEqual(['いえ', '外出']);
    expect(names(tree.folders[1].playlists)).toEqual(['あさ', 'ドライブ']);
  });

  it('フォルダの中のプレイリストを取り出す', () => {
    const tree = buildPlaylistTree(folders, playlists, 'manual');
    expect(names(playlistsIn(tree, 'f1'))).toEqual(['ドライブ', 'あさ']);
    expect(playlistsIn(tree, 'missing')).toEqual([]);
    expect(playlistsIn(tree, null)).toBe(tree.root);
  });
});

describe('placeRelative', () => {
  const order = ['a', 'b', 'c', 'd'];

  it('落とした項目の前・後ろへ動かす', () => {
    expect(placeRelative(order, 'd', 'b', false)).toEqual(['a', 'd', 'b', 'c']);
    expect(placeRelative(order, 'a', 'c', true)).toEqual(['b', 'c', 'a', 'd']);
    expect(placeRelative(order, 'a', 'b', false)).toEqual(['a', 'b', 'c', 'd']);
  });

  it('一覧にない項目（別のフォルダから運んだもの）を入れる', () => {
    expect(placeRelative(order, 'x', 'a', false)).toEqual(['x', 'a', 'b', 'c', 'd']);
    expect(placeRelative(order, 'x', 'd', true)).toEqual(['a', 'b', 'c', 'd', 'x']);
  });

  it('自分自身・一覧にない項目へ落とした場合は、何も変えない', () => {
    expect(placeRelative(order, 'b', 'b', true)).toEqual(order);
    expect(placeRelative(order, 'b', 'x', true)).toEqual(order);
  });
});

describe('isPlaylistSort', () => {
  it('並び順の値だけを通す', () => {
    expect(isPlaylistSort('manual')).toBe(true);
    expect(isPlaylistSort('rating')).toBe(false);
    expect(isPlaylistSort(null)).toBe(false);
  });
});
