import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/svelte-query';
import type { Track } from '#lib/types/models.js';

vi.mock('#lib/queries/tracks.js', () => ({
  fetchGroupTracks: vi.fn(),
  fetchAlbumArtistTracks: vi.fn()
}));

import { fetchAlbumArtistTracks, fetchGroupTracks } from '#lib/queries/tracks.js';
import {
  TRACK_DRAG_TYPE,
  TRACK_GROUP_DRAG_TYPE,
  isTrackDrag,
  readDraggedTracks,
  resolveDraggedTrackIds,
  startTrackGroupDrag
} from './trackDrag.js';

/** `dataTransfer`に指定したデータだけを持つドラッグイベント */
function dragEvent(data: Record<string, string>): DragEvent {
  return {
    dataTransfer: {
      types: Object.keys(data),
      getData: (type: string) => data[type] ?? ''
    }
  } as unknown as DragEvent;
}

/** まとまりのデータ（JSON）を持つドラッグイベント */
const groupDragEvent = (group: unknown) =>
  dragEvent({ [TRACK_GROUP_DRAG_TYPE]: JSON.stringify(group) });

const queryClient = {} as QueryClient;

beforeEach(() => {
  vi.clearAllMocks();
});

describe('readDraggedTracks', () => {
  it('ドラッグしたトラックIDを順番どおりに返す', () => {
    const event = dragEvent({ [TRACK_DRAG_TYPE]: JSON.stringify(['t2', 't1', 't3']) });
    expect(readDraggedTracks(event)).toEqual({ kind: 'tracks', trackIds: ['t2', 't1', 't3'] });
  });

  it('トラックのドラッグでない場合はnullを返す', () => {
    expect(readDraggedTracks(dragEvent({ 'text/plain': 't1' }))).toBeNull();
    expect(readDraggedTracks({ dataTransfer: null } as DragEvent)).toBeNull();
  });

  it('壊れたデータはnullにし、文字列でない要素は捨てる', () => {
    expect(readDraggedTracks(dragEvent({ [TRACK_DRAG_TYPE]: '{' }))).toBeNull();
    expect(readDraggedTracks(dragEvent({ [TRACK_DRAG_TYPE]: '"t1"' }))).toBeNull();
    expect(readDraggedTracks(dragEvent({ [TRACK_DRAG_TYPE]: '["t1", 2, null]' }))).toEqual({
      kind: 'tracks',
      trackIds: ['t1']
    });
  });

  it('アルバム・アーティスト・ジャンル・アルバムアーティストのまとまりを返す', () => {
    const groups = [
      { type: 'album', name: 'Blue Horizon', albumArtist: 'Aoi Sora' },
      { type: 'album', name: 'Untitled', albumArtist: null },
      { type: 'artist', name: 'Aoi Sora' },
      { type: 'genre', name: 'Jazz' },
      { type: 'albumArtist', name: 'Aoi Sora' },
      { type: 'albumArtist', name: null }
    ];
    for (const group of groups) {
      expect(readDraggedTracks(groupDragEvent(group))).toEqual({ kind: 'group', group });
    }
  });

  it('まとまりの余分な値は捨てる', () => {
    expect(
      readDraggedTracks(groupDragEvent({ type: 'artist', name: 'Aoi Sora', albumArtist: 'x' }))
    ).toEqual({ kind: 'group', group: { type: 'artist', name: 'Aoi Sora' } });
  });

  it('形の違うまとまりはnullにする', () => {
    const invalid = [
      { type: 'album', name: 'Blue Horizon' },
      { type: 'album', name: 1, albumArtist: null },
      { type: 'artist', name: null },
      { type: 'genre' },
      { type: 'albumArtist' },
      { type: 'playlist', name: 'x' },
      ['album'],
      'album',
      null
    ];
    for (const group of invalid) {
      expect(readDraggedTracks(groupDragEvent(group))).toBeNull();
    }
    expect(readDraggedTracks(dragEvent({ [TRACK_GROUP_DRAG_TYPE]: '{' }))).toBeNull();
  });
});

describe('isTrackDrag', () => {
  it('曲・まとまりのドラッグだけをtrueにする', () => {
    expect(isTrackDrag(dragEvent({ [TRACK_DRAG_TYPE]: '[]' }))).toBe(true);
    expect(isTrackDrag(dragEvent({ [TRACK_GROUP_DRAG_TYPE]: '{}' }))).toBe(true);
    expect(isTrackDrag(dragEvent({ Files: '' }))).toBe(false);
    expect(isTrackDrag({ dataTransfer: null } as DragEvent)).toBe(false);
  });
});

describe('startTrackGroupDrag', () => {
  /** ドラッグ中の画像を作るための、最小限の要素（テストはDOMのない環境で動く） */
  interface FakeElement {
    className: string;
    textContent: string;
    innerHTML: string;
    children: FakeElement[];
    append: (...children: FakeElement[]) => void;
    remove: () => void;
  }

  function fakeElement(): FakeElement {
    const element: FakeElement = {
      className: '',
      textContent: '',
      innerHTML: '',
      children: [],
      append: (...children) => element.children.push(...children),
      remove: () => {}
    };
    return element;
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('まとまりを、読み直せるデータとして運ぶ', () => {
    vi.stubGlobal('document', { createElement: fakeElement, body: { appendChild: () => {} } });
    const data: Record<string, string> = {};
    const transfer = {
      effectAllowed: 'all',
      setData: (type: string, value: string) => (data[type] = value),
      setDragImage: vi.fn()
    };
    const group = { type: 'album', name: 'Blue Horizon', albumArtist: 'Aoi Sora' } as const;

    startTrackGroupDrag({ dataTransfer: transfer } as unknown as DragEvent, group, {
      label: 'Blue Horizon',
      detail: '12曲'
    });

    expect(transfer.effectAllowed).toBe('copy');
    expect(readDraggedTracks(dragEvent(data))).toEqual({ kind: 'group', group });
    // ドラッグ中の画像に、名前と曲数を出す（アイコンの後に）
    const preview = transfer.setDragImage.mock.calls[0][0] as FakeElement;
    expect(preview.children.map((child) => [child.className, child.textContent])).toEqual([
      ['drag-preview-icon', ''],
      ['drag-preview-label', 'Blue Horizon'],
      ['drag-preview-detail', '12曲']
    ]);
  });
});

describe('resolveDraggedTrackIds', () => {
  const tracks = [{ id: 'a' }, { id: 'b' }] as Track[];

  it('曲は、運んだトラックIDをそのまま返す', async () => {
    await expect(
      resolveDraggedTrackIds(queryClient, { kind: 'tracks', trackIds: ['b', 'a'] })
    ).resolves.toEqual(['b', 'a']);
    expect(fetchGroupTracks).not.toHaveBeenCalled();
  });

  it('アルバムは、まとめたアーティストも渡して曲を取得する', async () => {
    vi.mocked(fetchGroupTracks).mockResolvedValue(tracks);
    const group = { type: 'album', name: 'Blue Horizon', albumArtist: 'Aoi Sora' } as const;

    await expect(resolveDraggedTrackIds(queryClient, { kind: 'group', group })).resolves.toEqual([
      'a',
      'b'
    ]);
    expect(fetchGroupTracks).toHaveBeenCalledWith(queryClient, 'album', 'Blue Horizon', 'Aoi Sora');
  });

  it('アーティスト・ジャンルは、名前で曲を取得する', async () => {
    vi.mocked(fetchGroupTracks).mockResolvedValue(tracks);

    await resolveDraggedTrackIds(queryClient, {
      kind: 'group',
      group: { type: 'artist', name: 'Aoi Sora' }
    });
    await resolveDraggedTrackIds(queryClient, {
      kind: 'group',
      group: { type: 'genre', name: 'Jazz' }
    });

    expect(fetchGroupTracks).toHaveBeenNthCalledWith(1, queryClient, 'artist', 'Aoi Sora', null);
    expect(fetchGroupTracks).toHaveBeenNthCalledWith(2, queryClient, 'genre', 'Jazz', null);
  });

  it('アルバムアーティストは、そのアルバムすべての曲を取得する', async () => {
    vi.mocked(fetchAlbumArtistTracks).mockResolvedValue(tracks);

    await expect(
      resolveDraggedTrackIds(queryClient, {
        kind: 'group',
        group: { type: 'albumArtist', name: null }
      })
    ).resolves.toEqual(['a', 'b']);
    expect(fetchAlbumArtistTracks).toHaveBeenCalledWith(queryClient, null);
  });

  it('取得に失敗した場合は、例外を投げる', async () => {
    vi.mocked(fetchGroupTracks).mockRejectedValue(new Error('取得できない'));

    await expect(
      resolveDraggedTrackIds(queryClient, {
        kind: 'group',
        group: { type: 'genre', name: 'Jazz' }
      })
    ).rejects.toThrow('取得できない');
  });
});
