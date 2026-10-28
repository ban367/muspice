/**
 * イコライザの設定
 *
 * 10バンドのグラフィックイコライザの設定（有効/無効・バンドのゲイン・プリセット）を持ち、
 * localStorageに保存する。音にかける処理は再生エンジン（`src-tauri/src/playback/effects.rs`）が行い、
 * 設定は再生コントローラー（`./playback.svelte.ts`）がここから読んでエンジンへ送る。
 */

// 10バンドの周波数定義
export const EQ_FREQUENCIES = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000] as const;
export type EQFrequency = (typeof EQ_FREQUENCIES)[number];

// 周波数表示用ラベル
export const EQ_FREQUENCY_LABELS: Record<EQFrequency, string> = {
  31: '31',
  62: '62',
  125: '125',
  250: '250',
  500: '500',
  1000: '1k',
  2000: '2k',
  4000: '4k',
  8000: '8k',
  16000: '16k'
};

// ゲインの範囲（dB）
export const MIN_GAIN = -12;
export const MAX_GAIN = 12;

// イコライザのバンド設定
export type EQBands = Record<EQFrequency, number>;

// デフォルトのバンド設定（全て0dB）
const DEFAULT_BANDS: EQBands = {
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

// ビルトインプリセット名
export type BuiltinPresetName =
  'flat' | 'bass_boost' | 'treble_boost' | 'vocal' | 'rock' | 'pop' | 'jazz' | 'classical';

// プリセット名（ビルトイン + カスタム）
export type PresetName = BuiltinPresetName | string;

/** @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う） */
export const BUILTIN_PRESETS: Record<BuiltinPresetName, EQBands> = {
  flat: { ...DEFAULT_BANDS },
  bass_boost: {
    31: 8,
    62: 6,
    125: 4,
    250: 2,
    500: 0,
    1000: 0,
    2000: 0,
    4000: 0,
    8000: 0,
    16000: 0
  },
  treble_boost: {
    31: 0,
    62: 0,
    125: 0,
    250: 0,
    500: 0,
    1000: 0,
    2000: 2,
    4000: 4,
    8000: 6,
    16000: 8
  },
  vocal: {
    31: -2,
    62: -1,
    125: 0,
    250: 2,
    500: 4,
    1000: 4,
    2000: 3,
    4000: 2,
    8000: 0,
    16000: -1
  },
  rock: {
    31: 5,
    62: 4,
    125: 2,
    250: 0,
    500: -1,
    1000: 0,
    2000: 2,
    4000: 4,
    8000: 5,
    16000: 6
  },
  pop: {
    31: -1,
    62: 0,
    125: 2,
    250: 3,
    500: 4,
    1000: 3,
    2000: 2,
    4000: 1,
    8000: 2,
    16000: 3
  },
  jazz: {
    31: 3,
    62: 2,
    125: 1,
    250: 2,
    500: -1,
    1000: -1,
    2000: 0,
    4000: 1,
    8000: 2,
    16000: 3
  },
  classical: {
    31: 4,
    62: 3,
    125: 2,
    250: 1,
    500: 0,
    1000: 0,
    2000: 0,
    4000: 1,
    8000: 2,
    16000: 3
  }
};

// カスタムプリセット
export interface CustomPreset {
  name: string;
  bands: EQBands;
}

// イコライザの設定（localStorageに保存する）
interface EqualizerSettings {
  enabled: boolean;
  bands: EQBands;
  currentPreset: PresetName | null;
  customPresets: CustomPreset[];
}

// ストレージキー
const STORAGE_KEY = 'muspice:equalizer';

// デフォルト設定
const DEFAULT_SETTINGS: EqualizerSettings = {
  enabled: false,
  bands: { ...DEFAULT_BANDS },
  currentPreset: 'flat',
  customPresets: []
};

/** 保存してあったバンドのゲイン（数でない値は0（フラット）、範囲の外の値は範囲の端にする） */
function readBands(value: unknown): EQBands {
  const stored =
    typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
  return Object.fromEntries(
    EQ_FREQUENCIES.map((frequency) => {
      const gain = stored[frequency];
      return [
        frequency,
        typeof gain === 'number' && Number.isFinite(gain)
          ? Math.max(MIN_GAIN, Math.min(MAX_GAIN, gain))
          : DEFAULT_BANDS[frequency]
      ];
    })
  ) as EQBands;
}

/** 保存してあったカスタムプリセット（名前のないものは除く。ゲインは`readBands`で補う） */
function readCustomPresets(value: unknown): CustomPreset[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((preset: unknown) => {
    const { name, bands } = (typeof preset === 'object' && preset !== null ? preset : {}) as {
      name?: unknown;
      bands?: unknown;
    };
    return typeof name === 'string' && name.trim() !== ''
      ? [{ name, bands: readBands(bands) }]
      : [];
  });
}

/**
 * localStorageから設定を読み込む
 *
 * 値ごとに形を確かめ、読めない値は既定値で補う（壊れた値を、そのまま再生エンジンへ送らない）。
 * 選んでいたプリセットが、ビルトインにもカスタムにもなければ、選択なし（カスタムの状態）にする。
 */
function loadSettings(): EqualizerSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed: unknown = JSON.parse(stored);
      const settings = (typeof parsed === 'object' && parsed !== null ? parsed : {}) as Record<
        string,
        unknown
      >;
      const customPresets = readCustomPresets(settings.customPresets);
      const preset = settings.currentPreset;
      const isKnownPreset =
        typeof preset === 'string' &&
        (isBuiltinPreset(preset) || customPresets.some((custom) => custom.name === preset));
      return {
        enabled:
          typeof settings.enabled === 'boolean' ? settings.enabled : DEFAULT_SETTINGS.enabled,
        bands: 'bands' in settings ? readBands(settings.bands) : { ...DEFAULT_BANDS },
        currentPreset:
          preset === undefined ? DEFAULT_SETTINGS.currentPreset : isKnownPreset ? preset : null,
        customPresets
      };
    }
  } catch (error) {
    // localStorageが使えない・パースエラー時はデフォルト値を使用
    console.warn('Failed to load equalizer settings:', error);
  }
  return { ...DEFAULT_SETTINGS };
}

// localStorageに設定を保存
function saveSettings(settings: EqualizerSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (error) {
    // 保存エラーをログに記録（プライベートブラウジングモードや容量制限など）
    console.warn('Failed to save equalizer settings:', error);
  }
}

// ビルトインプリセットかどうかをチェック
export function isBuiltinPreset(name: string): name is BuiltinPresetName {
  return name in BUILTIN_PRESETS;
}

/**
 * イコライザの設定
 *
 * 読み取りはプロパティ（`equalizer.enabled`など）、変更はメソッドで行う。
 * 変更はlocalStorageに保存する（再生エンジンへは、再生コントローラーが変更を検知して送る）。
 */
class Equalizer {
  #settings = $state.raw<EqualizerSettings>(loadSettings());

  /** イコライザの有効/無効 */
  get enabled(): boolean {
    return this.#settings.enabled;
  }

  /** 各バンドのゲイン（dB） */
  get bands(): EQBands {
    return this.#settings.bands;
  }

  /** 選択中のプリセット（スライダーを動かした後はnull = カスタム） */
  get currentPreset(): PresetName | null {
    return this.#settings.currentPreset;
  }

  /** 保存済みのカスタムプリセット */
  get customPresets(): CustomPreset[] {
    return this.#settings.customPresets;
  }

  // イコライザのON/OFF切り替え
  toggle(): void {
    this.setEnabled(!this.enabled);
  }

  // イコライザの有効/無効を設定
  setEnabled(enabled: boolean): void {
    this.#update({ enabled });
  }

  // 特定のバンドのゲインを設定
  setBandGain(frequency: EQFrequency, gain: number): void {
    // ゲインを範囲内に制限
    const clampedGain = Math.max(MIN_GAIN, Math.min(MAX_GAIN, gain));
    this.#update({ bands: { ...this.bands, [frequency]: clampedGain }, currentPreset: null });
  }

  // プリセットを適用（ビルトインまたはカスタム）
  applyPreset(presetName: PresetName): void {
    const preset = isBuiltinPreset(presetName)
      ? BUILTIN_PRESETS[presetName]
      : this.customPresets.find((p) => p.name === presetName)?.bands;
    if (!preset) return;

    this.#update({ bands: { ...preset }, currentPreset: presetName });
  }

  // カスタムプリセットを保存（同じ名前があれば上書き）
  saveCustomPreset(name: string): void {
    const newPreset: CustomPreset = { name, bands: { ...this.bands } };
    const existingIndex = this.customPresets.findIndex((p) => p.name === name);
    const customPresets =
      existingIndex >= 0
        ? this.customPresets.map((p, i) => (i === existingIndex ? newPreset : p))
        : [...this.customPresets, newPreset];

    this.#update({ customPresets, currentPreset: name });
  }

  // カスタムプリセットを削除
  deleteCustomPreset(name: string): void {
    this.#update({
      customPresets: this.customPresets.filter((p) => p.name !== name),
      // 削除したプリセットが選択されていた場合はnullに
      currentPreset: this.currentPreset === name ? null : this.currentPreset
    });
  }

  // リセット（フラットに戻す）
  reset(): void {
    this.#update({ bands: { ...DEFAULT_BANDS }, currentPreset: 'flat' });
  }

  #update(changes: Partial<EqualizerSettings>): void {
    this.#settings = { ...this.#settings, ...changes };
    saveSettings(this.#settings);
  }
}

// イコライザのエクスポート
export const equalizer = new Equalizer();
