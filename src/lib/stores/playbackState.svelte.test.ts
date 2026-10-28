import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RestoredPlaybackState } from '#lib/bindings.js';
import type { Track } from '#lib/types/models.js';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    getPlaybackState: vi.fn(),
    savePlaybackState: vi.fn()
  }
}));

import { commands } from '#lib/bindings.js';
import { restorePlaybackState, watchPlaybackState } from './playbackState.svelte.js';
import { addToQueue, player, replaceQueueAndPlay, resetPlayer } from './player.svelte.js';

function makeTrack(id: string): Track {
  return { id, title: id, duration: 180, isMissing: false } as Track;
}

const [a, b, c] = ['a', 'b', 'c'].map(makeTrack);
const ids = (entries: readonly { track: Track }[]) => entries.map((entry) => entry.track.id);

function stored(overrides: Partial<RestoredPlaybackState> = {}): RestoredPlaybackState {
  return {
    volume: 0.5,
    shuffle: true,
    repeat: 'all',
    queue: [a, b, c],
    originalTrackIds: ['c', 'a', 'b'],
    currentIndex: 1,
    ...overrides
  };
}

/** 保存の待ち時間（`SAVE_DELAY_MS`）が過ぎ、保存の結果まで済むのを待つ */
async function passSaveDelay() {
  flushSync();
  await vi.advanceTimersByTimeAsync(300);
}

/** 始めた監視（失敗したテストでも、次のテストへ残さない） */
let stopWatching: (() => void) | null = null;
function watch(): () => void {
  stopWatching = watchPlaybackState();
  return stopWatching;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetPlayer();
  // `resetPlayer`は、キュー以外（音量・シャッフル・リピート）を戻さない
  player.volume = 1;
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
  vi.mocked(commands.savePlaybackState).mockResolvedValue(null);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

afterEach(() => {
  stopWatching?.();
  stopWatching = null;
  vi.useRealTimers();
});

describe('restorePlaybackState', () => {
  it('音量・シャッフル・リピートと、キューと再生していた曲を戻す（再生は始めない）', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValue(stored());
    const onTrackRestoring = vi.fn();

    await restorePlaybackState({ isCancelled: () => false, onTrackRestoring });

    expect(player.volume).toBe(0.5);
    expect(player.isShuffleEnabled).toBe(true);
    expect(player.repeatMode).toBe('all');
    expect(onTrackRestoring).toHaveBeenCalledWith(b);
    expect(player.currentTrack?.id).toBe('b');
    expect(ids(player.playQueue)).toEqual(['a', 'b', 'c']);
    // シャッフルする前の順は、IDの並びから戻す
    expect(ids(player.originalQueue)).toEqual(['c', 'a', 'b']);
  });

  it('復元を待つ間に要らなくなった場合は、何も戻さない', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValue(stored());

    await restorePlaybackState({ isCancelled: () => true, onTrackRestoring: vi.fn() });

    expect(player.volume).toBe(1);
    expect(player.playQueue).toEqual([]);
  });

  it('待つ間に再生を始めていたら、キューは戻さない（音量などは戻す）', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValue(stored());
    replaceQueueAndPlay([makeTrack('playing')], 0);
    const onTrackRestoring = vi.fn();

    await restorePlaybackState({ isCancelled: () => false, onTrackRestoring });

    expect(player.volume).toBe(0.5);
    expect(player.currentTrack?.id).toBe('playing');
    expect(onTrackRestoring).not.toHaveBeenCalled();
  });

  it('何も再生していなかった（キューへ入れただけの）キューも戻す', async () => {
    vi.mocked(commands.getPlaybackState).mockResolvedValue(
      stored({ shuffle: false, originalTrackIds: null, currentIndex: null })
    );
    const onTrackRestoring = vi.fn();

    await restorePlaybackState({ isCancelled: () => false, onTrackRestoring });

    expect(ids(player.playQueue)).toEqual(['a', 'b', 'c']);
    expect(player.currentTrackIndex).toBe(-1);
    expect(player.currentTrack).toBeNull();
    expect(onTrackRestoring).not.toHaveBeenCalled();
  });
});

describe('watchPlaybackState', () => {
  it('始めた時点の状態（復元した直後）は保存しない', async () => {
    const stop = watch();
    await passSaveDelay();

    expect(commands.savePlaybackState).not.toHaveBeenCalled();
    stop();
  });

  it('続けての変更は、0.3秒待ってまとめて保存する（キューが変わっていなければ、キューは送らない）', async () => {
    const stop = watch();
    flushSync();

    player.volume = 0.3;
    flushSync();
    await vi.advanceTimersByTimeAsync(100);
    player.volume = 0.2;
    player.repeatMode = 'one';
    flushSync();
    await vi.advanceTimersByTimeAsync(199);
    expect(commands.savePlaybackState).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(commands.savePlaybackState).toHaveBeenCalledExactlyOnceWith(
      { volume: 0.2, shuffle: false, repeat: 'one', currentIndex: null },
      null
    );
    stop();
  });

  it('キューが変わったら、キューも送る。送った後は、キューが変わるまで送らない', async () => {
    const stop = watch();
    flushSync();

    addToQueue([a, b]);
    await passSaveDelay();
    expect(commands.savePlaybackState).toHaveBeenLastCalledWith(expect.anything(), {
      trackIds: ['a', 'b'],
      originalTrackIds: null
    });

    player.volume = 0.4;
    await passSaveDelay();
    expect(commands.savePlaybackState).toHaveBeenLastCalledWith(
      expect.objectContaining({ volume: 0.4 }),
      null
    );
    stop();
  });

  it('キューを送った保存が失敗したら、次の保存でキューを送り直す', async () => {
    vi.mocked(commands.savePlaybackState).mockRejectedValueOnce(new Error('保存できない'));
    const stop = watch();
    flushSync();

    addToQueue([a]);
    await passSaveDelay();
    player.volume = 0.4;
    await passSaveDelay();

    expect(commands.savePlaybackState).toHaveBeenCalledTimes(2);
    expect(vi.mocked(commands.savePlaybackState).mock.calls[1][1]).toEqual({
      trackIds: ['a'],
      originalTrackIds: null
    });
    stop();
  });

  it('やめる時に保存を待っている変更があれば、すぐに保存する', () => {
    const stop = watch();
    flushSync();
    player.volume = 0.7;
    flushSync();

    stop();

    expect(commands.savePlaybackState).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ volume: 0.7 }),
      null
    );
  });
});
