import { writable, get } from 'svelte/store';

/**
 * イコライザの状態管理ストア
 * Web Audio APIを使用して10バンドグラフィックイコライザを実装
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
export const DEFAULT_GAIN = 0;

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

// プリセット定義
export type PresetName =
  | 'flat'
  | 'bass_boost'
  | 'treble_boost'
  | 'vocal'
  | 'rock'
  | 'pop'
  | 'jazz'
  | 'classical';

export const PRESET_LABELS: Record<PresetName, string> = {
  flat: 'Flat',
  bass_boost: 'Bass Boost',
  treble_boost: 'Treble Boost',
  vocal: 'Vocal',
  rock: 'Rock',
  pop: 'Pop',
  jazz: 'Jazz',
  classical: 'Classical'
};

export const PRESETS: Record<PresetName, EQBands> = {
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

// イコライザの状態
interface EqualizerState {
  enabled: boolean;
  bands: EQBands;
  currentPreset: PresetName | null;
}

// ストレージキー
const STORAGE_KEY = 'muspice:equalizer';

// デフォルト状態
const DEFAULT_STATE: EqualizerState = {
  enabled: false,
  bands: { ...DEFAULT_BANDS },
  currentPreset: 'flat'
};

// localStorageから状態を読み込み
function loadState(): EqualizerState {
  if (typeof window === 'undefined') return { ...DEFAULT_STATE };

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as Partial<EqualizerState>;
      return {
        enabled: parsed.enabled ?? DEFAULT_STATE.enabled,
        bands: parsed.bands ?? { ...DEFAULT_BANDS },
        currentPreset: parsed.currentPreset ?? DEFAULT_STATE.currentPreset
      };
    }
  } catch {
    // パースエラー時はデフォルト値を使用
  }
  return { ...DEFAULT_STATE };
}

// localStorageに状態を保存
function saveState(state: EqualizerState): void {
  if (typeof window === 'undefined') return;

  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // 保存エラーは無視
  }
}

// イコライザストアを作成
function createEqualizerStore() {
  const initialState = loadState();
  const { subscribe, update } = writable<EqualizerState>(initialState);

  return {
    subscribe,

    // イコライザのON/OFF切り替え
    toggle: () => {
      update((state) => {
        const newState = { ...state, enabled: !state.enabled };
        saveState(newState);
        applyEqualizerSettings(newState);
        return newState;
      });
    },

    // イコライザの有効/無効を設定
    setEnabled: (enabled: boolean) => {
      update((state) => {
        const newState = { ...state, enabled };
        saveState(newState);
        applyEqualizerSettings(newState);
        return newState;
      });
    },

    // 特定のバンドのゲインを設定
    setBandGain: (frequency: EQFrequency, gain: number) => {
      update((state) => {
        // ゲインを範囲内に制限
        const clampedGain = Math.max(MIN_GAIN, Math.min(MAX_GAIN, gain));
        const newBands = { ...state.bands, [frequency]: clampedGain };
        const newState = { ...state, bands: newBands, currentPreset: null };
        saveState(newState);
        applyEqualizerSettings(newState);
        return newState;
      });
    },

    // プリセットを適用
    applyPreset: (presetName: PresetName) => {
      update((state) => {
        const preset = PRESETS[presetName];
        const newState = {
          ...state,
          bands: { ...preset },
          currentPreset: presetName
        };
        saveState(newState);
        applyEqualizerSettings(newState);
        return newState;
      });
    },

    // リセット（フラットに戻す）
    reset: () => {
      update((state) => {
        const newState = {
          ...state,
          bands: { ...DEFAULT_BANDS },
          currentPreset: 'flat' as PresetName
        };
        saveState(newState);
        applyEqualizerSettings(newState);
        return newState;
      });
    }
  };
}

// イコライザストアのエクスポート
export const equalizer = createEqualizerStore();

// Web Audio API関連
let audioContext: AudioContext | null = null;
let sourceNode: MediaElementAudioSourceNode | null = null;
let filterNodes: BiquadFilterNode[] = [];
let gainNode: GainNode | null = null;
let isInitialized = false;

/**
 * イコライザを初期化（Audio要素と接続）
 */
export async function initializeEqualizer(audioElement: HTMLAudioElement): Promise<void> {
  if (isInitialized) return;

  try {
    // AudioContextを作成
    audioContext = new AudioContext();

    // Audio要素をソースノードとして接続
    sourceNode = audioContext.createMediaElementSource(audioElement);

    // 各周波数のBiquadFilterを作成
    filterNodes = EQ_FREQUENCIES.map((freq) => {
      const filter = audioContext!.createBiquadFilter();
      filter.type = 'peaking';
      filter.frequency.value = freq;
      filter.Q.value = 1.4; // バンド幅を調整
      filter.gain.value = 0;
      return filter;
    });

    // ゲインノードを作成（最終出力用）
    gainNode = audioContext.createGain();
    gainNode.gain.value = 1.0;

    // ノードを直列接続: source -> filter1 -> filter2 -> ... -> filter10 -> gain -> destination
    sourceNode.connect(filterNodes[0]);
    for (let i = 0; i < filterNodes.length - 1; i++) {
      filterNodes[i].connect(filterNodes[i + 1]);
    }
    filterNodes[filterNodes.length - 1].connect(gainNode);
    gainNode.connect(audioContext.destination);

    isInitialized = true;

    // 保存されている設定を適用
    const state = get(equalizer);
    applyEqualizerSettings(state);
  } catch (error) {
    console.error('イコライザの初期化に失敗しました:', error);
  }
}

/**
 * イコライザ設定を適用
 */
function applyEqualizerSettings(state: EqualizerState): void {
  if (!isInitialized || filterNodes.length === 0) return;

  EQ_FREQUENCIES.forEach((freq, index) => {
    if (filterNodes[index]) {
      // イコライザが無効の場合はゲインを0に設定
      filterNodes[index].gain.value = state.enabled ? state.bands[freq] : 0;
    }
  });
}

/**
 * イコライザをクリーンアップ
 */
export function cleanupEqualizer(): void {
  if (sourceNode) {
    sourceNode.disconnect();
    sourceNode = null;
  }

  filterNodes.forEach((filter) => filter.disconnect());
  filterNodes = [];

  if (gainNode) {
    gainNode.disconnect();
    gainNode = null;
  }

  if (audioContext) {
    audioContext.close();
    audioContext = null;
  }

  isInitialized = false;
}

/**
 * AudioContextがサスペンド状態の場合に再開
 * (ユーザーインタラクション後に呼び出す必要がある)
 */
export async function resumeAudioContext(): Promise<void> {
  if (audioContext && audioContext.state === 'suspended') {
    await audioContext.resume();
  }
}

/**
 * イコライザが初期化済みかどうか
 */
export function isEqualizerInitialized(): boolean {
  return isInitialized;
}
