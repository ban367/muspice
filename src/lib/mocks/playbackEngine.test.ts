import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PlaybackEvent } from '#lib/types/models.js';
import { createMockPlaybackEngine, type MockPlaybackEngine } from './playbackEngine';

const durations: Record<string, number | null> = { a: 10, b: 20, endless: null };

let events: PlaybackEvent[];
let engine: MockPlaybackEngine;
let crossfadeSeconds: number;

/** 再生位置の通知を除いたイベント */
function transitions(): PlaybackEvent[] {
  return events.filter((event) => event.type !== 'position');
}

/** 最後に通知された再生位置 */
function lastPosition(): number | null | undefined {
  return events.findLast((event) => event.type === 'position')?.position;
}

beforeEach(() => {
  vi.useFakeTimers();
  events = [];
  crossfadeSeconds = 0;
  engine = createMockPlaybackEngine({
    emit: (event) => events.push(event),
    crossfadeSeconds: () => crossfadeSeconds,
    durationOf: (trackId) => {
      if (!(trackId in durations)) throw { code: 'NOT_FOUND', message: 'トラックが見つかりません' };
      return durations[trackId];
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('createMockPlaybackEngine', () => {
  it('時計に合わせて再生位置を通知し、曲の終わりで終了を通知する', () => {
    expect(engine.play('a', 1)).toEqual({ duration: 10 });

    vi.advanceTimersByTime(3000);
    expect(lastPosition()).toBe(3);
    expect(transitions()).toEqual([]);

    vi.advanceTimersByTime(7000);
    expect(transitions()).toEqual([{ type: 'ended', token: 1 }]);

    // 終わった後は通知しない
    const count = events.length;
    vi.advanceTimersByTime(5000);
    expect(events).toHaveLength(count);
  });

  it('続けて再生する曲へ、曲の終わりで切り替わる', () => {
    engine.play('a', 1);
    engine.setNext('b', 2);

    vi.advanceTimersByTime(10_500);

    expect(transitions()).toEqual([{ type: 'advanced', token: 2, duration: 20 }]);
    // 切り替えのタイマーが遅れた分は、次の曲の再生位置に含める
    expect(lastPosition()).toBe(0.5);

    vi.advanceTimersByTime(20_000);
    expect(transitions().at(-1)).toEqual({ type: 'ended', token: 2 });
  });

  it('続けて再生する曲を取り消すと、曲の終わりで終了する', () => {
    engine.play('a', 1);
    engine.setNext('b', 2);
    engine.setNext(null, 3);

    vi.advanceTimersByTime(11_000);

    expect(transitions()).toEqual([{ type: 'ended', token: 1 }]);
  });

  it('一時停止中は再生位置が進まず、再開すると続きから進む', () => {
    engine.play('a', 1);
    vi.advanceTimersByTime(2000);

    engine.pause();
    vi.advanceTimersByTime(60_000);
    expect(lastPosition()).toBe(2);
    expect(transitions()).toEqual([]);

    engine.resume();
    vi.advanceTimersByTime(1000);
    expect(lastPosition()).toBe(3);
  });

  it('シークした位置から再生を続ける（一時停止中のシークは、再開するまで進まない）', () => {
    engine.play('a', 1);
    vi.advanceTimersByTime(1000);

    engine.seek(8);
    expect(lastPosition()).toBe(8);
    vi.advanceTimersByTime(1000);
    expect(lastPosition()).toBe(9);

    engine.pause();
    engine.seek(4);
    vi.advanceTimersByTime(5000);
    expect(lastPosition()).toBe(4);

    // 曲の長さを超える位置は、曲の終わりになる
    engine.resume();
    engine.seek(99);
    vi.advanceTimersByTime(250);
    expect(transitions()).toEqual([{ type: 'ended', token: 1 }]);
  });

  it('再生すると、再生中の曲と続けて再生する曲を置き換える', () => {
    engine.play('a', 1);
    engine.setNext('b', 2);

    engine.play('b', 3);
    vi.advanceTimersByTime(20_000);

    expect(transitions()).toEqual([{ type: 'ended', token: 3 }]);
  });

  it('逆の順で届いた古い要求では、新しい再生を置き換えない', () => {
    engine.play('b', 5);

    expect(() => engine.play('a', 4)).toThrow(expect.objectContaining({ code: 'PLAYBACK' }));
    engine.setNext('a', 4);

    vi.advanceTimersByTime(20_000);
    expect(transitions()).toEqual([{ type: 'ended', token: 5 }]);
  });

  it('再生できない曲では、エラーを投げて再生しない', () => {
    expect(() => engine.play('missing', 1)).toThrow(expect.objectContaining({ code: 'NOT_FOUND' }));

    vi.advanceTimersByTime(5000);
    expect(events).toEqual([]);
  });

  it('長さの分からない曲は、止めるまで再生を続ける', () => {
    expect(engine.play('endless', 1)).toEqual({ duration: null });
    vi.advanceTimersByTime(3_600_000);
    expect(transitions()).toEqual([]);

    engine.stop();
    const count = events.length;
    vi.advanceTimersByTime(5000);
    expect(events).toHaveLength(count);
  });

  it('クロスフェードでは、重なりの始まりで次の曲へ切り替わる', () => {
    crossfadeSeconds = 3;
    engine.play('a', 1); // 10秒
    engine.setNext('b', 2); // 20秒

    vi.advanceTimersByTime(6750);
    expect(transitions()).toEqual([]);

    // 終わりの3秒前（7秒）で切り替わる
    vi.advanceTimersByTime(500);
    expect(transitions()).toEqual([{ type: 'advanced', token: 2, duration: 20 }]);
    expect(lastPosition()).toBe(0.25);
  });

  it('クロスフェードは曲の長さの半分までで、同じ曲の繰り返しではしない', () => {
    crossfadeSeconds = 12;
    engine.play('a', 1); // 10秒 → 重ねるのは5秒まで
    engine.setNext('b', 2);
    vi.advanceTimersByTime(5000);
    expect(transitions()).toEqual([{ type: 'advanced', token: 2, duration: 20 }]);

    // 1曲リピート: 同じ曲は、終わりで切れ目なく続ける
    engine.play('a', 3);
    engine.setNext('a', 4);
    vi.advanceTimersByTime(9750);
    expect(transitions().at(-1)).toEqual({ type: 'advanced', token: 2, duration: 20 });
    vi.advanceTimersByTime(250);
    expect(transitions().at(-1)).toEqual({ type: 'advanced', token: 4, duration: 10 });
  });
});
