import { beforeEach, describe, expect, it } from 'vitest';
import { createPlayTracker, type PlayTracker } from './playTracker.js';

/** 再生回数・スキップ回数に数えた曲（数えた順） */
let played: string[];
let skipped: string[];
let tracker: PlayTracker;

/** 再生位置の通知（0.25秒ごと）を、`from`秒から`to`秒まで届ける */
function play(from: number, to: number): void {
  for (let position = from; position <= to + 1e-9; position += 0.25) {
    tracker.progress(position);
  }
}

beforeEach(() => {
  played = [];
  skipped = [];
  tracker = createPlayTracker({
    onPlayed: (trackId) => played.push(trackId),
    onSkipped: (trackId) => skipped.push(trackId)
  });
});

describe('再生回数', () => {
  it('曲の半分を聴いた時に、1回だけ数える', () => {
    tracker.begin('t1', 200);
    play(0, 99.5);
    expect(played).toEqual([]);

    play(99.75, 100);
    expect(played).toEqual(['t1']);

    // 最後まで聴いても、もう1回は数えない
    play(100.25, 200);
    tracker.finish();
    expect(played).toEqual(['t1']);
    expect(skipped).toEqual([]);
  });

  it('長い曲は、4分を聴いた時に数える', () => {
    tracker.begin('long', 1200);
    play(0, 239.5);
    expect(played).toEqual([]);

    play(239.75, 240);
    expect(played).toEqual(['long']);
  });

  it('シークで飛ばした分は、聴いた時間に含めない', () => {
    tracker.begin('t1', 200);
    play(0, 10);
    // 終わりの近くへシークして、最後まで再生した
    play(190, 200);
    tracker.finish();

    expect(played).toEqual([]);
  });

  it('シークをまたいで聴いた時間を合計する（戻って聴き直した分も含める）', () => {
    tracker.begin('t1', 200);
    play(0, 60);
    // 頭へ戻って、もう一度聴く
    play(0, 40);

    expect(played).toEqual(['t1']);
  });

  it('一時停止していた時間は含めない（再生位置が進んだ分だけを足す）', () => {
    tracker.begin('t1', 200);
    play(0, 50);
    // 一時停止の間は通知が届かず、再開すると続きから届く
    play(50.25, 99.75);
    expect(played).toEqual([]);

    play(100, 100.25);
    expect(played).toEqual(['t1']);
  });

  it('再生エンジンが読んだ曲の長さで判定する', () => {
    // ライブラリに長さがない曲: 再生エンジンが読むまでは、4分で判定する
    tracker.begin('t1', 0);
    play(0, 20);
    expect(played).toEqual([]);

    // 30秒の曲だと分かった時点で、もう半分を聴いている
    tracker.setDuration(30);
    expect(played).toEqual(['t1']);
  });

  it('長さが分からない曲は、終わりまで再生した時に数える', () => {
    tracker.begin('t1', 0);
    play(0, 90);
    expect(played).toEqual([]);

    tracker.finish();
    expect(played).toEqual(['t1']);
  });

  it('同じ曲を頭から再生し直すと、聴いた時間を数え直す（1曲リピートの繰り返し）', () => {
    tracker.begin('t1', 200);
    play(0, 200);
    tracker.finish();
    tracker.begin('t1', 200);
    play(0, 99);
    expect(played).toEqual(['t1']);

    play(99.25, 200);
    expect(played).toEqual(['t1', 't1']);
    expect(skipped).toEqual([]);
  });

  it('再生していない間の通知は無視する', () => {
    tracker.progress(10);
    tracker.setDuration(20);
    tracker.finish();

    expect(played).toEqual([]);
    expect(skipped).toEqual([]);
  });
});

describe('スキップ回数', () => {
  it('再生回数に数える前に別の曲へ移ったら、前の曲を数える', () => {
    tracker.begin('t1', 200);
    play(0, 30);
    tracker.begin('t2', 200);

    expect(skipped).toEqual(['t1']);
    expect(played).toEqual([]);
  });

  it('再生回数に数えた後に別の曲へ移っても、数えない', () => {
    tracker.begin('t1', 200);
    play(0, 120);
    tracker.begin('t2', 200);

    expect(played).toEqual(['t1']);
    expect(skipped).toEqual([]);
  });

  it('2秒に満たない曲は数えない（続けて次へ送っている途中の曲）', () => {
    tracker.begin('t1', 200);
    play(0, 1.75);
    tracker.begin('t2', 200);
    tracker.begin('t3', 200);
    expect(skipped).toEqual([]);

    play(0, 2);
    tracker.begin('t4', 200);
    expect(skipped).toEqual(['t3']);
  });

  it('同じ曲を頭から再生し直しても、数えない（「前へ」での頭出し）', () => {
    tracker.begin('t1', 200);
    play(0, 30);
    tracker.begin('t1', 200);

    expect(skipped).toEqual([]);
  });

  it('最後まで再生した曲は、聴いた時間が足りなくても数えない', () => {
    tracker.begin('t1', 200);
    play(0, 10);
    play(195, 200);
    tracker.finish();
    tracker.begin('t2', 200);

    expect(skipped).toEqual([]);
    expect(played).toEqual([]);
  });

  it('停止・再生の失敗の後に別の曲を再生しても、数えない', () => {
    tracker.begin('t1', 200);
    play(0, 30);
    tracker.reset();
    tracker.begin('t2', 200);

    expect(skipped).toEqual([]);
  });
});
