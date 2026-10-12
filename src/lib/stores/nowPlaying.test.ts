import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { NowPlayingUpdate } from '#lib/bindings.js';
import {
  createNowPlayingReporter,
  type NowPlayingReporter,
  type NowPlayingState
} from './nowPlaying.js';

/** 時計（ms）。テストから進める */
let clock: number;
/** OSへ伝えた内容 */
let sent: (NowPlayingUpdate | null)[];
/** 呼び出しの結果を返す関数（結果を待たせるテストで使う） */
let resolvers: (() => void)[];
/** 呼び出しの結果を、テストが返すまで待たせるか */
let hold: boolean;
let reporter: NowPlayingReporter;

/** 非同期の処理（呼び出しの結果待ち）が進むのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function state(overrides: Partial<NowPlayingState> = {}): NowPlayingState {
  return { trackId: 't1', playing: true, position: 0, duration: 180, ...overrides };
}

/** 状態を伝え、呼び出しが済むのを待つ（呼び出しは1つずつ行われるため） */
async function report(next: NowPlayingState | null): Promise<void> {
  reporter.update(next);
  await flush();
}

beforeEach(() => {
  clock = 0;
  sent = [];
  resolvers = [];
  hold = false;
  reporter = createNowPlayingReporter(
    (update) => {
      sent.push(update);
      if (!hold) return Promise.resolve();
      return new Promise<void>((resolve) => resolvers.push(resolve));
    },
    () => clock
  );
});

describe('createNowPlayingReporter', () => {
  it('曲・再生中かどうか・位置・長さを伝える', async () => {
    await report(state({ position: 12.5, duration: 180.4 }));

    expect(sent).toEqual([{ trackId: 't1', playing: true, position: 12.5, duration: 180.4 }]);
  });

  it('曲の長さが分からなければ、nullで伝える', async () => {
    await report(state({ duration: 0 }));

    expect(sent[0]?.duration).toBeNull();
  });

  it('再生が進んでいるだけなら、伝え直さない', async () => {
    await report(state({ position: 10 }));
    clock += 250;
    await report(state({ position: 10.25 }));
    clock += 60_000;
    await report(state({ position: 70.3 }));

    expect(sent).toHaveLength(1);
  });

  it('シークしたら、位置を伝え直す', async () => {
    await report(state({ position: 10 }));
    clock += 250;
    await report(state({ position: 95 }));

    expect(sent).toHaveLength(2);
    expect(sent[1]).toMatchObject({ trackId: 't1', playing: true, position: 95 });

    // 伝え直した位置から、また計算する
    clock += 5000;
    await report(state({ position: 100.1 }));
    expect(sent).toHaveLength(2);
  });

  it('再生が途切れて位置が遅れたら、伝え直す', async () => {
    await report(state({ position: 10 }));
    clock += 5000;
    // 5秒たったのに、2秒しか進んでいない
    await report(state({ position: 12 }));

    expect(sent).toHaveLength(2);
    expect(sent[1]?.position).toBe(12);
  });

  it('一時停止と再開を伝える（一時停止中は、時間がたっても位置は進まない）', async () => {
    await report(state({ position: 10 }));
    clock += 2000;
    await report(state({ position: 12, playing: false }));
    clock += 60_000;
    await report(state({ position: 12, playing: false }));
    expect(sent).toHaveLength(2);
    expect(sent[1]).toMatchObject({ playing: false, position: 12 });

    // 一時停止中のシーク
    await report(state({ position: 40, playing: false }));
    expect(sent).toHaveLength(3);

    await report(state({ position: 40, playing: true }));
    expect(sent).toHaveLength(4);
    expect(sent[3]).toMatchObject({ playing: true, position: 40 });
  });

  it('曲が変わったら伝える', async () => {
    await report(state({ position: 179 }));
    clock += 1000;
    await report(state({ trackId: 't2', position: 0, duration: 200 }));

    expect(sent).toHaveLength(2);
    expect(sent[1]).toEqual({ trackId: 't2', playing: true, position: 0, duration: 200 });
  });

  it('曲の長さの秒未満の違いでは、伝え直さない', async () => {
    // 再生を始めた時点ではライブラリの値（秒単位）で、再生エンジンが読んだ値に変わる
    await report(state({ duration: 180 }));
    await report(state({ duration: 180.33 }));
    expect(sent).toHaveLength(1);

    await report(state({ duration: 195 }));
    expect(sent).toHaveLength(2);
  });

  it('再生している曲がなくなったら、nullを伝える（伝えていなければ何もしない）', async () => {
    await report(null);
    expect(sent).toEqual([]);

    await report(state());
    await report(null);
    await report(null);
    expect(sent).toEqual([expect.objectContaining({ trackId: 't1' }), null]);

    // なくなった後は、同じ曲でも伝え直す
    await report(state());
    expect(sent).toHaveLength(3);
  });

  it('前の呼び出しの結果を待ってから、最後の状態だけを伝える', async () => {
    hold = true;
    reporter.update(state({ trackId: 't1' }));
    reporter.update(state({ trackId: 't2' }));
    reporter.update(state({ trackId: 't3', playing: false }));
    // 1つ目の結果を待っている間は、次を呼ばない
    expect(sent.map((update) => update?.trackId)).toEqual(['t1']);

    resolvers.shift()?.();
    await flush();
    expect(sent.map((update) => update?.trackId)).toEqual(['t1', 't3']);

    resolvers.shift()?.();
    await flush();
    expect(sent).toHaveLength(2);
  });

  it('伝えるのに失敗しても、次の状態は伝える', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const failing = vi
      .fn<(update: NowPlayingUpdate | null) => Promise<unknown>>()
      .mockRejectedValueOnce({ code: 'NOT_FOUND', message: '見つかりません' })
      .mockResolvedValue(null);
    const reporter = createNowPlayingReporter(failing, () => clock);

    reporter.update(state({ trackId: 't1' }));
    await flush();
    reporter.update(state({ trackId: 't2' }));
    await flush();

    expect(failing).toHaveBeenCalledTimes(2);
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});
