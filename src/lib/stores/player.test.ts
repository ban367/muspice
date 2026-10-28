import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import type { QueueEntry } from '#lib/utils/playQueue.js';
import {
  player,
  addNextInQueue,
  addToQueue,
  clearQueue,
  moveUpcomingEntry,
  peekNextTrack,
  playNextTrack,
  playPreviousTrack,
  playNow,
  playQueueIndex,
  playShuffled,
  replaceQueueAndPlay,
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
const ids = (queue: readonly QueueEntry[]) => queue.map((entry) => entry.track.id);
/** 再生キューの`index`の位置の要素のID */
const entryIdAt = (index: number) => player.playQueue[index].entryId;
/** 「次に再生」の`index`の位置の要素のID */
const upcomingIdAt = (index: number) => player.upcomingEntries[index].entryId;

beforeEach(() => {
  // ストアはモジュール単位のシングルトンのため、テストごとに初期化する
  resetPlayer();
  player.isShuffleEnabled = false;
  player.repeatMode = 'off';
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('replaceQueueAndPlay', () => {
  it('キューを設定し、指定位置のトラックを再生する', () => {
    replaceQueueAndPlay(tracks, 2);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('同じ曲が複数ある一覧では、選んだ位置の曲から再生する', () => {
    replaceQueueAndPlay([tracks[0], tracks[1], tracks[0]], 2);

    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t1');
    expect(peekNextTrack()).toBeNull();
  });

  it('範囲外のインデックスでは状態を変えない', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    replaceQueueAndPlay(tracks, 4);

    expect(player.playQueue).toEqual([]);
    expect(player.currentTrack).toBeNull();
  });

  it('シャッフル中は選択したトラックを先頭にし、残りを並べ替える', () => {
    player.isShuffleEnabled = true;
    replaceQueueAndPlay(tracks, 2);

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
    replaceQueueAndPlay(tracks, 0);

    expect(playNextTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('リピートなしでは最後のトラックで止まる', () => {
    replaceQueueAndPlay(tracks, 3);

    expect(playNextTrack()).toBe(false);
    expect(player.currentTrack?.id).toBe('t4');
  });

  it('全曲リピートでは最後から先頭へ戻る', () => {
    player.repeatMode = 'all';
    replaceQueueAndPlay(tracks, 3);

    expect(playNextTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t1');
    expect(player.currentTrackIndex).toBe(0);
  });

  it('1曲リピートでは同じトラックを再生する', () => {
    player.repeatMode = 'one';
    replaceQueueAndPlay(tracks, 1);

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
      replaceQueueAndPlay(tracks, index);

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
    replaceQueueAndPlay(tracks, 2);

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('3秒を超えて再生している場合はトラックの先頭へ戻る', () => {
    replaceQueueAndPlay(tracks, 2);
    player.currentTime = 10;

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTime).toBe(0);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('リピートなしでは先頭のトラックで止まる', () => {
    replaceQueueAndPlay(tracks, 0);

    expect(playPreviousTrack()).toBe(false);
    expect(player.currentTrack?.id).toBe('t1');
  });

  it('全曲リピートでは先頭から最後へ移動する', () => {
    player.repeatMode = 'all';
    replaceQueueAndPlay(tracks, 0);

    expect(playPreviousTrack()).toBe(true);
    expect(player.currentTrack?.id).toBe('t4');
    expect(player.currentTrackIndex).toBe(3);
  });
});

describe('toggleShuffle', () => {
  it('有効にすると再生中のトラックを先頭にして残りを並べ替える', () => {
    replaceQueueAndPlay(tracks, 2);
    toggleShuffle();

    const queue = ids(player.playQueue);
    expect(player.isShuffleEnabled).toBe(true);
    expect(queue[0]).toBe('t3');
    expect([...queue].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(0);
  });

  it('無効にすると元の順序に戻し、再生中トラックの位置を合わせる', () => {
    replaceQueueAndPlay(tracks, 2);
    toggleShuffle();
    playNextTrack();
    const playing = player.currentTrack?.id;

    toggleShuffle();

    expect(player.isShuffleEnabled).toBe(false);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.playQueue[player.currentTrackIndex].track.id).toBe(playing);
  });
});

describe('toggleShuffle（何も再生していない時）', () => {
  it('キュー全体を並べ替え、解除すると追加した順に戻る', () => {
    addToQueue(tracks);
    // 毎回いちばん前の要素と入れ替える乱数（並びが必ず変わる）
    vi.spyOn(Math, 'random').mockReturnValue(0);

    toggleShuffle();
    expect(player.isShuffleEnabled).toBe(true);
    expect(player.currentTrackIndex).toBe(-1);
    expect(ids(player.playQueue)).not.toEqual(['t1', 't2', 't3', 't4']);
    expect([...ids(player.playQueue)].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(ids(player.upcomingEntries)).toEqual(ids(player.playQueue));

    toggleShuffle();
    expect(player.isShuffleEnabled).toBe(false);
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(-1);
  });
});

describe('playNow', () => {
  const albumTracks = ['a1', 'a2', 'a3'].map(makeTrack);

  it('何も再生していなければ、キューの先頭に入れて再生する', () => {
    playNow([tracks[1]]);

    expect(ids(player.playQueue)).toEqual(['t2']);
    expect(player.currentTrackIndex).toBe(0);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('何も再生していない時にキューへ入れた曲は残し、その前に入れて再生する', () => {
    addToQueue([tracks[0], tracks[1]]);
    playNow([tracks[2]]);

    expect(ids(player.playQueue)).toEqual(['t3', 't1', 't2']);
    expect(player.currentTrackIndex).toBe(0);
    expect(ids(player.upcomingEntries)).toEqual(['t1', 't2']);
  });

  it('再生中の曲の次に入れて再生し、キューのほかの曲は残す', () => {
    replaceQueueAndPlay(tracks, 1);
    playNow([makeTrack('x')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 'x', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('x');
    // 入れた曲の後は、それまでの「次に再生」の曲へ続く
    expect(ids(player.upcomingEntries)).toEqual(['t3', 't4']);
  });

  it('アルバムなどの複数の曲は、渡した順に入れて最初の曲から再生する', () => {
    replaceQueueAndPlay(tracks, 0);
    playNow(albumTracks);

    expect(ids(player.playQueue)).toEqual(['t1', 'a1', 'a2', 'a3', 't2', 't3', 't4']);
    expect(player.currentTrack?.id).toBe('a1');
    expect(peekNextTrack()?.id).toBe('a2');
  });

  it('キューの最後の曲の後に再生すると、最後に入る', () => {
    replaceQueueAndPlay(tracks, 3);
    playNow([makeTrack('x')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 'x']);
    expect(player.currentTrackIndex).toBe(4);
  });

  it('再生中の曲と同じ曲も、別の要素として次に入れる', () => {
    replaceQueueAndPlay(tracks, 0);
    const playingEntryId = entryIdAt(0);
    playNow([tracks[0]]);

    expect(ids(player.playQueue)).toEqual(['t1', 't1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(entryIdAt(1)).not.toBe(playingEntryId);
  });

  it('シャッフル中は最初の曲から再生し、残りを並べ替える。解除すると渡した順に戻る', () => {
    replaceQueueAndPlay(tracks, 0);
    toggleShuffle();
    playNow(albumTracks);

    const queue = ids(player.playQueue);
    expect(player.currentTrack?.id).toBe('a1');
    expect(queue[player.currentTrackIndex]).toBe('a1');
    expect([...queue.slice(2, 4)].sort()).toEqual(['a2', 'a3']);
    expect(ids(player.originalQueue)).toEqual(['t1', 'a1', 'a2', 'a3', 't2', 't3', 't4']);

    toggleShuffle();
    expect(ids(player.playQueue)).toEqual(['t1', 'a1', 'a2', 'a3', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
  });
});

describe('playShuffled', () => {
  it('ランダムな順で再生中の曲の次に入れて再生する（キューのほかの曲は残す）', () => {
    replaceQueueAndPlay([makeTrack('x'), makeTrack('y')], 0);
    // Math.random() = 0.6 → 4曲を並べ替える（Fisher-Yates）
    vi.spyOn(Math, 'random').mockReturnValue(0.6);
    playShuffled(tracks);

    const queue = ids(player.playQueue);
    expect(queue[0]).toBe('x');
    expect(queue[queue.length - 1]).toBe('y');
    expect([...queue.slice(1, 5)].sort()).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe(queue[1]);
  });

  it('シャッフルモードは変えない。シャッフルする前の順には、渡した順で入れる', () => {
    playShuffled(tracks);

    expect(player.isShuffleEnabled).toBe(false);
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3', 't4']);
  });

  it('空の一覧では何もしない', () => {
    playShuffled([]);
    expect(player.currentTrack).toBeNull();
    expect(player.playQueue).toEqual([]);
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
    replaceQueueAndPlay(tracks, 0);

    expect(playQueueIndex(2)).toBe(true);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
    expect(ids(player.upcomingEntries)).toEqual(['t4']);
  });

  it('再生中より前の位置へも移れる', () => {
    replaceQueueAndPlay(tracks, 3);

    expect(playQueueIndex(1)).toBe(true);

    expect(player.currentTrack?.id).toBe('t2');
    expect(ids(player.upcomingEntries)).toEqual(['t3', 't4']);
  });

  it('シャッフル中は並べ替えた後のキューの位置で移る', () => {
    player.isShuffleEnabled = true;
    replaceQueueAndPlay(tracks, 0);
    const queue = ids(player.playQueue);

    expect(playQueueIndex(3)).toBe(true);

    expect(ids(player.playQueue)).toEqual(queue);
    expect(player.currentTrack?.id).toBe(queue[3]);
  });

  it('範囲外・整数でない位置では状態を変えない', () => {
    replaceQueueAndPlay(tracks, 1);

    expect(playQueueIndex(-1)).toBe(false);
    expect(playQueueIndex(4)).toBe(false);
    expect(playQueueIndex(1.5)).toBe(false);

    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });
});

describe('addToQueue / addNextInQueue', () => {
  it('addToQueueはキューの最後に追加する', () => {
    replaceQueueAndPlay(tracks, 1);
    addToQueue([makeTrack('x'), makeTrack('y')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 'x', 'y']);
    expect(player.currentTrackIndex).toBe(1);
  });

  it('addNextInQueueは再生中のトラックの次に、渡した順で追加する', () => {
    replaceQueueAndPlay(tracks, 1);
    addNextInQueue([makeTrack('x'), makeTrack('y')]);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 'x', 'y', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('再生していない場合、addNextInQueueはキューの先頭に追加する', () => {
    addNextInQueue([makeTrack('x')]);

    expect(ids(player.playQueue)).toEqual(['x']);
  });

  it('何も再生していない時に追加した曲は、「次に再生」に出て、「次へ」で先頭から再生する', () => {
    addToQueue([makeTrack('x'), makeTrack('y')]);
    addNextInQueue([makeTrack('w')]);

    expect(player.currentTrackIndex).toBe(-1);
    expect(ids(player.upcomingEntries)).toEqual(['w', 'x', 'y']);
    expect(player.hasNextTrack).toBe(true);

    playNextTrack();
    expect(player.currentTrack?.id).toBe('w');
    expect(ids(player.upcomingEntries)).toEqual(['x', 'y']);
  });

  it('数万曲をまとめて追加できる', () => {
    replaceQueueAndPlay(tracks, 0);
    const many = Array.from({ length: 200_000 }, (_, i) => makeTrack(`m${i}`));

    addNextInQueue(many);
    addToQueue(many);

    expect(player.playQueue).toHaveLength(400_004);
    expect(player.playQueue[1].track.id).toBe('m0');
    expect(player.playQueue[200_001].track.id).toBe('t2');
  });
});

describe('ファイルが見つからない曲', () => {
  const missing = { id: 'gone', title: 'gone', isMissing: true } as Track;
  const withMissing = [tracks[0], missing, tracks[1], tracks[2]];

  it('キューには入れず、選んだ曲から再生する', () => {
    replaceQueueAndPlay(withMissing, 2);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3']);
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('見つからない曲を選んだ場合は、再生もキューの変更もしない', () => {
    replaceQueueAndPlay(tracks, 0);
    replaceQueueAndPlay(withMissing, 1);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrack?.id).toBe('t1');
  });

  it('キューへの追加・シャッフル再生でも入れない', () => {
    replaceQueueAndPlay(tracks, 0);
    addToQueue([missing, makeTrack('x')]);
    addNextInQueue([missing, makeTrack('y')]);
    expect(ids(player.playQueue)).toEqual(['t1', 'y', 't2', 't3', 't4', 'x']);

    playShuffled([missing, makeTrack('only')]);
    expect(ids(player.playQueue)).toEqual(['t1', 'only', 'y', 't2', 't3', 't4', 'x']);
    // 再生できる曲がなければ何もしない
    playShuffled([missing]);
    expect(ids(player.playQueue)).toEqual(['t1', 'only', 'y', 't2', 't3', 't4', 'x']);
    expect(player.currentTrack?.id).toBe('only');
  });

  it('次のキューに追加して再生でも入れず、どれも見つからなければ再生もキューの変更もしない', () => {
    replaceQueueAndPlay(tracks, 0);
    playNow([missing, makeTrack('x')]);
    expect(ids(player.playQueue)).toEqual(['t1', 'x', 't2', 't3', 't4']);
    expect(player.currentTrack?.id).toBe('x');

    playNow([missing]);
    expect(ids(player.playQueue)).toEqual(['t1', 'x', 't2', 't3', 't4']);
    expect(player.currentTrack?.id).toBe('x');
  });
});

describe('removeFromQueue', () => {
  it('再生中以外のトラックを削除すると再生中トラックのインデックスを詰める', () => {
    replaceQueueAndPlay(tracks, 2);
    removeFromQueue(entryIdAt(0));

    expect(ids(player.playQueue)).toEqual(['t2', 't3', 't4']);
    expect(ids(player.originalQueue)).toEqual(['t2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('再生中のトラックを削除すると同じ位置の次のトラックに移る', () => {
    replaceQueueAndPlay(tracks, 1);
    removeFromQueue(entryIdAt(1));

    expect(player.currentTrack?.id).toBe('t3');
    expect(player.currentTrackIndex).toBe(1);
  });

  it('最後のトラックが再生中に削除されると新しい最後のトラックに移る', () => {
    replaceQueueAndPlay(tracks, 3);
    removeFromQueue(entryIdAt(3));

    expect(player.currentTrack?.id).toBe('t3');
    expect(player.currentTrackIndex).toBe(2);
  });

  it('キューが空になるとプレイヤーをリセットする', () => {
    replaceQueueAndPlay([makeTrack('only')], 0);
    removeFromQueue(entryIdAt(0));

    expect(player.currentTrack).toBeNull();
    expect(player.currentTrackIndex).toBe(-1);
  });
});

describe('playRequestCount', () => {
  it('曲を選んで再生する操作のたびに増える（同じ曲を選び直した場合も）', () => {
    const before = player.playRequestCount;

    replaceQueueAndPlay(tracks, 0);
    replaceQueueAndPlay(tracks, 0);
    playQueueIndex(2);
    playShuffled(tracks);
    playNow([tracks[0]]);

    expect(player.playRequestCount).toBe(before + 5);
  });

  it('次へ・前へ・復元・範囲の外の指定では増えない', () => {
    replaceQueueAndPlay(tracks, 0);
    const before = player.playRequestCount;

    playNextTrack();
    playPreviousTrack();
    playQueueIndex(99);
    restoreQueue(tracks, tracks, 1);

    expect(player.playRequestCount).toBe(before);
  });
});

describe('moveUpcomingEntry', () => {
  const queue = ['t1', 't2', 't3', 't4', 't5'].map(makeTrack);

  it('「次に再生」の曲を、落とした曲の位置へ動かす（下へ・上へ）', () => {
    replaceQueueAndPlay(queue, 1);
    // 次に再生: t3, t4, t5
    moveUpcomingEntry(upcomingIdAt(0), upcomingIdAt(2));
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't4', 't5', 't3']);

    moveUpcomingEntry(upcomingIdAt(2), upcomingIdAt(0));
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 't5']);

    moveUpcomingEntry(upcomingIdAt(1), upcomingIdAt(0));
    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't4', 't3', 't5']);
  });

  it('再生中の曲と、その位置は変わらない。続けて再生する曲は、並べ替えた後の先頭になる', () => {
    replaceQueueAndPlay(queue, 1);
    moveUpcomingEntry(upcomingIdAt(2), upcomingIdAt(0));

    expect(player.currentTrack?.id).toBe('t2');
    expect(player.currentTrackIndex).toBe(1);
    expect(ids(player.upcomingEntries)).toEqual(['t5', 't3', 't4']);
    expect(peekNextTrack()?.id).toBe('t5');
  });

  it('シャッフルする前の順は変えない', () => {
    replaceQueueAndPlay(queue, 0);
    const original = ids(player.originalQueue);
    moveUpcomingEntry(upcomingIdAt(0), upcomingIdAt(3));

    expect(ids(player.originalQueue)).toEqual(original);
  });

  it('ドラッグしている間に曲が進んでも、ドラッグした曲を落とした曲の位置へ動かす', () => {
    replaceQueueAndPlay(queue, 0);
    // 次に再生: t2, t3, t4, t5。t4をドラッグし始めた後で、曲がt2へ進んだ
    const dragged = upcomingIdAt(2);
    playNextTrack();
    moveUpcomingEntry(dragged, upcomingIdAt(0));

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't4', 't3', 't5']);
  });

  it('「次に再生」にない要素（再生中・再生し終えた・キューにない）・同じ要素では、何もしない', () => {
    replaceQueueAndPlay(queue, 1);
    const before = player.playQueue;

    moveUpcomingEntry(entryIdAt(1), upcomingIdAt(1));
    moveUpcomingEntry(entryIdAt(0), upcomingIdAt(1));
    moveUpcomingEntry(upcomingIdAt(1), entryIdAt(1));
    moveUpcomingEntry(-1, upcomingIdAt(1));
    moveUpcomingEntry(upcomingIdAt(1), upcomingIdAt(1));
    expect(player.playQueue).toBe(before);

    const [first, second] = [entryIdAt(0), entryIdAt(1)];
    resetPlayer();
    moveUpcomingEntry(first, second);
    expect(player.playQueue).toEqual([]);
  });
});

describe('moveUpcomingEntry（何も再生していない時）', () => {
  it('キューのどの曲も動かせる', () => {
    addToQueue(tracks);

    moveUpcomingEntry(entryIdAt(3), entryIdAt(0));

    expect(ids(player.playQueue)).toEqual(['t4', 't1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(-1);
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

  it('何も再生していなかったキューは、すべてを「次に再生」に戻す', () => {
    restoreQueue(tracks, tracks, null);

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4']);
    expect(player.currentTrackIndex).toBe(-1);
    expect(player.currentTrack).toBeNull();
    expect(ids(player.upcomingEntries)).toEqual(['t1', 't2', 't3', 't4']);
  });

  it('シャッフルしていたキューは、解除すると元の順に戻る', () => {
    player.isShuffleEnabled = true;
    restoreQueue([tracks[2], tracks[0], tracks[1]], [tracks[0], tracks[1], tracks[2]], 0);

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(2);
    expect(player.currentTrack?.id).toBe('t3');
  });

  it('同じ曲が複数ある場合は、シャッフルする前の順の同じ曲へ、前から順に要素を対応付ける', () => {
    player.isShuffleEnabled = true;
    // 再生する順: t1, t2, t1（再生中）。シャッフルする前の順: t2, t1, t1
    restoreQueue([tracks[0], tracks[1], tracks[0]], [tracks[1], tracks[0], tracks[0]], 2);

    const entryIds = (queue: readonly QueueEntry[]) => queue.map((entry) => entry.entryId).sort();
    expect(entryIds(player.originalQueue)).toEqual(entryIds(player.playQueue));

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t2', 't1', 't1']);
    expect(player.currentTrackIndex).toBe(2);
  });
});

describe('同じ曲を重ねて入れたキュー', () => {
  it('同じ曲を何度でも入れられ、要素ごとに別のIDを振る', () => {
    replaceQueueAndPlay(tracks, 0);
    addToQueue([tracks[1]]);
    addNextInQueue([tracks[0]]);

    expect(ids(player.playQueue)).toEqual(['t1', 't1', 't2', 't3', 't4', 't2']);
    const entryIds = player.playQueue.map((entry) => entry.entryId);
    expect(new Set(entryIds).size).toBe(entryIds.length);
  });

  it('削除は指定した要素だけを消し、再生中の曲と同じ曲を「次に再生」から消しても再生中の曲は変わらない', () => {
    replaceQueueAndPlay(tracks, 1);
    addNextInQueue([tracks[1]]);
    addToQueue([tracks[1]]);
    // t1, t2（再生中）, t2, t3, t4, t2

    removeFromQueue(upcomingIdAt(0));

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 't2']);
    expect(ids(player.originalQueue)).toEqual(['t1', 't2', 't3', 't4', 't2']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
    expect(peekNextTrack()?.id).toBe('t3');
  });

  it('再生中の曲より前にある同じ曲を消すと、再生中の位置を詰める', () => {
    replaceQueueAndPlay([tracks[0], tracks[1], tracks[0]], 2);

    removeFromQueue(entryIdAt(0));

    expect(ids(player.playQueue)).toEqual(['t2', 't1']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t1');
  });

  it('キューにない要素のIDでは、何もしない', () => {
    replaceQueueAndPlay(tracks, 0);
    const before = player.playQueue;

    removeFromQueue(-1);

    expect(player.playQueue).toBe(before);
  });

  it('シャッフルを有効にしても、同じ曲の別の要素は残る', () => {
    replaceQueueAndPlay([tracks[0], tracks[1], tracks[0], tracks[0]], 2);
    const playing = entryIdAt(2);

    toggleShuffle();

    expect(entryIdAt(0)).toBe(playing);
    expect(ids(player.playQueue).sort()).toEqual(['t1', 't1', 't1', 't2']);
  });

  it('シャッフルを解除すると、同じ曲の別の要素ではなく、再生中の要素の位置へ戻る', () => {
    replaceQueueAndPlay([tracks[0], tracks[1], tracks[0], tracks[2]], 2);

    toggleShuffle();
    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't1', 't3']);
    expect(player.currentTrackIndex).toBe(2);
    expect(peekNextTrack()?.id).toBe('t3');
  });

  it('並べ替えは、同じ曲の別の要素ではなく、ドラッグした要素を動かす', () => {
    replaceQueueAndPlay([tracks[0], tracks[1], tracks[2], tracks[1]], 0);
    // 次に再生: t2, t3, t2（2つ目）
    const second = upcomingIdAt(2);

    moveUpcomingEntry(second, upcomingIdAt(0));

    expect(ids(player.upcomingEntries)).toEqual(['t2', 't2', 't3']);
    expect(upcomingIdAt(0)).toBe(second);
  });
});

describe('シャッフル中のキューへの追加', () => {
  beforeEach(() => {
    player.isShuffleEnabled = true;
    replaceQueueAndPlay(tracks, 1);
  });

  it('最後に追加した曲は、シャッフルを解除しても最後に残る', () => {
    addToQueue([makeTrack('x')]);
    expect(ids(player.playQueue).at(-1)).toBe('x');

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 't3', 't4', 'x']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('t2');
  });

  it('「次に再生」に追加した曲は、シャッフルを解除しても再生中の曲の次に残る', () => {
    addNextInQueue([makeTrack('x'), makeTrack('y')]);
    expect(ids(player.upcomingEntries).slice(0, 2)).toEqual(['x', 'y']);

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 't2', 'x', 'y', 't3', 't4']);
    expect(peekNextTrack()?.id).toBe('x');
  });
});

describe('シャッフルの解除で、シャッフルする前の順に再生中の曲がない場合', () => {
  it('再生中の位置へ入れ、「次へ」はその後ろの曲へ進む', () => {
    player.isShuffleEnabled = true;
    // 古い版で保存したキュー: シャッフル中に追加した曲（x）が、シャッフルする前の順にない
    restoreQueue(
      [tracks[2], makeTrack('x'), tracks[0], tracks[1]],
      [tracks[0], tracks[1], tracks[2]],
      1
    );

    toggleShuffle();

    expect(ids(player.playQueue)).toEqual(['t1', 'x', 't2', 't3']);
    expect(player.currentTrackIndex).toBe(1);
    expect(player.currentTrack?.id).toBe('x');
    expect(peekNextTrack()?.id).toBe('t2');
  });
});

describe('clearQueue', () => {
  it('再生中のトラックも含めて、キューを空にする', () => {
    replaceQueueAndPlay(tracks, 2);
    toggleShuffle();
    clearQueue();

    expect(player.playQueue).toEqual([]);
    expect(player.originalQueue).toEqual([]);
    expect(player.currentTrackIndex).toBe(-1);
    expect(player.currentTrack).toBeNull();
    expect(player.isPlaying).toBe(false);
    expect(player.upcomingEntries).toEqual([]);
  });
});

describe('派生ストア', () => {
  it('hasNextTrack / hasPreviousTrack はリピートなしでは両端でfalseになる', () => {
    replaceQueueAndPlay(tracks, 0);
    expect(player.hasPreviousTrack).toBe(false);
    expect(player.hasNextTrack).toBe(true);

    replaceQueueAndPlay(tracks, 3);
    expect(player.hasPreviousTrack).toBe(true);
    expect(player.hasNextTrack).toBe(false);
  });

  it('リピート中はキューがあれば常に前後へ移動できる', () => {
    player.repeatMode = 'all';
    replaceQueueAndPlay(tracks, 3);

    expect(player.hasNextTrack).toBe(true);
    expect(player.hasPreviousTrack).toBe(true);
  });

  it('upcomingEntries は再生中より後ろの要素を返す', () => {
    replaceQueueAndPlay(tracks, 1);
    expect(ids(player.upcomingEntries)).toEqual(['t3', 't4']);
  });
});
