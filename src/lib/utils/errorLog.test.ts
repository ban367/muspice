import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    logFrontendMessage: vi.fn(async () => null)
  }
}));

import { commands } from '#lib/bindings.js';
import {
  describeError,
  logToFile,
  reportError,
  resetErrorLogForTest,
  watchUnhandledErrors
} from './errorLog.js';

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetErrorLogForTest();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('describeError', () => {
  it('Errorは、種類・メッセージ・スタックを入れる', () => {
    const error = new TypeError('x is undefined');
    error.stack = 'f@app.js:1:2\ng@app.js:3:4';

    expect(describeError(error)).toBe('TypeError: x is undefined\nf@app.js:1:2\ng@app.js:3:4');
  });

  it('スタックがメッセージから始まる場合は、繰り返さない', () => {
    const error = new Error('boom');
    error.stack = 'Error: boom\n    at f (app.js:1:2)';

    expect(describeError(error)).toBe('Error: boom\n    at f (app.js:1:2)');
  });

  it('AppErrorは、コードとメッセージにする', () => {
    expect(describeError({ code: 'DATABASE', message: 'SQLITE_BUSY' })).toBe(
      '[DATABASE] SQLITE_BUSY'
    );
  });

  it('文字列はそのまま、ほかの値は文字にする', () => {
    expect(describeError('failed')).toBe('failed');
    expect(describeError({ a: 1 })).toBe('{"a":1}');
    expect(describeError(undefined)).toBe('undefined');
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(describeError(circular)).toBe('[object Object]');
  });
});

describe('logToFile', () => {
  it('文を、重さと一緒にRust側へ送る', () => {
    reportError(new Error('boom'), 'Loading tracks');

    expect(commands.logFrontendMessage).toHaveBeenCalledTimes(1);
    const [level, message] = vi.mocked(commands.logFrontendMessage).mock.calls[0];
    expect(level).toBe('error');
    expect(message).toMatch(/^Loading tracks: Error: boom/);
  });

  it('直前と同じ文は、しばらく送らない', () => {
    logToFile('error', 'same');
    logToFile('error', 'same');
    expect(commands.logFrontendMessage).toHaveBeenCalledTimes(1);

    logToFile('error', 'other');
    expect(commands.logFrontendMessage).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(10_000);
    logToFile('error', 'other');
    expect(commands.logFrontendMessage).toHaveBeenCalledTimes(3);
  });

  it('送れなくても、例外にしない', async () => {
    vi.mocked(commands.logFrontendMessage).mockRejectedValueOnce(new Error('no ipc'));
    expect(() => logToFile('warn', 'a')).not.toThrow();

    vi.mocked(commands.logFrontendMessage).mockImplementationOnce(() => {
      throw new Error('window is not defined');
    });
    expect(() => logToFile('warn', 'b')).not.toThrow();
    await Promise.resolve();
  });
});

describe('watchUnhandledErrors', () => {
  it('受け止められなかったエラーとPromiseの失敗を、ログに残す', () => {
    const listeners = new Map<string, (event: unknown) => void>();
    const target = {
      addEventListener: (type: string, listener: (event: unknown) => void) => {
        listeners.set(type, listener);
      }
    } as unknown as Pick<Window, 'addEventListener'>;

    watchUnhandledErrors(target);
    listeners.get('error')?.({
      error: new RangeError('bad index'),
      message: 'RangeError: bad index',
      filename: 'app.js',
      lineno: 10,
      colno: 5
    });
    listeners.get('unhandledrejection')?.({ reason: { code: 'IO', message: 'disk' } });

    const messages = vi.mocked(commands.logFrontendMessage).mock.calls.map((call) => call[1]);
    expect(messages[0]).toMatch(/^Uncaught error \(app\.js:10:5\): RangeError: bad index/);
    expect(messages[1]).toBe('Unhandled promise rejection: [IO] disk');
  });
});
