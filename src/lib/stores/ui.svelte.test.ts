import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { FOLLOW_INFO_INTERVAL, followTrackInfo, trackInfoTarget, ui } from './ui.svelte.js';

/** テスト用の曲（`album`をnullにすると、アルバムのない曲） */
function track(id: string, album: string | null = 'Album'): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: `Song ${id}`,
    artist: 'Artist',
    album,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: 1,
    discNumber: null,
    duration: 200,
    fileSize: 1,
    format: 'mp3',
    bitrate: null,
    sampleRate: null,
    isFavorite: false,
    rating: 0,
    playCount: 0,
    skipCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    sortTags: { title: null, artist: null, album: null, albumArtist: null },
    isMissing: false
  };
}

const artist = {
  name: 'Artist',
  sortName: null,
  albumCount: 2,
  trackCount: 20,
  totalDuration: 4800,
  representativeTrackId: 't1'
};

describe('followTrackInfo', () => {
  /** 一覧の曲（取り直すと、配列ごと入れ替わる） */
  let list = $state.raw<Track[]>([]);
  /** 一覧で最後に選んだ曲のID */
  let activeId = $state<string | null>(null);
  let cleanup: () => void;

  beforeEach(() => {
    vi.useFakeTimers();
    list = [];
    activeId = null;
    ui.infoTarget = null;
    ui.isRightSidebarExpanded = false;
    ui.activeRightSidebarPanel = 'queue';
    cleanup = $effect.root(() => {
      followTrackInfo(() => list.find((track) => track.id === activeId) ?? null);
    });
    flushSync();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  function openInfoPanel() {
    ui.activeRightSidebarPanel = 'info';
    ui.isRightSidebarExpanded = true;
  }

  it('情報パネルを開いている間だけ、選んだ曲のアルバムを反映する（閉じている間は開かない）', () => {
    list = [track('t1'), track('t2', null)];
    activeId = 't1';
    flushSync();
    expect(ui.infoTarget).toBeNull();
    expect(ui.isRightSidebarExpanded).toBe(false);

    // 開いた時に、選んでいる曲を反映する
    openInfoPanel();
    flushSync();
    expect(ui.infoTarget).toEqual(trackInfoTarget(track('t1')));

    // 別の曲を選ぶと入れ替える（アルバムのない曲は、曲そのもの）
    vi.advanceTimersByTime(FOLLOW_INFO_INTERVAL);
    activeId = 't2';
    flushSync();
    expect(ui.infoTarget).toEqual({ type: 'track', track: track('t2', null) });
  });

  it('まだ選んでいない時・一覧から選んだ曲がなくなった時は、表示している項目を残す', () => {
    ui.showInfo({ type: 'artist', artist });
    flushSync();
    expect(ui.infoTarget).toEqual({ type: 'artist', artist });

    list = [track('t1')];
    activeId = 't1';
    flushSync();
    expect(ui.infoTarget).toEqual(trackInfoTarget(track('t1')));

    // 絞り込みなどで一覧から消えても、最後に選んだ曲のアルバムを残す
    list = [];
    flushSync();
    expect(ui.infoTarget).toEqual(trackInfoTarget(track('t1')));
  });

  it('同じ曲のまま一覧を取り直しても反映し直さず、曲が変わった（タグを編集した）時は追随する', () => {
    const follow = vi.spyOn(ui, 'followInfo');
    openInfoPanel();
    const song = track('t1');
    list = [song];
    activeId = 't1';
    flushSync();
    expect(follow).toHaveBeenCalledTimes(1);

    // 取り直しても変わらない曲は同じオブジェクトのまま（TanStack Queryの構造の共有）
    list = [song, track('t2')];
    flushSync();
    expect(follow).toHaveBeenCalledTimes(1);

    // タグを変えた曲は、別のオブジェクトになる
    vi.advanceTimersByTime(FOLLOW_INFO_INTERVAL);
    list = [{ ...song, album: 'Renamed' }, track('t2')];
    flushSync();
    expect(follow).toHaveBeenCalledTimes(2);
    expect(ui.infoTarget?.type === 'album' && ui.infoTarget.album.name).toBe('Renamed');
  });
});
