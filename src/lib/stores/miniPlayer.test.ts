import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { commands } from '#lib/bindings.js';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    setMiniPlayer: vi.fn(async () => null)
  }
}));

/** Node環境にはlocalStorageがないため、メモリ上の実装に差し替える */
function createMemoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => {
      items.delete(key);
    },
    setItem: (key, value) => {
      items.set(key, String(value));
    }
  };
}

/** 保存した値の読み込みを確かめるため、モジュールを読み込み直す */
async function importFresh() {
  vi.resetModules();
  return (await import('./miniPlayer.svelte.js')).miniPlayer;
}

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
  vi.clearAllMocks();
  vi.mocked(commands.setMiniPlayer).mockResolvedValue(null);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('miniPlayer', () => {
  it('最初は通常の表示で、ミニプレーヤーは手前に表示する設定', async () => {
    const miniPlayer = await importFresh();

    expect(miniPlayer.isActive).toBe(false);
    expect(miniPlayer.alwaysOnTop).toBe(true);
  });

  it('切り替えると、ウィンドウの大きさを変えてから表示を切り替える', async () => {
    const miniPlayer = await importFresh();

    await miniPlayer.toggle();
    expect(commands.setMiniPlayer).toHaveBeenLastCalledWith(true, true);
    expect(miniPlayer.isActive).toBe(true);

    await miniPlayer.toggle();
    expect(commands.setMiniPlayer).toHaveBeenLastCalledWith(false, true);
    expect(miniPlayer.isActive).toBe(false);

    // 通常の表示の間の「戻す」は、何もしない
    await miniPlayer.exit();
    expect(commands.setMiniPlayer).toHaveBeenCalledTimes(2);
  });

  it('ウィンドウを切り替えられなかった場合は、表示を変えない', async () => {
    const miniPlayer = await importFresh();
    vi.mocked(commands.setMiniPlayer).mockRejectedValueOnce({ code: 'IO', message: 'だめ' });

    await miniPlayer.toggle();

    expect(miniPlayer.isActive).toBe(false);
    // 続けて切り替えられる
    await miniPlayer.toggle();
    expect(miniPlayer.isActive).toBe(true);
  });

  it('切り替えを待っている間の操作は、重ねて行わない', async () => {
    const miniPlayer = await importFresh();
    let resolve: (value: null) => void = () => {};
    vi.mocked(commands.setMiniPlayer).mockReturnValueOnce(new Promise((done) => (resolve = done)));

    const first = miniPlayer.toggle();
    await miniPlayer.toggle();
    resolve(null);
    await first;

    expect(commands.setMiniPlayer).toHaveBeenCalledTimes(1);
    expect(miniPlayer.isActive).toBe(true);
  });

  it('「常に手前に表示」は保存し、ミニプレーヤーの間はすぐに反映する', async () => {
    const first = await importFresh();
    await first.setAlwaysOnTop(false);
    // 通常の表示の間は、ウィンドウには触れない
    expect(commands.setMiniPlayer).not.toHaveBeenCalled();
    expect(first.alwaysOnTop).toBe(false);

    const second = await importFresh();
    expect(second.alwaysOnTop).toBe(false);
    await second.toggle();
    expect(commands.setMiniPlayer).toHaveBeenLastCalledWith(true, false);

    await second.setAlwaysOnTop(true);
    expect(commands.setMiniPlayer).toHaveBeenLastCalledWith(true, true);
    expect(second.isActive).toBe(true);
  });
});
