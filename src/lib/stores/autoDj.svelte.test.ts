import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/svelte-query';
import type { Track } from '#lib/types/models.js';
import { commands } from '#lib/bindings.js';
import { AUTO_DJ_BATCH_SIZE } from '#lib/utils/autoDj.js';
import { watchAutoDj } from './autoDj.svelte.js';
import { playbackAids } from './playbackAids.svelte.js';
import {
  player,
  playNextTrack,
  playTrackFromQueue,
  resetPlayer,
  toggleRepeat
} from './player.svelte.js';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    getAllTracks: vi.fn(),
    getPlaylistTracks: vi.fn()
  }
}));

function makeTrack(id: string): Track {
  return { id, title: id, isMissing: false } as Track;
}

const library = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'].map(makeTrack);
const playlist = ['p1', 'p2'].map(makeTrack);
const ids = (tracks: readonly Track[]) => tracks.map((track) => track.id);

/** 非同期の処理（曲の取得）が終わるのを待つ */
const flush = async () => {
  flushSync();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
};

/** キャッシュを持たず、毎回取得するクエリクライアント */
const queryClient = {
  fetchQuery: vi.fn(({ queryFn }: { queryFn: () => Promise<Track[]> }) => queryFn())
} as unknown as QueryClient;

let enabled = $state(true);
let playlistId = $state<string | null>(null);
let stop: () => void;

beforeEach(() => {
  resetPlayer();
  playbackAids.reset();
  player.repeatMode = 'off';
  player.isShuffleEnabled = false;
  enabled = true;
  playlistId = null;
  vi.clearAllMocks();
  vi.mocked(commands.getAllTracks).mockResolvedValue(library);
  vi.mocked(commands.getPlaylistTracks).mockResolvedValue(playlist);
  vi.spyOn(console, 'error').mockImplementation(() => {});
  stop = watchAutoDj({ enabled: () => enabled, playlistId: () => playlistId, queryClient });
});

afterEach(() => {
  stop();
  vi.restoreAllMocks();
});

describe('watchAutoDj', () => {
  it('キューの最後の曲になったら、キューにない曲をライブラリから足す', async () => {
    playTrackFromQueue([makeTrack('a'), makeTrack('b')], 0);
    await flush();
    // まだ次の曲がある
    expect(player.playQueue).toHaveLength(2);
    expect(commands.getAllTracks).not.toHaveBeenCalled();

    playNextTrack();
    await flush();

    expect(player.playQueue).toHaveLength(2 + AUTO_DJ_BATCH_SIZE);
    const added = ids(player.playQueue).slice(2);
    expect(new Set(added).size).toBe(AUTO_DJ_BATCH_SIZE);
    expect(added).not.toContain('a');
    expect(added).not.toContain('b');
    // 再生中の曲は変えない
    expect(player.currentTrack?.id).toBe('b');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('1曲だけを再生した場合も、すぐに足す', async () => {
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();

    expect(player.playQueue).toHaveLength(1 + AUTO_DJ_BATCH_SIZE);
    expect(player.upcomingTracks).toHaveLength(AUTO_DJ_BATCH_SIZE);
  });

  it('足した曲の最後まで進んだら、また足す', async () => {
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();
    for (let index = 0; index < AUTO_DJ_BATCH_SIZE; index++) playNextTrack();
    await flush();

    expect(player.playQueue).toHaveLength(1 + AUTO_DJ_BATCH_SIZE * 2);
    expect(new Set(ids(player.playQueue)).size).toBe(player.playQueue.length);
  });

  it('プレイリストを元にした場合は、プレイリストの曲から足す', async () => {
    playlistId = 'playlist-1';
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();

    expect(commands.getPlaylistTracks).toHaveBeenCalledWith('playlist-1');
    expect(commands.getAllTracks).not.toHaveBeenCalled();
    expect(ids(player.playQueue).slice(1).sort()).toEqual(['p1', 'p2']);
  });

  it('無効・リピート中・「この曲が終わったら停止」の間・再生する曲がない間は、足さない', async () => {
    enabled = false;
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();
    expect(player.playQueue).toHaveLength(1);

    enabled = true;
    toggleRepeat();
    playbackAids.stopAfterCurrent = true;
    await flush();
    expect(player.playQueue).toHaveLength(1);

    // リピートを解除しても、「この曲が終わったら停止」の間は足さない
    player.repeatMode = 'off';
    await flush();
    expect(player.playQueue).toHaveLength(1);

    playbackAids.stopAfterCurrent = false;
    await flush();
    expect(player.playQueue).toHaveLength(1 + AUTO_DJ_BATCH_SIZE);

    resetPlayer();
    await flush();
    expect(player.playQueue).toEqual([]);
  });

  it('有効にした時に最後の曲を再生していたら、その場で足す', async () => {
    enabled = false;
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();

    enabled = true;
    await flush();

    expect(player.playQueue).toHaveLength(1 + AUTO_DJ_BATCH_SIZE);
  });

  it('取得を待つ間にキューが変わっていたら、足さない', async () => {
    let resolveTracks: (tracks: Track[]) => void = () => {};
    vi.mocked(commands.getAllTracks).mockReturnValue(
      new Promise((resolve) => (resolveTracks = resolve))
    );
    playTrackFromQueue([makeTrack('a')], 0);
    flushSync();

    // 取得を待つ間に、別の曲を再生し始めた（最後の曲ではなくなった）
    playTrackFromQueue([makeTrack('x'), makeTrack('y')], 0);
    resolveTracks(library);
    await flush();

    expect(ids(player.playQueue)).toEqual(['x', 'y']);
  });

  it('選べる曲がない・取得に失敗した場合は、何も足さない', async () => {
    vi.mocked(commands.getAllTracks).mockResolvedValue([]);
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();
    expect(player.playQueue).toHaveLength(1);

    vi.mocked(commands.getAllTracks).mockRejectedValue(new Error('failed'));
    playTrackFromQueue([makeTrack('b')], 0);
    await flush();
    expect(ids(player.playQueue)).toEqual(['b']);
    expect(console.error).toHaveBeenCalled();
  });

  it('止めた後は、足さない', async () => {
    stop();
    playTrackFromQueue([makeTrack('a')], 0);
    await flush();

    expect(player.playQueue).toHaveLength(1);
  });
});
