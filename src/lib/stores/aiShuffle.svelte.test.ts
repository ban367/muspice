import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { commands } from '#lib/bindings.js';
import type { QueueEntry } from '#lib/utils/playQueue.js';
import { watchAiShuffle } from './aiShuffle.svelte.js';
import {
  player,
  playNextTrack,
  replaceQueueAndPlay,
  resetPlayer,
  setAiShuffleEnabled
} from './player.svelte.js';

vi.mock('#lib/bindings.js', () => ({
  AI_SHUFFLE_EXCLUSION_WINDOW: 2,
  commands: {
    pickAiShuffleTrack: vi.fn()
  }
}));

function makeTrack(id: string): Track {
  return { id, title: id, isMissing: false } as Track;
}

const library = ['a', 'b', 'c', 'd', 'e', 'f'].map(makeTrack);
const ids = (queue: readonly QueueEntry[]) => queue.map((entry) => entry.track.id);

/** 非同期の処理（曲の選択）が終わるのを待つ */
const flush = async () => {
  flushSync();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
};

let stop: () => void;

beforeEach(() => {
  resetPlayer();
  player.isShuffleEnabled = false;
  player.isAiShuffleEnabled = false;
  player.repeatMode = 'off';
  vi.clearAllMocks();
  vi.mocked(commands.pickAiShuffleTrack).mockResolvedValue(makeTrack('picked'));
  vi.spyOn(console, 'error').mockImplementation(() => {});
  stop = watchAiShuffle();
});

afterEach(() => {
  stop();
  vi.restoreAllMocks();
});

/** 曲を再生している状態にする（再生コントローラーの代わり） */
function play(index: number): void {
  replaceQueueAndPlay(library, index);
  player.isPlaying = true;
}

describe('watchAiShuffle', () => {
  it('有効な間、再生中の曲の次に、選んだ曲を足す（前後の曲を避けるように渡す）', async () => {
    play(2);
    setAiShuffleEnabled(true);
    await flush();

    expect(commands.pickAiShuffleTrack).toHaveBeenCalledExactlyOnceWith('c', [
      'a',
      'b',
      'c',
      'd',
      'e'
    ]);
    expect(ids(player.playQueue)).toEqual(['a', 'b', 'c', 'picked', 'd', 'e', 'f']);
  });

  it('次の曲へ進むたびに足す（足した曲の再生中も）', async () => {
    play(0);
    setAiShuffleEnabled(true);
    await flush();
    vi.mocked(commands.pickAiShuffleTrack).mockResolvedValue(makeTrack('picked2'));

    playNextTrack();
    await flush();

    expect(commands.pickAiShuffleTrack).toHaveBeenLastCalledWith('picked', expect.any(Array));
    expect(ids(player.playQueue)).toEqual(['a', 'picked', 'picked2', 'b', 'c', 'd', 'e', 'f']);
  });

  it('無効の間・再生していない間・1曲リピートの間は、選ばない', async () => {
    play(0);
    await flush();
    expect(commands.pickAiShuffleTrack).not.toHaveBeenCalled();

    player.isPlaying = false;
    setAiShuffleEnabled(true);
    await flush();
    expect(commands.pickAiShuffleTrack).not.toHaveBeenCalled();

    player.repeatMode = 'one';
    player.isPlaying = true;
    await flush();
    expect(commands.pickAiShuffleTrack).not.toHaveBeenCalled();

    // 1曲リピートを解除したら、足す
    player.repeatMode = 'off';
    await flush();
    expect(commands.pickAiShuffleTrack).toHaveBeenCalledOnce();
    expect(ids(player.playQueue)).toEqual(['a', 'picked', 'b', 'c', 'd', 'e', 'f']);
  });

  it('選んでいる間に曲が変わったら、前の結果は使わない', async () => {
    let resolveFirst: (track: Track) => void = () => {};
    vi.mocked(commands.pickAiShuffleTrack)
      .mockImplementationOnce(
        () =>
          new Promise<Track>((resolve) => {
            resolveFirst = resolve;
          })
      )
      .mockResolvedValueOnce(makeTrack('second'));
    play(0);
    setAiShuffleEnabled(true);
    flushSync();

    playNextTrack();
    await flush();
    resolveFirst(makeTrack('first'));
    await flush();

    expect(ids(player.playQueue)).toEqual(['a', 'b', 'second', 'c', 'd', 'e', 'f']);
  });

  it('選べなかった・失敗した時は、キューを変えない', async () => {
    vi.mocked(commands.pickAiShuffleTrack).mockResolvedValueOnce(null);
    play(0);
    setAiShuffleEnabled(true);
    await flush();
    expect(ids(player.playQueue)).toEqual(['a', 'b', 'c', 'd', 'e', 'f']);

    vi.mocked(commands.pickAiShuffleTrack).mockRejectedValueOnce(new Error('failed'));
    playNextTrack();
    await flush();
    expect(ids(player.playQueue)).toEqual(['a', 'b', 'c', 'd', 'e', 'f']);
    expect(console.error).toHaveBeenCalled();
  });
});
