import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EQBands } from './equalizer.svelte.js';

const STORAGE_KEY = 'muspice:equalizer';

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

/** localStorageを差し替えた後で読み込み直す（保存値からの初期化はモジュールの読み込み時に行うため） */
async function importFreshEqualizer() {
  vi.resetModules();
  return import('./equalizer.svelte.js');
}

/** 保存されている設定 */
function storedSettings() {
  return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
}

const FLAT: EQBands = {
  31: 0,
  62: 0,
  125: 0,
  250: 0,
  500: 0,
  1000: 0,
  2000: 0,
  4000: 0,
  8000: 0,
  16000: 0
};

beforeEach(() => {
  vi.stubGlobal('localStorage', createMemoryStorage());
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('初期化', () => {
  it('保存がなければ無効・Flatで始める', async () => {
    const { equalizer } = await importFreshEqualizer();
    expect(equalizer.enabled).toBe(false);
    expect(equalizer.currentPreset).toBe('flat');
    expect(equalizer.bands).toEqual(FLAT);
    expect(equalizer.customPresets).toEqual([]);
  });

  it('保存した設定で初期化する', async () => {
    const bands = { ...FLAT, 31: 6 };
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        enabled: true,
        bands,
        currentPreset: 'mine',
        customPresets: [{ name: 'mine', bands }]
      })
    );

    const { equalizer } = await importFreshEqualizer();
    expect(equalizer.enabled).toBe(true);
    expect(equalizer.bands).toEqual(bands);
    expect(equalizer.currentPreset).toBe('mine');
    expect(equalizer.customPresets).toEqual([{ name: 'mine', bands }]);
  });

  it('壊れた設定が保存されていても既定値で始める', async () => {
    localStorage.setItem(STORAGE_KEY, '{');
    const { equalizer } = await importFreshEqualizer();
    expect(equalizer.enabled).toBe(false);
    expect(equalizer.bands).toEqual(FLAT);
  });
});

describe('バンド・プリセットの操作', () => {
  it('バンドのゲインは範囲内に制限し、プリセットの選択を外す（カスタム）', async () => {
    const { equalizer, MAX_GAIN, MIN_GAIN } = await importFreshEqualizer();

    equalizer.setBandGain(62, 30);
    equalizer.setBandGain(8000, -30);
    expect(equalizer.bands[62]).toBe(MAX_GAIN);
    expect(equalizer.bands[8000]).toBe(MIN_GAIN);
    expect(equalizer.currentPreset).toBeNull();
    expect(storedSettings().bands[62]).toBe(MAX_GAIN);
  });

  it('ビルトインプリセットを適用する（プリセット自体は書き換えない）', async () => {
    const { equalizer, BUILTIN_PRESETS } = await importFreshEqualizer();

    equalizer.applyPreset('rock');
    expect(equalizer.currentPreset).toBe('rock');
    expect(equalizer.bands).toEqual(BUILTIN_PRESETS.rock);

    equalizer.setBandGain(31, -5);
    expect(BUILTIN_PRESETS.rock[31]).toBe(5);
  });

  it('存在しないプリセットは無視する', async () => {
    const { equalizer } = await importFreshEqualizer();
    equalizer.setBandGain(31, 3);

    equalizer.applyPreset('unknown');
    expect(equalizer.bands[31]).toBe(3);
    expect(equalizer.currentPreset).toBeNull();
  });

  it('カスタムプリセットを保存して選択し、後から適用できる', async () => {
    const { equalizer } = await importFreshEqualizer();
    equalizer.setBandGain(125, 4);

    equalizer.saveCustomPreset('お気に入り');
    expect(equalizer.currentPreset).toBe('お気に入り');
    expect(equalizer.customPresets).toEqual([{ name: 'お気に入り', bands: { ...FLAT, 125: 4 } }]);

    equalizer.reset();
    expect(equalizer.bands).toEqual(FLAT);
    expect(equalizer.currentPreset).toBe('flat');

    equalizer.applyPreset('お気に入り');
    expect(equalizer.bands[125]).toBe(4);
    expect(equalizer.currentPreset).toBe('お気に入り');
  });

  it('同じ名前で保存すると上書きする', async () => {
    const { equalizer } = await importFreshEqualizer();
    equalizer.setBandGain(125, 4);
    equalizer.saveCustomPreset('A');
    equalizer.setBandGain(125, -2);
    equalizer.saveCustomPreset('A');

    expect(equalizer.customPresets).toEqual([{ name: 'A', bands: { ...FLAT, 125: -2 } }]);
  });

  it('保存したプリセットは、後からバンドを動かしても変わらない', async () => {
    const { equalizer } = await importFreshEqualizer();
    equalizer.setBandGain(125, 4);
    equalizer.saveCustomPreset('A');

    equalizer.setBandGain(125, 8);
    expect(equalizer.customPresets[0].bands[125]).toBe(4);
  });

  it('選択中のカスタムプリセットを削除すると、選択を外す', async () => {
    const { equalizer } = await importFreshEqualizer();
    equalizer.saveCustomPreset('A');
    equalizer.saveCustomPreset('B');

    // 選択中でないプリセットの削除は選択を変えない
    equalizer.deleteCustomPreset('A');
    expect(equalizer.customPresets.map((p) => p.name)).toEqual(['B']);
    expect(equalizer.currentPreset).toBe('B');

    equalizer.deleteCustomPreset('B');
    expect(equalizer.customPresets).toEqual([]);
    expect(equalizer.currentPreset).toBeNull();
  });

  it('変更を保存し、次回の起動時に復元する', async () => {
    const first = await importFreshEqualizer();
    first.equalizer.toggle();
    first.equalizer.applyPreset('jazz');
    first.equalizer.saveCustomPreset('夜');

    const { equalizer, BUILTIN_PRESETS } = await importFreshEqualizer();
    expect(equalizer.enabled).toBe(true);
    expect(equalizer.bands).toEqual(BUILTIN_PRESETS.jazz);
    expect(equalizer.currentPreset).toBe('夜');
    expect(equalizer.customPresets.map((p) => p.name)).toEqual(['夜']);
  });
});
