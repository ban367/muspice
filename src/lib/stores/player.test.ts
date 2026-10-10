import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  player,
  addNextInQueue,
  addToQueue,
  clearQueue,
  formatTime,
  moveUpcomingTrack,
  peekNextTrack,
  playNextTrack,
  playPreviousTrack,
  playQueueIndex,
  playShuffled,
  playTrackFromQueue,
  removeFromQueue,
  resetPlayer,
  restoreQueue,
  toggleRepeat,
  toggleShuffle
} from './player.svelte.js';

/** キュー操作の検証に必要なidだけを持つトラック */
function makeTrack(id: string): Track {
  return { id, title: id } as Track;
}

const tracks = ['t1', 't2', 't3', 't4'].map(makeTrack);
const ids = (list: Track[]) => list.map((t) => t.id);

beforeEach(() => {
  // ストアはモジュール単位のシングルトンのため、テストごとに初期化する
  resetPlayer();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('playTrackFromQueue', () => {
  it('キューを設定し、指定位置のトラックを再生する', () => {
    playTrackFromQueue(tracks, 2);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('範囲外のインデックスでは状態を変えない', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    playTrackFromQueue(tracks, 4);

    expect(player.playQueue).toEqual([]);
    expect(player.currentTrack).toBeNull();
  });

  it('シャッフル中は選択したトラックを先頭にし、残りを並べ替える', () => {
    player.isShuffleEnabled = true;
    playTrackFromQueue(tracks, 2);

    const queue = ids(player.playQueue);
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(0);
    // シャッフル解除時に戻すため元の順序を保持する
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3', 't4']);
  });
});

describe('playNextTrack', () => {
  it('次のトラックへ進む', () => {
    playTrackFromQueue(tracks, 0);

    expect(playNextTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('リピートなしでは最後のトラックで止まる', () => {
    playTrackFromQueue(tracks, 3);

    expect(playNextTrack()).toBe(false);
    expect(player.currentTrack?.id).toBe('t4');
  });

  it('全曲リピートでは最後から先頭へ戻る', () => {
    player.repeatMode = 'all';
    playTrackFromQueue(tracks, 3);

    expect(playNextTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t1');
    expect(player.currentTrackIndex).toBe(0);
  });

  it('1曲リピートでは同じトラックを再生する', () => {
    player.repeatMode = 'one';
    playTrackFromQueue(tracks, 1);

    expect(playNextTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('キューが空なら何もしない', () => {
    expect(playNextTrack()).toBe(false);
  });
});

describe('peekNextTrack', () => {
  it('playNextTrackで進む先のトラックを、状態を変えずに返す', () => {
    const cases: Array<{
      repeat: typeof player.repeatMode;
      index: number;
      expected: string | null;
    }> = [
      { repeat: 'off', index: 0, expected: 't2' },
      { repeat: 'off', index: 3, expected: null },
      { repeat: 'all', index: 3, expected: 't1' },
      { repeat: 'one', index: 1, expected: 't2' }
    ];
    for (const { repeat, index, expected } of cases) {
      player.repeatMode = repeat;
      playTrackFromQueue(tracks, index);

      expect(peekNextTrack()?.id ?? null).toBe(expected);
      // 状態は変えない
      expect(player.currentTrackIndex).toBe(index);

      playNextTrack();
      expect(player.currentTrack?.id).toBe(expected ?? tracks[index].id);
    }
  });

  it('キューが空ならnullを返す', () => {
    expect(peekNextTrack()).toBeNull();
  });
});

describe('playPreviousTrack', () => {
  it('前のトラックへ戻る', () => {
    playTrackFromQueue(tracks, 2);

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('3秒を超えて再生している場合はトラックの先頭へ戻る', () => {
    playTrackFromQueue(tracks, 2);
    player.currentTime = 10;

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTime).toBe(0);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('リピートなしでは先頭のトラックで止まる', () => {
    playTrackFromQueue(tracks, 0);

    expect(playPreviousTrack()).toBe(false);
    expect(player.currentTrack?.id).toBe('t1');
  });

  it('全曲リピートでは先頭から最後へ移動する', () => {
    player.repeatMode = 'all';
    playTrackFromQueue(tracks, 0);

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t4');
    expect(player.currentTrackIndex).toBe(3);
  });
});

describe('toggleShuffle', () => {
  it('有効にすると再生中のトラックを先頭にして残りを並べ替える', () => {
    playTrackFromQueue(tracks, 2);
    toggleShuffle();

    const queue = ids(player.playQueue);
    expect(player.isShuffleEnabled).toBe(true);
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(0);
  });

  it('無効にすると元の順序に戻し、再生中トラックの位置を合わせる', () => {
    playTrackFromQueue(tracks, 2);
    toggleShuffle();
    playNextTrack();
    const playing = player.currentTrack?.id;

    toggleShuffle();

    expect(player.isShuffleEnabled).toBe(false);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.playQueue[player.currentTrackIndex].id).toBe(playing);
  });
});

describe('playShuffled', () => {
  it('シャッフルモードを有効にし、ランダムに選んだトラックから再生する', () => {
    // Math.random() = 0.6 → 4曲中インデックス2（t3）を先頭にする
    vi.spyOn(Math, 'random').mockReturnValue(0.6);
    playShuffled(tracks);

    const queue = ids(player.playQueue);
    expect(player.isShuffleEnabled).toBe(true);
    expect(player.currentTrack?.id).toBe('t3');
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
  });

  it('元の順序を保持し、シャッフルを解除すると戻る', () => {
    playShuffled(tracks);
    const playing = player.currentTrack?.id;

    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3', 't4']);

    toggleShuffle();
    expect(player.isShuffleEnabled).toBe(false);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.playQueue[player.currentTrackIndex].id).toBe(playing);
  });

  it('空の一覧では何もしない', () => {
    playShuffled([]);
    expect(player.isShuffleEnabled).toBe(false);
    expect(player.currentTrack).toBeNull();
  });
});

describe('toggleRepeat', () => {
  it('off → all → one → off の順に切り替わる', () => {
    toggleRepeat();
    expect(player.repeatMode).toBe('all');
    toggleRepeat();
    expect(player.repeatMode).toBe('one');
    toggleRepeat();
    expect(player.repeatMode).toBe('off');
  });
});

describe('playQueueIndex', () => {
  it('キューの並びを変えずに、指定した位置のトラックへ移る', () => {
    playTrackFromQueue(tracks, 0);

    expect(playQueueIndex(2)).toBe(true);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
    expect(ids(player.upcomingTracks)).toEqual(['t4']);
  });

  it('再生中より前の位置へも移れる', () => {
    playTrackFromQueue(tracks, 3);

    expect(playQueueIndex(1)).toBe(true);

    expect(player.currentTrack?.id).toBe('t2');
    expect(ids(player.upcomingTracks)).toEqual(['t3', 't4']);
  });

  it('シャッフル中は並べ替えた後のキューの位置で移る', () => {
    player.isShuffleEnabled = true;
    playTrackFromQueue(tracks, 0);
    const queue = ids(player.playQueue);

    expect(playQueueIndex(3)).toBe(true);

    expect(ids(player.playQueue)).toEqual(queue);
    expect(player.currentTrack?.id).toBe(queue[3]);
  });

  it('範囲外・整数でない位置では状態を変えない', () => {
    playTrackFromQueue(tracks, 1);

    expect(playQueueIndex(-1)).toBe(false);
    expect(playQueueIndex(4)).toBe(false);
    expect(playQueueIndex(1.5)).toBe(false);

    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });
});

describe('addToQueue / addNextInQueue', () => {
  it('addToQueueはキューの最後に追加する', () => {
    playTrackFromQueue(tracks, 1);
    addToQueue([makeTrack('x'), makeTrack('y')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 'x', 'y']);
    expect(player.currentTrackIndex).toBe(1);
  });

  it('addNextInQueueは再生中のトラックの次に、渡した順で追加する', () => {
    playTrackFromQueue(tracks, 1);
    addNextInQueue([makeTrack('x'), makeTrack('y')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 'x', 'y', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('再生していない場合、addNextInQueueはキューの先頭に追加する', () => {
    addNextInQueue([makeTrack('x')]);

    expect(ids(player.playQueue)).toEqual(['x']);
  });

  it('数万曲をまとめて追加できる', () => {
    playTrackFromQueue(tracks, 0);
    const many = Array.from({ length: 200_000 }, (_, i) => makeTrack(`m${i}`));

    addNextInQueue(many);
    addToQueue(many);

    expect(player.playQueue).toHaveLength(400_004);
    expect(player.playQueue[1].id).toBe('m0');
    expect(player.playQueue[200_001].id).toBe('t2');
  });
});

describe('ファイルが見つからない曲', () => {
  const missing = { id: 'gone', title: 'gone', isMissing: true } as Track;
  const withMissing = [tracks[0], missing, tracks[1], tracks[2]];

  it('キューには入れず、選んだ曲から再生する', () => {
    playTrackFromQueue(withMissing, 2);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3']);
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('見つからない曲を選んだ場合は、再生もキューの変更もしない', () => {
    playTrackFromQueue(tracks, 0);
    playTrackFromQueue(withMissing, 1);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrack?.id).toBe('t1');
  });

  it('キューへの追加・シャッフル再生でも入れない', () => {
    playTrackFromQueue(tracks, 0);
    addToQueue([missing, makeTrack('x')]);
    addNextInQueue([missing, makeTrack('y')]);
    expect(ids(player.playQueue)).toEqual(['t1', 'y', 't2', 't3', 't4', 'x']);

    playShuffled([missing, makeTrack('only')]);
    expect(ids(player.playQueue)).toEqual(['only']);
    // 再生できる曲がなければ何もしない
    playShuffled([missing]);
    expect(ids(player.playQueue)).toEqual(['only']);
  });
});

describe('removeFromQueue', () => {
  it('再生中以外のトラックを削除すると再生中トラックのインデックスを詰める', () => {
    playTrackFromQueue(tracks, 2);
    removeFromQueue('t1');

    expect(ids(player.playQueue)).toEqual(['t2', 't3', 't4']);
    expect(ids(player.originalQueue)).toEqual(['t2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('再生中のトラックを削除すると同じ位置の次のトラックに移る', () => {
    playTrackFromQueue(tracks, 1);
    removeFromQueue('t2');

    expect(player.currentTrack?.id).toBe('t3');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('最後のトラックが再生中に削除されると新しい最後のトラックに移る', () => {
    playTrackFromQueue(tracks, 3);
    removeFromQueue('t4');

    expect(player.currentTrack?.id).toBe('t3');
    expect(player.currentTrackIndex).toBe(2);
  });

  it('キューが空になるとプレイヤーをリセットする', () => {
    playTrackFromQueue([makeTrack('only')], 0);
    removeFromQueue('only');

    expect(player.currentTrack).toBeNull();
    expect(player.currentTrackIndex).toBe(-1);
  });
});

describe('moveUpcomingTrack', () => {
  const queue = ['t1', 't2', 't3', 't4', 't5'].map(makeTrack);

  it('「次に再生」の曲を、落とした曲の位置へ動かす（下へ・上へ）', () => {
    playTrackFromQueue(queue, 1);
    // 次に再生: t3, t4, t5
    moveUpcomingTrack(0, 2);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't4', 't5', 't3']);

    moveUpcomingTrack(2, 0);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 't5']);

    moveUpcomingTrack(1, 0);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't4', 't3', 't5']);
  });

  it('再生中の曲と、その位置は変わらない。続けて再生する曲は、並べ替えた後の先頭になる', () => {
    playTrackFromQueue(queue, 1);
    moveUpcomingTrack(2, 0);

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
    expect(ids(player.upcomingTracks)).toEqual(['t5', 't3', 't4']);
    expect(peekNextTrack()?.id).toBe('t5');
  });

  it('シャッフルする前の順は変えない', () => {
    playTrackFromQueue(queue, 0);
    const original = ids(player.originalQueue);
    moveUpcomingTrack(0, 3);

    expect(ids(player.originalQueue)).toEqual(original);
  });

  it('範囲の外・同じ位置・再生していない場合は、何もしない', () => {
    playTrackFromQueue(queue, 1);
    const before = player.playQueue;

    moveUpcomingTrack(0, 3);
    moveUpcomingTrack(-1, 1);
    moveUpcomingTrack(1, 1);
    moveUpcomingTrack(0.5, 1);
    expect(player.playQueue).toBe(before);

    resetPlayer();
    moveUpcomingTrack(0, 1);
    expect(player.playQueue).toEqual([]);
  });
});

describe('restoreQueue', () => {
  it('キューと再生していたトラックを、再生していない状態で戻す', () => {
    const queue = [tracks[2], tracks[0], tracks[1]];
    const withDuration = { ...tracks[0], duration: 200 } as Track;
    queue[1] = withDuration;

    restoreQueue(queue, [withDuration, tracks[1], tracks[2]], 1);

    expect(ids(player.playQueue)).toEqual(['t3', 't1', 't2']);
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack).toBe(withDuration);
    expect(player.duration).toBe(200);
    expect(player.currentTime).toBe(0);
    expect(player.isPlaying).toBe(false);
  });

  it('範囲外の位置では何も戻さない', () => {
    restoreQueue(tracks, tracks, 9);

    expect(player.playQueue).toEqual([]);
    expect(player.currentTrack).toBeNull();
  });

  it('シャッフルしていたキューは、解除すると元の順に戻る', () => {
    player.isShuffleEnabled = true;
    restoreQueue([tracks[2], tracks[0], tracks[1]], [tracks[0], tracks[1], tracks[2]], 0);

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
  });
});

describe('clearQueue', () => {
  it('再生中のトラックだけを残す', () => {
    playTrackFromQueue(tracks, 2);
    clearQueue();

    expect(ids(player.playQueue)).toEqual(['t3']);
    expect(player.currentTrackIndex).toBe(0);
  });
});

describe('派生ストア', () => {
  it('hasNextTrack / hasPreviousTrack はリピートなしでは両端でfalseになる', () => {
    playTrackFromQueue(tracks, 0);
    expect(player.hasPreviousTrack).toBe(false);
    expect(player.hasNextTrack).toBe(true);

    playTrackFromQueue(tracks, 3);
    expect(player.hasPreviousTrack).toBe(true);
    expect(player.hasNextTrack).toBe(false);
  });

  it('リピート中はキューがあれば常に前後へ移動できる', () => {
    player.repeatMode = 'all';
    playTrackFromQueue(tracks, 3);

    expect(player.hasNextTrack).toBe(true);
    expect(player.hasPreviousTrack).toBe(true);
  });

  it('upcomingTracks は再生中より後ろのトラックを返す', () => {
    playTrackFromQueue(tracks, 1);
    expect(ids(player.upcomingTracks)).toEqual(['t3', 't4']);
  });
});

describe('formatTime', () => {
  it('秒数を m:ss 形式にする', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(65.9)).toBe('1:05');
    expect(formatTime(NaN)).toBe('0:00');
  });
});
