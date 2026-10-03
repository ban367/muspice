/**
 * イコライザ
 * Web Audio APIを使用して10バンドグラフィックイコライザを実装
 *
 * 設定（`equalizer`）はRunesの状態で、Web Audioのノードの管理（`initializeEqualizer`など）は
 * 再生コントローラー（`./playback`）から呼ばれるモジュール関数として分けている。
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

export const BUILTIN_PRESET_LABELS: Record<BuiltinPresetName, string> = {
  flat: 'Flat',
  bass_boost: 'Bass Boost',
  treble_boost: 'Treble Boost',
  vocal: 'Vocal',
  rock: 'Rock',
  pop: 'Pop',
  jazz: 'Jazz',
  classical: 'Classical'
};

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

// localStorageから設定を読み込み
function loadSettings(): EqualizerSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as Partial<EqualizerSettings>;
      return {
        enabled: parsed.enabled ?? DEFAULT_SETTINGS.enabled,
        bands: parsed.bands ?? { ...DEFAULT_BANDS },
        currentPreset: parsed.currentPreset ?? DEFAULT_SETTINGS.currentPreset,
        customPresets: parsed.customPresets ?? []
      };
    }
  } catch (error) {
    // localStorageが使えない・パースエラー時はデフォルト値を使用
    console.warn('イコライザ設定の読み込みに失敗しました:', error);
  }
  return { ...DEFAULT_SETTINGS };
}

// localStorageに設定を保存
function saveSettings(settings: EqualizerSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (error) {
    // 保存エラーをログに記録（プライベートブラウジングモードや容量制限など）
    console.warn('イコライザ設定の保存に失敗しました:', error);
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
 * 変更はlocalStorageに保存し、Web Audioのノードが接続済みならゲインに反映する。
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

    // ゲインは変わらないためノードへの反映は不要
    this.#update({ customPresets, currentPreset: name }, { apply: false });
  }

  // カスタムプリセットを削除
  deleteCustomPreset(name: string): void {
    this.#update(
      {
        customPresets: this.customPresets.filter((p) => p.name !== name),
        // 削除したプリセットが選択されていた場合はnullに
        currentPreset: this.currentPreset === name ? null : this.currentPreset
      },
      { apply: false }
    );
  }

  // リセット（フラットに戻す）
  reset(): void {
    this.#update({ bands: { ...DEFAULT_BANDS }, currentPreset: 'flat' });
  }

  #update(changes: Partial<EqualizerSettings>, { apply = true } = {}): void {
    this.#settings = { ...this.#settings, ...changes };
    saveSettings(this.#settings);
    if (apply) {
      applyEqualizerSettings(this);
    }
  }
}

// イコライザのエクスポート
export const equalizer = new Equalizer();

// Web Audio API関連
// 音声の経路: audio要素（デッキ） -> 音量の正規化 -> フェード -> イコライザ（10バンド） -> 出力 -> スピーカー
// ギャップレス再生のためaudio要素（デッキ）は複数あり、正規化・フェードはデッキごと、イコライザ以降は共通
let audioContext: AudioContext | null = null;
let sourceNodes: MediaElementAudioSourceNode[] = [];
let normalizationNodes: GainNode[] = [];
let fadeNodes: GainNode[] = [];
let filterNodes: BiquadFilterNode[] = [];
let gainNode: GainNode | null = null;
let isInitialized = false;
/** デッキごとの音量の正規化の倍率（ノードの接続前に設定された値も、接続時に反映する） */
const normalizationGainValues: number[] = [];

/**
 * イコライザを初期化（Audio要素と接続）
 *
 * @param audioElements 再生に使うaudio要素（デッキ）。並び順がデッキの番号になる
 */
export async function initializeEqualizer(
  audioElements: readonly HTMLAudioElement[]
): Promise<void> {
  if (isInitialized) return;

  try {
    // AudioContextを作成
    const context = new AudioContext();
    audioContext = context;

    // 各周波数のBiquadFilterを作成
    filterNodes = EQ_FREQUENCIES.map((freq) => {
      const filter = context.createBiquadFilter();
      filter.type = 'peaking';
      filter.frequency.value = freq;
      filter.Q.value = 1.4; // バンド幅を調整
      filter.gain.value = 0;
      return filter;
    });

    // ゲインノードを作成（最終出力用）
    gainNode = context.createGain();
    gainNode.gain.value = 1.0;

    // イコライザ以降を直列接続: filter1 -> ... -> filter10 -> gain -> destination
    // （audio要素をつなぐ前に出力までつなぎ、途中で失敗しても、つないだデッキの音が出るようにする）
    for (let i = 0; i < filterNodes.length - 1; i++) {
      filterNodes[i].connect(filterNodes[i + 1]);
    }
    filterNodes[filterNodes.length - 1].connect(gainNode);
    gainNode.connect(context.destination);

    // デッキごとに: audio要素 -> 音量の正規化（ReplayGain） -> フェード（クロスフェード） -> イコライザの先頭
    // （曲ごとの音量差を、イコライザの前でそろえる）
    sourceNodes = [];
    normalizationNodes = [];
    fadeNodes = [];
    audioElements.forEach((audioElement, deck) => {
      const source = context.createMediaElementSource(audioElement);
      const normalization = context.createGain();
      normalization.gain.value = normalizationGainValues[deck] ?? 1;
      const fade = context.createGain();
      fade.gain.value = 1;
      source.connect(normalization);
      normalization.connect(fade);
      fade.connect(filterNodes[0]);
      sourceNodes.push(source);
      normalizationNodes.push(normalization);
      fadeNodes.push(fade);
    });

    isInitialized = true;

    // 保存されている設定を適用
    applyEqualizerSettings(equalizer);
  } catch (error) {
    console.error('イコライザの初期化に失敗しました:', error);
  }
}

/**
 * イコライザ設定をノードのゲインに反映する（ノードが未接続なら何もしない）
 */
function applyEqualizerSettings(settings: Pick<EqualizerSettings, 'enabled' | 'bands'>): void {
  if (!isInitialized || filterNodes.length === 0) return;

  EQ_FREQUENCIES.forEach((freq, index) => {
    if (filterNodes[index]) {
      // イコライザが無効の場合はゲインを0に設定
      filterNodes[index].gain.value = settings.enabled ? settings.bands[freq] : 0;
    }
  });
}

/**
 * イコライザをクリーンアップ
 */
export async function cleanupEqualizer(): Promise<void> {
  sourceNodes.forEach((source) => source.disconnect());
  sourceNodes = [];

  normalizationNodes.forEach((normalization) => normalization.disconnect());
  normalizationNodes = [];

  fadeNodes.forEach((fade) => fade.disconnect());
  fadeNodes = [];

  filterNodes.forEach((filter) => filter.disconnect());
  filterNodes = [];

  if (gainNode) {
    gainNode.disconnect();
    gainNode = null;
  }

  if (audioContext) {
    await audioContext.close();
    audioContext = null;
  }

  isInitialized = false;
}

/**
 * デッキの音量の正規化の倍率を設定する（再生コントローラーが、曲や設定が変わるたびに呼ぶ）
 *
 * ノードの接続前に呼ばれた場合は値を覚えておき、接続時に反映する。
 * @param deck デッキの番号（`initializeEqualizer`に渡したaudio要素の位置）
 */
export function setNormalizationGain(deck: number, gain: number): void {
  normalizationGainValues[deck] = gain;
  const node = normalizationNodes[deck];
  if (node) {
    node.gain.value = gain;
  }
}

/** フェードの変化を表す点の数 */
const FADE_CURVE_LENGTH = 64;

/**
 * フェードの音量の変化（等パワー）
 *
 * 2つの曲を重ねたときに、合計の大きさが途中で下がらないよう、フェードインはsin、
 * フェードアウトはcosで変化させる（2乗の和が常に1になる）。
 */
function fadeCurve(direction: 'in' | 'out'): Float32Array {
  const curve = new Float32Array(FADE_CURVE_LENGTH);
  for (let i = 0; i < FADE_CURVE_LENGTH; i++) {
    const angle = (i / (FADE_CURVE_LENGTH - 1)) * (Math.PI / 2);
    curve[i] = direction === 'in' ? Math.sin(angle) : Math.cos(angle);
  }
  return curve;
}

/**
 * デッキの音量を、今から指定した秒数でフェードさせる（クロスフェード）
 *
 * 経路が未接続の場合は何もしない（呼び出し側は`isEqualizerInitialized`で確かめてから使う）。
 * @param deck デッキの番号（`initializeEqualizer`に渡したaudio要素の位置）
 */
export function fadeDeck(deck: number, direction: 'in' | 'out', seconds: number): void {
  const node = fadeNodes[deck];
  if (!node || !audioContext) return;
  // 途中のフェードがあれば取り消してから始める（予定が重なるとエラーになるため）
  node.gain.cancelScheduledValues(0);
  node.gain.setValueCurveAtTime(fadeCurve(direction), audioContext.currentTime, seconds);
}

/**
 * デッキのフェードを取り消し、音量を決める（1で通常の音量）
 * @param deck デッキの番号（`initializeEqualizer`に渡したaudio要素の位置）
 */
export function setDeckFade(deck: number, value: number): void {
  const node = fadeNodes[deck];
  if (!node) return;
  node.gain.cancelScheduledValues(0);
  node.gain.value = value;
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
