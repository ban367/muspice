<!--
  @component EqualizerSection
  サウンドのパネル（`SoundPanel`）のイコライザのまとまり。見出しの右端でオン・オフを切り替え、
  プリセットの選択・保存・削除・リセットと、10バンドのゲインのスライダーを出す。
  どのプリセットを選んでいても、並びと高さは変えない（保存はテキスト入力のダイアログで名前を聞き、
  削除のボタンは、保存したプリセットを選んでいる時だけ使える）。
-->
<script lang="ts">
  import {
    equalizer,
    EQ_FREQUENCIES,
    EQ_FREQUENCY_LABELS,
    MIN_GAIN,
    MAX_GAIN,
    isBuiltinPreset,
    type BuiltinPresetName,
    type EQFrequency
  } from '#lib/stores/equalizer.svelte.js';
  import { confirmDestructive, promptText } from '#lib/utils/dialog.svelte.js';
  import SidePanelSection from './SidePanelSection.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // ビルトインプリセットの選択肢（名前は、表示の言語に合わせてテンプレートで`m`から読む）
  const builtinPresetOptions: BuiltinPresetName[] = [
    'flat',
    'bass_boost',
    'treble_boost',
    'vocal',
    'rock',
    'pop',
    'jazz',
    'classical'
  ];

  // カスタム状態かどうか（プリセットが選択されていない= スライダー操作された状態）
  const isCustom = $derived(equalizer.currentPreset === null);

  // 選んでいる、保存したプリセットの名前（ビルトイン・カスタム状態の時はnull）
  const selectedCustomPreset = $derived(
    equalizer.currentPreset !== null && !isBuiltinPreset(equalizer.currentPreset)
      ? equalizer.currentPreset
      : null
  );

  // プリセット変更ハンドラー
  function handlePresetChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const presetName = target.value;
    // "custom"は特別な値なので無視
    if (presetName && presetName !== '_custom_') {
      equalizer.applyPreset(presetName);
    }
  }

  // バンドゲイン変更ハンドラー
  // スライダーはmin=MAX, max=MINで逆にしているため、値を反転
  function handleBandChange(frequency: EQFrequency, event: Event) {
    const target = event.target as HTMLInputElement;
    const invertedValue = parseFloat(target.value);
    // 反転を戻す（スライダーが逆なので）
    const gain = -invertedValue;
    equalizer.setBandGain(frequency, gain);
  }

  // ゲイン表示用フォーマット
  function formatGain(gain: number): string {
    if (gain > 0) return `+${gain}`;
    return gain.toString();
  }

  // ゲインから位置を計算（0-100%、下が-12、上が+12）
  function gainToPercent(gain: number): number {
    return ((gain - MIN_GAIN) / (MAX_GAIN - MIN_GAIN)) * 100;
  }

  // ゲインをスライダー値に変換（反転）
  function gainToSliderValue(gain: number): number {
    return -gain;
  }

  // カスタムプリセットを保存（名前は、テキスト入力のダイアログで聞く）
  async function savePreset() {
    const name = await promptText({
      title: m.equalizer.savePreset,
      label: m.equalizer.presetName,
      confirmLabel: m.common.save
    });
    if (name !== null) {
      equalizer.saveCustomPreset(name);
    }
  }

  // カスタムプリセットを削除
  async function deletePreset(name: string) {
    if (await confirmDestructive(m.equalizer.confirmDeletePreset(name))) {
      equalizer.deleteCustomPreset(name);
    }
  }

  // 現在選択されているプリセットの値（セレクト用）
  const currentSelectValue = $derived(
    equalizer.currentPreset === null ? '_custom_' : equalizer.currentPreset
  );
</script>

<SidePanelSection title={m.equalizer.title}>
  {#snippet actions()}
    <button
      type="button"
      class="eq-toggle"
      class:active={equalizer.enabled}
      onclick={() => equalizer.toggle()}
      title={equalizer.enabled ? m.equalizer.turnOff : m.equalizer.turnOn}
      aria-label={equalizer.enabled ? m.equalizer.turnOff : m.equalizer.turnOn}
    >
      <span class="toggle-track">
        <span class="toggle-thumb"></span>
      </span>
    </button>
  {/snippet}

  <div class="equalizer">
    <!-- プリセットコントロール -->
    <div class="eq-controls">
      <select
        class="preset-select"
        value={currentSelectValue}
        onchange={handlePresetChange}
        disabled={!equalizer.enabled}
        aria-label={m.equalizer.preset}
      >
        <!-- カスタム状態を表示 -->
        {#if isCustom}
          <option value="_custom_" disabled>{m.equalizer.custom}</option>
        {/if}
        <optgroup label={m.equalizer.builtIn}>
          {#each builtinPresetOptions as preset (preset)}
            <option value={preset}>{m.equalizer.presets[preset]}</option>
          {/each}
        </optgroup>
        {#if equalizer.customPresets.length > 0}
          <optgroup label={m.equalizer.saved}>
            {#each equalizer.customPresets as customPreset (customPreset.name)}
              <option value={customPreset.name}>{customPreset.name}</option>
            {/each}
          </optgroup>
        {/if}
      </select>

      <!-- 保存ボタン（カスタム状態でのみ有効） -->
      <button
        type="button"
        class="icon-btn"
        class:highlight={isCustom && equalizer.enabled}
        onclick={savePreset}
        disabled={!equalizer.enabled || !isCustom}
        title={isCustom ? m.equalizer.savePreset : m.equalizer.savePresetHint}
        aria-label={m.equalizer.savePreset}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          aria-hidden="true"
        >
          <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
          <polyline points="17 21 17 13 7 13 7 21" />
          <polyline points="7 3 7 8 15 8" />
        </svg>
      </button>

      <!-- 削除ボタン（保存したプリセットを選んでいる時だけ有効） -->
      <button
        type="button"
        class="icon-btn"
        onclick={() => selectedCustomPreset !== null && deletePreset(selectedCustomPreset)}
        disabled={!equalizer.enabled || selectedCustomPreset === null}
        title={m.equalizer.deletePreset}
        aria-label={m.equalizer.deletePreset}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6" />
        </svg>
      </button>

      <!-- リセットボタン -->
      <button
        type="button"
        class="icon-btn"
        onclick={() => equalizer.reset()}
        disabled={!equalizer.enabled}
        title={m.equalizer.reset}
        aria-label={m.equalizer.resetLabel}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          aria-hidden="true"
        >
          <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
          <path d="M3 3v5h5" />
        </svg>
      </button>
    </div>

    <!-- スライダーエリア -->
    <div class="eq-sliders-wrapper" class:disabled={!equalizer.enabled}>
      <!-- ゲインラベル（左側） -->
      <div class="gain-labels">
        <span>+{MAX_GAIN}</span>
        <span>0</span>
        <span>{MIN_GAIN}</span>
      </div>

      <!-- スライダーグリッド -->
      <div class="sliders-grid">
        {#each EQ_FREQUENCIES as freq (freq)}
          <div class="band-column">
            <!-- カスタムスライダー表示 -->
            <div class="custom-slider">
              <div class="slider-track"></div>
              <div class="slider-center-line"></div>
              <div
                class="slider-thumb"
                style="bottom: {gainToPercent(equalizer.bands[freq])}%"
              ></div>
            </div>
            <!-- ネイティブスライダー（透明）
                 min/maxを反転して上に行くほど値が大きくなるようにする -->
            <input
              type="range"
              min={-MAX_GAIN}
              max={-MIN_GAIN}
              step="1"
              value={gainToSliderValue(equalizer.bands[freq])}
              oninput={(e) => handleBandChange(freq, e)}
              disabled={!equalizer.enabled}
              class="native-slider"
              title="{EQ_FREQUENCY_LABELS[freq]}Hz: {formatGain(equalizer.bands[freq])}dB"
              aria-label="{EQ_FREQUENCY_LABELS[freq]}Hz"
            />
          </div>
        {/each}
      </div>
    </div>

    <!-- 周波数ラベルと値 -->
    <div class="freq-row">
      <div class="freq-spacer"></div>
      {#each EQ_FREQUENCIES as freq (freq)}
        <div class="freq-cell">
          <span class="freq-label">{EQ_FREQUENCY_LABELS[freq]}</span>
          <span
            class="gain-value"
            class:positive={equalizer.bands[freq] > 0}
            class:negative={equalizer.bands[freq] < 0}
          >
            {formatGain(equalizer.bands[freq])}
          </span>
        </div>
      {/each}
    </div>
  </div>
</SidePanelSection>

<style>
  @reference "../../../app.css";

  .equalizer {
    @apply flex flex-col gap-2;
  }

  /* トグルスイッチ */
  .eq-toggle {
    @apply p-0 bg-transparent border-none cursor-pointer;
  }

  /* オフの間は、テーマの文字の色のつまみを、少し濃い面に置く（白のつまみは、ライトテーマで見えない） */
  .toggle-track {
    @apply relative block w-10 h-5 rounded-full bg-surface-strong transition-colors duration-200;
  }

  .eq-toggle.active .toggle-track {
    @apply bg-secondary;
  }

  .toggle-thumb {
    @apply absolute left-0.5 top-0.5 w-4 h-4 rounded-full bg-text-primary transition-transform duration-200;
  }

  /* オンの間は、色の付いた面の上の白（どちらのテーマでも同じ） */
  .eq-toggle.active .toggle-thumb {
    @apply translate-x-5 bg-white;
  }

  /* コントロールエリア */
  .eq-controls {
    @apply flex items-center gap-1;
  }

  .preset-select {
    @apply flex-1 min-w-0 h-7 px-2 bg-surface border border-border rounded-md text-xs text-text-primary
           cursor-pointer transition-colors duration-200;
  }

  .preset-select:hover:not(:disabled) {
    @apply border-border-light;
  }

  .preset-select:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .icon-btn {
    @apply w-7 h-7 shrink-0 p-0 bg-transparent border border-border rounded-md text-text-secondary
           cursor-pointer transition-all duration-200 flex items-center justify-center;
  }

  .icon-btn svg {
    @apply w-3.5 h-3.5;
  }

  .icon-btn:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .icon-btn:disabled {
    @apply opacity-30 cursor-not-allowed;
  }

  .icon-btn.highlight {
    @apply border-primary text-primary;
  }

  .icon-btn.highlight:hover:not(:disabled) {
    @apply bg-primary/10;
  }

  /* スライダーエリア */
  .eq-sliders-wrapper {
    @apply flex gap-1 overflow-hidden;
    height: 160px;
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
    direction: ltr;
  }

  .native-slider:disabled {
    @apply cursor-not-allowed;
  }

  /* 周波数ラベル行 */
  .freq-row {
    @apply flex pt-1 border-t border-border;
  }

  .freq-spacer {
    width: 20px;
    @apply shrink-0;
  }

  .freq-cell {
    @apply flex-1 flex flex-col items-center;
  }

  .freq-label {
    @apply text-[0.5rem] text-text-dimmed;
  }

  .gain-value {
    @apply text-[0.5rem] text-text-dimmed tabular-nums;
  }

  .gain-value.positive {
    @apply text-success;
  }

  .gain-value.negative {
    @apply text-error;
  }
</style>
