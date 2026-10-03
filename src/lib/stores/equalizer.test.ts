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

/** テスト用のWeb Audio（ノードのゲインだけを記録する） */
class FakeAudioContext {
  state = 'running';
  destination = {};
  currentTime = 10;
  filters: { gain: { value: number } }[] = [];
  gains: {
    gain: {
      value: number;
      cancelScheduledValues: ReturnType<typeof vi.fn>;
      setValueCurveAtTime: ReturnType<typeof vi.fn>;
    };
  }[] = [];

  createMediaElementSource() {
    return { connect: vi.fn(), disconnect: vi.fn() };
  }

  createBiquadFilter() {
    const filter = {
      type: '',
      frequency: { value: 0 },
      Q: { value: 0 },
      gain: { value: 0 },
      connect: vi.fn(),
      disconnect: vi.fn()
    };
    this.filters.push(filter);
    return filter;
  }

  createGain() {
    const gain = {
      gain: {
        value: 1,
        cancelScheduledValues: vi.fn(),
        setValueCurveAtTime: vi.fn()
      },
      connect: vi.fn(),
      disconnect: vi.fn()
    };
    this.gains.push(gain);
    return gain;
  }

  async close() {}
}

describe('Web Audioへの反映', () => {
  it('接続時に保存済みの設定を反映し、変更にも追随する（無効時は0dB）', async () => {
    const contexts: FakeAudioContext[] = [];
    vi.stubGlobal(
      'AudioContext',
      class extends FakeAudioContext {
        constructor() {
          super();
          contexts.push(this);
        }
      }
    );
    const { equalizer, initializeEqualizer, cleanupEqualizer, BUILTIN_PRESETS } =
      await importFreshEqualizer();
    equalizer.applyPreset('bass_boost');

    await initializeEqualizer([{} as HTMLAudioElement]);
    expect(contexts).toHaveLength(1);
    const gains = () => contexts[0].filters.map((f) => f.gain.value);
    // 無効のままなので0dB
    expect(gains().every((gain) => gain === 0)).toBe(true);

    equalizer.setEnabled(true);
    expect(gains()).toEqual(Object.values(BUILTIN_PRESETS.bass_boost));

    equalizer.setBandGain(16000, 3);
    expect(gains().at(-1)).toBe(3);

    await cleanupEqualizer();
  });
});

describe('音量の正規化', () => {
  it('デッキごとに倍率を設定でき、接続前に設定した倍率も接続時に反映する', async () => {
    const contexts: FakeAudioContext[] = [];
    vi.stubGlobal(
      'AudioContext',
      class extends FakeAudioContext {
        constructor() {
          super();
          contexts.push(this);
        }
      }
    );
    const { initializeEqualizer, cleanupEqualizer, setNormalizationGain } =
      await importFreshEqualizer();

    setNormalizationGain(1, 0.5);
    await initializeEqualizer([{} as HTMLAudioElement, {} as HTMLAudioElement]);
    // 最初に作るGainNodeが最終出力用で、続いてデッキごとに正規化用・フェード用
    const [, deck0, , deck1] = contexts[0].gains;
    expect(deck0.gain.value).toBe(1);
    expect(deck1.gain.value).toBe(0.5);

    setNormalizationGain(0, 0.25);
    expect(deck0.gain.value).toBe(0.25);
    expect(deck1.gain.value).toBe(0.5);

    await cleanupEqualizer();
  });
});

describe('クロスフェード', () => {
  it('デッキごとに等パワーの曲線でフェードさせ、取り消して音量を決められる', async () => {
    const contexts: FakeAudioContext[] = [];
    vi.stubGlobal(
      'AudioContext',
      class extends FakeAudioContext {
        constructor() {
          super();
          contexts.push(this);
        }
      }
    );
    const { initializeEqualizer, cleanupEqualizer, fadeDeck, setDeckFade } =
      await importFreshEqualizer();
    await initializeEqualizer([{} as HTMLAudioElement, {} as HTMLAudioElement]);
    const [, , fade0, , fade1] = contexts[0].gains;

    fadeDeck(0, 'out', 5);
    fadeDeck(1, 'in', 5);

    const [outCurve, outStart, outSeconds] = fade0.gain.setValueCurveAtTime.mock.calls[0];
    const [inCurve] = fade1.gain.setValueCurveAtTime.mock.calls[0];
    expect(outStart).toBe(10);
    expect(outSeconds).toBe(5);
    expect(outCurve[0]).toBeCloseTo(1);
    expect(outCurve.at(-1)).toBeCloseTo(0);
    expect(inCurve[0]).toBeCloseTo(0);
    expect(inCurve.at(-1)).toBeCloseTo(1);
    // 重ねても合計の大きさ（2乗の和）が下がらない
    for (let i = 0; i < inCurve.length; i++) {
      expect(outCurve[i] ** 2 + inCurve[i] ** 2).toBeCloseTo(1);
    }
    // 途中のフェードを取り消してから始める
    expect(fade0.gain.cancelScheduledValues).toHaveBeenCalledWith(0);

    setDeckFade(1, 1);
    expect(fade1.gain.cancelScheduledValues).toHaveBeenCalledTimes(2);
    expect(fade1.gain.value).toBe(1);

    await cleanupEqualizer();
  });
});
