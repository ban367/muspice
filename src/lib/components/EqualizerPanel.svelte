<script lang="ts">
  import {
    equalizer,
    EQ_FREQUENCIES,
    EQ_FREQUENCY_LABELS,
    MIN_GAIN,
    MAX_GAIN,
    PRESET_LABELS,
    type PresetName,
    type EQFrequency
  } from '$lib/stores/equalizer';

  // プリセットの選択肢
  const presetOptions: PresetName[] = [
    'flat',
    'bass_boost',
    'treble_boost',
    'vocal',
    'rock',
    'pop',
    'jazz',
    'classical'
  ];

  // スライダーの値を反転（上が+、下が-）
  function invertGain(gain: number): number {
    return -gain;
  }

  // スライダーの値を元に戻す
  function revertGain(inverted: number): number {
    return -inverted;
  }

  // プリセット変更ハンドラー
  function handlePresetChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const presetName = target.value as PresetName;
    equalizer.applyPreset(presetName);
  }

  // バンドゲイン変更ハンドラー
  function handleBandChange(frequency: EQFrequency, event: Event) {
    const target = event.target as HTMLInputElement;
    const invertedValue = parseFloat(target.value);
    const gain = revertGain(invertedValue);
    equalizer.setBandGain(frequency, gain);
  }

  // ゲイン表示用フォーマット
  function formatGain(gain: number): string {
    if (gain > 0) return `+${gain}`;
    return gain.toString();
  }

  // ゲインから位置を計算（0-100%）
  function gainToPercent(gain: number): number {
    return ((gain - MIN_GAIN) / (MAX_GAIN - MIN_GAIN)) * 100;
  }
</script>

<div class="equalizer-panel">
  <!-- ヘッダー -->
  <div class="eq-header">
    <h3>イコライザ</h3>
    <button
      class="eq-toggle"
      class:active={$equalizer.enabled}
      onclick={() => equalizer.toggle()}
      title={$equalizer.enabled ? 'イコライザをオフ' : 'イコライザをオン'}
      aria-label={$equalizer.enabled ? 'イコライザをオフ' : 'イコライザをオン'}
    >
      <span class="toggle-track">
        <span class="toggle-thumb"></span>
      </span>
    </button>
  </div>

  <!-- プリセットとリセット -->
  <div class="eq-controls">
    <select
      class="preset-select"
      value={$equalizer.currentPreset || 'flat'}
      onchange={handlePresetChange}
      disabled={!$equalizer.enabled}
    >
      {#each presetOptions as preset}
        <option value={preset}>{PRESET_LABELS[preset]}</option>
      {/each}
    </select>
    <button
      class="reset-btn"
      onclick={() => equalizer.reset()}
      disabled={!$equalizer.enabled}
      title="リセット"
      aria-label="イコライザをリセット"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
        <path d="M3 3v5h5" />
      </svg>
    </button>
  </div>

  <!-- スライダーエリア -->
  <div class="eq-sliders-wrapper" class:disabled={!$equalizer.enabled}>
    <!-- ゲインラベル（左側） -->
    <div class="gain-labels">
      <span>+{MAX_GAIN}</span>
      <span>0</span>
      <span>{MIN_GAIN}</span>
    </div>

    <!-- スライダーグリッド -->
    <div class="sliders-grid">
      {#each EQ_FREQUENCIES as freq}
        <div class="band-column">
          <!-- カスタムスライダー表示 -->
          <div class="custom-slider">
            <div class="slider-track"></div>
            <div class="slider-center-line"></div>
            <div
              class="slider-thumb"
              style="bottom: {gainToPercent($equalizer.bands[freq])}%"
            ></div>
          </div>
          <!-- 非表示のネイティブスライダー -->
          <input
            type="range"
            min={invertGain(MAX_GAIN)}
            max={invertGain(MIN_GAIN)}
            step="1"
            value={invertGain($equalizer.bands[freq])}
            oninput={(e) => handleBandChange(freq, e)}
            disabled={!$equalizer.enabled}
            class="native-slider"
            title="{EQ_FREQUENCY_LABELS[freq]}Hz: {formatGain($equalizer.bands[freq])}dB"
            aria-label="{EQ_FREQUENCY_LABELS[freq]}Hz"
          />
        </div>
      {/each}
    </div>
  </div>

  <!-- 周波数ラベルと値 -->
  <div class="freq-row">
    <div class="freq-spacer"></div>
    {#each EQ_FREQUENCIES as freq}
      <div class="freq-cell">
        <span class="freq-label">{EQ_FREQUENCY_LABELS[freq]}</span>
        <span
          class="gain-value"
          class:positive={$equalizer.bands[freq] > 0}
          class:negative={$equalizer.bands[freq] < 0}
        >
          {formatGain($equalizer.bands[freq])}
        </span>
      </div>
    {/each}
  </div>
</div>

<style>
  @reference "../../app.css";

  .equalizer-panel {
    @apply flex flex-col p-3 gap-2 overflow-hidden;
    max-height: 320px;
  }

  /* ヘッダー */
  .eq-header {
    @apply flex items-center justify-between pb-2 border-b border-border shrink-0;
  }

  .eq-header h3 {
    @apply m-0 text-sm font-semibold text-text-primary;
  }

  /* トグルスイッチ */
  .eq-toggle {
    @apply p-0 bg-transparent border-none cursor-pointer;
  }

  .toggle-track {
    @apply relative block w-10 h-5 rounded-full bg-surface-active transition-colors duration-200;
  }

  .eq-toggle.active .toggle-track {
    @apply bg-secondary;
  }

  .toggle-thumb {
    @apply absolute left-0.5 top-0.5 w-4 h-4 rounded-full bg-white transition-transform duration-200;
  }

  .eq-toggle.active .toggle-thumb {
    @apply translate-x-5;
  }

  /* コントロールエリア */
  .eq-controls {
    @apply flex items-center gap-2 shrink-0;
  }

  .preset-select {
    @apply flex-1 px-2 py-1.5 bg-surface border border-border rounded text-xs text-text-primary
           cursor-pointer transition-colors duration-200;
  }

  .preset-select:hover:not(:disabled) {
    @apply border-border-light;
  }

  .preset-select:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .reset-btn {
    @apply p-1 bg-transparent border border-border rounded text-text-secondary
           cursor-pointer transition-all duration-200 flex items-center justify-center;
  }

  .reset-btn:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .reset-btn:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  /* スライダーエリア */
  .eq-sliders-wrapper {
    @apply flex gap-1 overflow-hidden;
    height: 180px;
  }

  .eq-sliders-wrapper.disabled {
    @apply opacity-50 pointer-events-none;
  }

  /* ゲインラベル */
  .gain-labels {
    @apply flex flex-col justify-between text-[0.5rem] text-text-dimmed shrink-0 py-1;
    width: 20px;
  }

  /* スライダーグリッド */
  .sliders-grid {
    @apply flex-1 flex;
  }

  /* バンドカラム */
  .band-column {
    @apply flex-1 relative;
  }

  /* カスタムスライダー表示 */
  .custom-slider {
    @apply absolute inset-0 flex items-center justify-center pointer-events-none;
  }

  .slider-track {
    @apply absolute w-0.5 rounded-full bg-progress-bg;
    height: 90%;
    top: 5%;
  }

  .slider-center-line {
    @apply absolute w-1 h-px bg-text-dimmed/30;
    top: 50%;
  }

  .slider-thumb {
    @apply absolute w-2.5 h-2.5 rounded-full bg-secondary;
    left: 50%;
    transform: translate(-50%, 50%);
    transition: bottom 0.05s ease-out;
  }

  /* ネイティブスライダー（透明だがインタラクティブ） */
  .native-slider {
    @apply absolute inset-0 opacity-0 cursor-pointer;
    writing-mode: vertical-lr;
    direction: rtl;
  }

  .native-slider:disabled {
    @apply cursor-not-allowed;
  }

  /* 周波数ラベル行 */
  .freq-row {
    @apply flex shrink-0 pt-1 border-t border-border;
  }

  .freq-spacer {
    width: 20px;
    @apply shrink-0;
  }

  .freq-cell {
    @apply flex-1 flex flex-col items-center;
  }

  .freq-label {
    @apply text-[0.45rem] text-text-dimmed;
  }

  .gain-value {
    @apply text-[0.45rem] text-text-dimmed;
  }

  .gain-value.positive {
    @apply text-green-400;
  }

  .gain-value.negative {
    @apply text-red-400;
  }
</style>
