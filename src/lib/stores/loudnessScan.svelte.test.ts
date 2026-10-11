import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  LoudnessScanProgress,
  LoudnessScanResult,
  LoudnessScanStatus,
  Track
} from '#lib/types/models.js';

type Listener<T> = (event: { payload: T }) => void;

/** 進捗・完了の通知を聞いている関数 */
const progressListeners = new Set<Listener<LoudnessScanProgress>>();
const finishedListeners = new Set<Listener<LoudnessScanResult>>();

vi.mock('#lib/bindings.js', () => ({
  commands: {
    getLoudnessScanStatus: vi.fn(),
    startLoudnessScan: vi.fn(),
    cancelLoudnessScan: vi.fn()
  },
  events: {
    loudnessScanProgress: {
      listen: vi.fn(async (listener: Listener<LoudnessScanProgress>) => {
        progressListeners.add(listener);
        return () => progressListeners.delete(listener);
      })
    },
    loudnessScanFinished: {
      listen: vi.fn(async (listener: Listener<LoudnessScanResult>) => {
        finishedListeners.add(listener);
        return () => finishedListeners.delete(listener);
      })
    }
  }
}));

import { commands } from '#lib/bindings.js';
import { notifications } from './error.svelte.js';
import { loudnessScan, loudnessScanDialog, loudnessScanSummary } from './loudnessScan.svelte.js';

const RESULT: LoudnessScanResult = {
  writtenCount: 3,
  unchangedCount: 1,
  errorCount: 0,
  errors: [],
  cancelled: false
};

const status = (overrides: Partial<LoudnessScanStatus> = {}): LoudnessScanStatus => ({
  running: false,
  current: 0,
  total: 0,
  pendingCount: 5,
  ...overrides
});

/** 非同期の処理（通知を聞き始める・コマンドの結果待ち）が進むのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const lastNotification = () => notifications.items.at(-1);
const lastMessage = () => lastNotification()?.message;

beforeEach(() => {
  vi.clearAllMocks();
  progressListeners.clear();
  finishedListeners.clear();
  notifications.clear();
  Object.assign(loudnessScan, {
    isRunning: false,
    isCancelling: false,
    progress: null,
    lastResult: null,
    pendingCount: null
  });
  vi.mocked(commands.getLoudnessScanStatus).mockResolvedValue(status());
  vi.mocked(commands.cancelLoudnessScan).mockResolvedValue(null);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('loudnessScan', () => {
  it('解析を始め、終わったら結果を持ち、通知を出す', async () => {
    let finish!: (result: LoudnessScanResult) => void;
    vi.mocked(commands.startLoudnessScan).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );

    const running = loudnessScan.start(null);
    expect(loudnessScan.isRunning).toBe(true);
    expect(commands.startLoudnessScan).toHaveBeenCalledWith(null);
    // 実行中は、もう1つ始めない
    expect(await loudnessScan.start(null)).toBeNull();
    expect(commands.startLoudnessScan).toHaveBeenCalledTimes(1);

    finish(RESULT);
    expect(await running).toEqual(RESULT);
    expect(loudnessScan.isRunning).toBe(false);
    expect(loudnessScan.lastResult).toEqual(RESULT);
    expect(lastMessage()).toBe(loudnessScanSummary(RESULT));
    expect(lastNotification()?.type).toBe('info');
  });

  it('書き込めなかった曲がある・中断した場合は、警告として知らせる', async () => {
    vi.mocked(commands.startLoudnessScan).mockResolvedValueOnce({
      ...RESULT,
      errorCount: 1,
      errors: ['/music/a.mp3: 読めません']
    });
    await loudnessScan.start(null);
    expect(lastNotification()?.type).toBe('warning');

    const cancelled = { ...RESULT, cancelled: true };
    vi.mocked(commands.startLoudnessScan).mockResolvedValueOnce(cancelled);
    await loudnessScan.start(null);
    expect(lastNotification()?.type).toBe('warning');
    expect(lastMessage()).toBe(loudnessScanSummary(cancelled));
    expect(loudnessScanSummary(cancelled)).not.toBe(loudnessScanSummary(RESULT));
  });

  it('始められなかった場合は、今の状態に合わせる', async () => {
    // ほかのウィンドウで実行中
    vi.mocked(commands.startLoudnessScan).mockRejectedValue({
      code: 'LOCK',
      message: '音量の解析を実行中です'
    });
    vi.mocked(commands.getLoudnessScanStatus).mockResolvedValue(
      status({ running: true, current: 4, total: 10 })
    );

    expect(await loudnessScan.start(null)).toBeNull();

    expect(loudnessScan.isRunning).toBe(true);
    expect(loudnessScan.progress).toEqual({ current: 4, total: 10, currentFile: '' });
    expect(lastNotification()?.type).toBe('error');
  });

  it('選んだ曲の解析では、ファイルが見つからない曲を除く', async () => {
    vi.mocked(commands.startLoudnessScan).mockResolvedValue(RESULT);
    const track = (id: string, isMissing: boolean) => ({ id, isMissing }) as Track;

    await loudnessScan.startForTracks([track('a', false), track('b', true), track('c', false)]);
    expect(commands.startLoudnessScan).toHaveBeenCalledWith(['a', 'c']);

    // 対象がなければ、始めない
    vi.mocked(commands.startLoudnessScan).mockClear();
    expect(await loudnessScan.startForTracks([track('b', true)])).toBeNull();
    expect(commands.startLoudnessScan).not.toHaveBeenCalled();
  });

  it('進捗と完了の通知で、ほかのウィンドウで始めた解析の状態も分かる', async () => {
    const stop = loudnessScan.watch();
    await flush();
    expect(loudnessScan.pendingCount).toBe(5);

    const progress = { current: 2, total: 8, currentFile: 'a.flac' };
    progressListeners.forEach((listener) => listener({ payload: progress }));
    expect(loudnessScan.isRunning).toBe(true);
    expect(loudnessScan.progress).toEqual(progress);

    vi.mocked(commands.getLoudnessScanStatus).mockResolvedValue(status({ pendingCount: 0 }));
    finishedListeners.forEach((listener) => listener({ payload: RESULT }));
    await flush();
    expect(loudnessScan.isRunning).toBe(false);
    expect(loudnessScan.progress).toBeNull();
    expect(loudnessScan.lastResult).toEqual(RESULT);
    // 終わったら、音量の情報がない曲の数を取り直す
    expect(loudnessScan.pendingCount).toBe(0);
    // 始めたウィンドウではないため、通知は出さない
    expect(notifications.items).toEqual([]);

    stop();
    await flush();
    expect(progressListeners.size).toBe(0);
    expect(finishedListeners.size).toBe(0);
  });

  it('画面を開いた時に実行中なら、進み具合を出す', async () => {
    vi.mocked(commands.getLoudnessScanStatus).mockResolvedValue(
      status({ running: true, current: 3, total: 9 })
    );

    await loudnessScan.refresh();

    expect(loudnessScan.isRunning).toBe(true);
    expect(loudnessScan.progress).toEqual({ current: 3, total: 9, currentFile: '' });
  });

  it('中断を頼むと、止まるまで中断中になる', async () => {
    // 実行中でなければ、何もしない
    await loudnessScan.cancel();
    expect(commands.cancelLoudnessScan).not.toHaveBeenCalled();

    loudnessScan.isRunning = true;
    await loudnessScan.cancel();
    await loudnessScan.cancel();
    expect(commands.cancelLoudnessScan).toHaveBeenCalledTimes(1);
    expect(loudnessScan.isCancelling).toBe(true);

    finishedListeners.clear();
    const stop = loudnessScan.watch();
    await flush();
    finishedListeners.forEach((listener) => listener({ payload: { ...RESULT, cancelled: true } }));
    expect(loudnessScan.isCancelling).toBe(false);
    stop();
  });
});

describe('loudnessScanDialog', () => {
  it('曲があれば開き、閉じると曲を捨てる', () => {
    loudnessScanDialog.open([]);
    expect(loudnessScanDialog.tracks).toBeNull();

    const tracks = [{ id: 'a' } as Track];
    loudnessScanDialog.open(tracks);
    expect(loudnessScanDialog.tracks).toBe(tracks);

    loudnessScanDialog.close();
    expect(loudnessScanDialog.tracks).toBeNull();
  });
});
