import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConvertProgress, ConvertResult, Track } from '#lib/types/models.js';

type Listener = (event: { payload: ConvertProgress }) => void;

/** 進捗の通知を聞いている関数 */
const listeners = new Set<Listener>();

vi.mock('#lib/bindings.js', () => ({
  DEFAULT_AAC_BITRATE_KBPS: 256,
  commands: {
    convertTracks: vi.fn(),
    cancelConversion: vi.fn()
  },
  events: {
    convertProgress: {
      listen: vi.fn(async (listener: Listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      })
    }
  }
}));

import { commands } from '#lib/bindings.js';
import { DEFAULT_CONVERT_OPTIONS } from '#lib/utils/convert.js';
import { conversion, conversionSummary, convertDialog } from './convert.svelte.js';
import { notifications } from './error.svelte.js';

const RESULT: ConvertResult = {
  convertedCount: 2,
  skippedCount: 1,
  failedCount: 0,
  errors: [],
  cancelled: false
};

/** 非同期の処理（通知を聞き始める・コマンドの結果待ち）が進むのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const lastNotification = () => notifications.items.at(-1);

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  notifications.clear();
  Object.assign(conversion, { isRunning: false, isCancelling: false, progress: null });
  vi.mocked(commands.cancelConversion).mockResolvedValue(null);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('conversion', () => {
  it('変換を始め、進捗を受け取り、終わったら通知を出す', async () => {
    let finish!: (result: ConvertResult) => void;
    vi.mocked(commands.convertTracks).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const stop = conversion.watch();
    await flush();

    const running = conversion.start(['a', 'b'], DEFAULT_CONVERT_OPTIONS);
    expect(conversion.isRunning).toBe(true);
    expect(commands.convertTracks).toHaveBeenCalledWith(['a', 'b'], DEFAULT_CONVERT_OPTIONS);
    // 実行中は、もう1つ始めない
    expect(await conversion.start(['c'], DEFAULT_CONVERT_OPTIONS)).toBeNull();
    expect(commands.convertTracks).toHaveBeenCalledTimes(1);

    const progress = { current: 1, total: 2, currentFile: 'a.flac' };
    listeners.forEach((listener) => listener({ payload: progress }));
    expect(conversion.progress).toEqual(progress);

    finish(RESULT);
    expect(await running).toEqual(RESULT);
    expect(conversion.isRunning).toBe(false);
    expect(conversion.progress).toBeNull();
    expect(lastNotification()).toMatchObject({ message: conversionSummary(RESULT), type: 'info' });

    // 実行中でない時に届いた進捗は、使わない
    listeners.forEach((listener) => listener({ payload: progress }));
    expect(conversion.progress).toBeNull();
    stop();
    await flush();
    expect(listeners.size).toBe(0);
  });

  it('書き出すフォルダを選ばなかった場合は、通知を出さない', async () => {
    vi.mocked(commands.convertTracks).mockResolvedValue(null);

    expect(await conversion.start(['a'], DEFAULT_CONVERT_OPTIONS)).toBeNull();

    expect(conversion.isRunning).toBe(false);
    expect(notifications.items).toEqual([]);
  });

  it('変換できなかった曲がある・中断した場合は、警告として知らせる', async () => {
    const failed = { ...RESULT, failedCount: 1, errors: ['/music/a.wav: 読めません'] };
    vi.mocked(commands.convertTracks).mockResolvedValueOnce(failed);
    expect(await conversion.start(['a'], DEFAULT_CONVERT_OPTIONS)).toEqual(failed);
    expect(lastNotification()?.type).toBe('warning');

    const cancelled = { ...RESULT, cancelled: true };
    vi.mocked(commands.convertTracks).mockResolvedValueOnce(cancelled);
    await conversion.start(['a'], DEFAULT_CONVERT_OPTIONS);
    expect(lastNotification()).toMatchObject({
      message: conversionSummary(cancelled),
      type: 'warning'
    });
    expect(conversionSummary(cancelled)).not.toBe(conversionSummary(RESULT));
  });

  it('始められなかった場合は、エラーを知らせて、実行中でない状態に戻す', async () => {
    vi.mocked(commands.convertTracks).mockRejectedValue({
      code: 'LOCK',
      message: '形式の変換を実行中です'
    });

    expect(await conversion.start(['a'], DEFAULT_CONVERT_OPTIONS)).toBeNull();

    expect(conversion.isRunning).toBe(false);
    expect(lastNotification()?.type).toBe('error');
  });

  it('中断を頼むと、終わるまで中断中になる', async () => {
    // 実行中でなければ、何もしない
    await conversion.cancel();
    expect(commands.cancelConversion).not.toHaveBeenCalled();

    let finish!: (result: ConvertResult) => void;
    vi.mocked(commands.convertTracks).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const running = conversion.start(['a'], DEFAULT_CONVERT_OPTIONS);
    await conversion.cancel();
    await conversion.cancel();
    expect(commands.cancelConversion).toHaveBeenCalledTimes(1);
    expect(conversion.isCancelling).toBe(true);

    finish({ ...RESULT, cancelled: true });
    await running;
    expect(conversion.isCancelling).toBe(false);
  });
});

describe('convertDialog', () => {
  it('曲があれば開き、閉じると曲を捨てる', () => {
    convertDialog.open([]);
    expect(convertDialog.tracks).toBeNull();

    const tracks = [{ id: 'a' } as Track];
    convertDialog.open(tracks);
    expect(convertDialog.tracks).toBe(tracks);

    convertDialog.close();
    expect(convertDialog.tracks).toBeNull();
  });
});
