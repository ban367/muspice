import { describe, expect, it, vi } from 'vitest';
import { stopListening } from './listen.js';

describe('stopListening', () => {
  it('購読を解除する', async () => {
    const stop = vi.fn();
    stopListening(Promise.resolve(stop), Promise.resolve(stop));
    await Promise.resolve();
    expect(stop).toHaveBeenCalledTimes(2);
  });

  it('購読に失敗した場合のために、拒否の処理も付ける（未処理の拒否にしない）', async () => {
    const listening = { then: vi.fn() } as unknown as Promise<() => void>;
    stopListening(listening);
    expect(listening.then).toHaveBeenCalledWith(expect.any(Function), expect.any(Function));

    // 実際に拒否されたPromiseでも、例外は外へ出ない
    await expect(
      (async () => stopListening(Promise.reject(new Error('購読できない'))))()
    ).resolves.toBeUndefined();
  });
});
