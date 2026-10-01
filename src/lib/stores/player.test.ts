import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '$lib/types/models';
import {
  clearQueue,
  currentTime,
  currentTrack,
  currentTrackIndex,
  formatTime,
  hasNextTrack,
  hasPreviousTrack,
  isShuffleEnabled,
  originalQueue,
  playNextTrack,
  playPreviousTrack,
  playQueue,
  playTrackFromQueue,
  removeFromQueue,
  repeatMode,
  resetPlayer,
  toggleRepeat,
  toggleShuffle,
  upcomingTracks
} from './player';

/** キュー操作の検証に必要なidだけを持つトラック */
function makeTrack(id: string): Track {
  return { id, title: id } as Track;
}

const tracks = ['t1', 't2', 't3', 't4'].map(makeTrack);
const ids = (list: Track[]) => list.map((t) => t.id);

beforeEach(() => {
  // ストアはモジュール単位のシングルトンのため、テストごとに初期化する
  resetPlayer();
  isShuffleEnabled.set(false);
  repeatMode.set('off');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('playTrackFromQueue', () => {
  it('キューを設定し、指定位置のトラックを再生する', () => {
    playTrackFromQueue(tracks, 2);

    expect(ids(get(playQueue))).toEqual(['t1', 't2', 't3', 't4']);
    expect(get(currentTrackIndex)).toBe(2);
    expect(get(currentTrack)?.id).toBe('t3');
  });

  it('範囲外のインデックスでは状態を変えない', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    playTrackFromQueue(tracks, 4);

    expect(get(playQueue)).toEqual([]);
    expect(get(currentTrack)).toBeNull();
  });

  it('シャッフル中は選択したトラックを先頭にし、残りを並べ替える', () => {
    isShuffleEnabled.set(true);
    playTrackFromQueue(tracks, 2);

    const queue = ids(get(playQueue));
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(get(currentTrackIndex)).toBe(0);
    // シャッフル解除時に戻すため元の順序を保持する
    expect(ids(get(originalQueue))).toEqual(['t1', 't2', 't3', 't4']);
  });
});

describe('playNextTrack', () => {
  it('次のトラックへ進む', () => {
    playTrackFromQueue(tracks, 0);

    expect(playNextTrack()).toBe(true);
    expect(get(currentTrack)?.id).toBe('t2');
    expect(get(currentTrackIndex)).toBe(1);
  });

  it('リピートなしでは最後のトラックで止まる', () => {
    playTrackFromQueue(tracks, 3);

    expect(playNextTrack()).toBe(false);
    expect(get(currentTrack)?.id).toBe('t4');
  });

  it('全曲リピートでは最後から先頭へ戻る', () => {
    repeatMode.set('all');
    playTrackFromQueue(tracks, 3);

    expect(playNextTrack()).toBe(true);
    expect(get(currentTrack)?.id).toBe('t1');
    expect(get(currentTrackIndex)).toBe(0);
  });

  it('1曲リピートでは同じトラックを再生する', () => {
    repeatMode.set('one');
    playTrackFromQueue(tracks, 1);

    expect(playNextTrack()).toBe(true);
    expect(get(currentTrack)?.id).toBe('t2');
    expect(get(currentTrackIndex)).toBe(1);
  });

  it('キューが空なら何もしない', () => {
    expect(playNextTrack()).toBe(false);
  });
});

describe('playPreviousTrack', () => {
  it('前のトラックへ戻る', () => {
    playTrackFromQueue(tracks, 2);

    expect(playPreviousTrack()).toBe(true);
    expect(get(currentTrack)?.id).toBe('t2');
  });

  it('3秒を超えて再生している場合はトラックの先頭へ戻る', () => {
    playTrackFromQueue(tracks, 2);
    currentTime.set(10);

    expect(playPreviousTrack()).toBe(true);
    expect(get(currentTime)).toBe(0);
    expect(get(currentTrack)?.id).toBe('t3');
  });

  it('リピートなしでは先頭のトラックで止まる', () => {
    playTrackFromQueue(tracks, 0);

    expect(playPreviousTrack()).toBe(false);
    expect(get(currentTrack)?.id).toBe('t1');
  });

  it('全曲リピートでは先頭から最後へ移動する', () => {
    repeatMode.set('all');
    playTrackFromQueue(tracks, 0);

    expect(playPreviousTrack()).toBe(true);
    expect(get(currentTrack)?.id).toBe('t4');
    expect(get(currentTrackIndex)).toBe(3);
  });
});

describe('toggleShuffle', () => {
  it('有効にすると再生中のトラックを先頭にして残りを並べ替える', () => {
    playTrackFromQueue(tracks, 2);
    toggleShuffle();

    const queue = ids(get(playQueue));
    expect(get(isShuffleEnabled)).toBe(true);
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(get(currentTrackIndex)).toBe(0);
  });

  it('無効にすると元の順序に戻し、再生中トラックの位置を合わせる', () => {
    playTrackFromQueue(tracks, 2);
    toggleShuffle();
    playNextTrack();
    const playing = get(currentTrack)?.id;

    toggleShuffle();

    expect(get(isShuffleEnabled)).toBe(false);
    expect(ids(get(playQueue))).toEqual(['t1', 't2', 't3', 't4']);
    expect(get(playQueue)[get(currentTrackIndex)].id).toBe(playing);
  });
});

describe('toggleRepeat', () => {
  it('off → all → one → off の順に切り替わる', () => {
    toggleRepeat();
    expect(get(repeatMode)).toBe('all');
    toggleRepeat();
    expect(get(repeatMode)).toBe('one');
    toggleRepeat();
    expect(get(repeatMode)).toBe('off');
  });
});

describe('removeFromQueue', () => {
  it('再生中以外のトラックを削除すると再生中トラックのインデックスを詰める', () => {
    playTrackFromQueue(tracks, 2);
    removeFromQueue('t1');

    expect(ids(get(playQueue))).toEqual(['t2', 't3', 't4']);
    expect(ids(get(originalQueue))).toEqual(['t2', 't3', 't4']);
    expect(get(currentTrackIndex)).toBe(1);
    expect(get(currentTrack)?.id).toBe('t3');
  });

  it('再生中のトラックを削除すると同じ位置の次のトラックに移る', () => {
    playTrackFromQueue(tracks, 1);
    removeFromQueue('t2');

    expect(get(currentTrack)?.id).toBe('t3');
    expect(get(currentTrackIndex)).toBe(1);
  });

  it('最後のトラックが再生中に削除されると新しい最後のトラックに移る', () => {
    playTrackFromQueue(tracks, 3);
    removeFromQueue('t4');

    expect(get(currentTrack)?.id).toBe('t3');
    expect(get(currentTrackIndex)).toBe(2);
  });

  it('キューが空になるとプレイヤーをリセットする', () => {
    playTrackFromQueue([makeTrack('only')], 0);
    removeFromQueue('only');

    expect(get(currentTrack)).toBeNull();
    expect(get(currentTrackIndex)).toBe(-1);
  });
});

describe('clearQueue', () => {
  it('再生中のトラックだけを残す', () => {
    playTrackFromQueue(tracks, 2);
    clearQueue();

    expect(ids(get(playQueue))).toEqual(['t3']);
    expect(get(currentTrackIndex)).toBe(0);
  });
});

describe('派生ストア', () => {
  it('hasNextTrack / hasPreviousTrack はリピートなしでは両端でfalseになる', () => {
    playTrackFromQueue(tracks, 0);
    expect(get(hasPreviousTrack)).toBe(false);
    expect(get(hasNextTrack)).toBe(true);

    playTrackFromQueue(tracks, 3);
    expect(get(hasPreviousTrack)).toBe(true);
    expect(get(hasNextTrack)).toBe(false);
  });

  it('リピート中はキューがあれば常に前後へ移動できる', () => {
    repeatMode.set('all');
    playTrackFromQueue(tracks, 3);

    expect(get(hasNextTrack)).toBe(true);
    expect(get(hasPreviousTrack)).toBe(true);
  });

  it('upcomingTracks は再生中より後ろのトラックを返す', () => {
    playTrackFromQueue(tracks, 1);
    expect(ids(get(upcomingTracks))).toEqual(['t3', 't4']);
  });
});

describe('formatTime', () => {
  it('秒数を m:ss 形式にする', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(65.9)).toBe('1:05');
    expect(formatTime(NaN)).toBe('0:00');
  });
});
