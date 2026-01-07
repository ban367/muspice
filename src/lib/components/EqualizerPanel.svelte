<script lang="ts">
  import {
    equalizer,
    EQ_FREQUENCIES,
    EQ_FREQUENCY_LABELS,
    MIN_GAIN,
    MAX_GAIN,
    BUILTIN_PRESET_LABELS,
    isBuiltinPreset,
    type BuiltinPresetName,
    type EQFrequency
  } from '$lib/stores/equalizer';

  // ビルトインプリセットの選択肢
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

  // カスタムプリセット保存ダイアログ
  let showSaveDialog = $state(false);
  let newPresetName = $state('');

  // カスタム状態かどうか（プリセットが選択されていない= スライダー操作された状態）
  const isCustom = $derived($equalizer.currentPreset === null);

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

  // カスタムプリセットを保存
  function savePreset() {
    const trimmedName = newPresetName.trim();
    if (trimmedName) {
      equalizer.saveCustomPreset(trimmedName);
      newPresetName = '';
      showSaveDialog = false;
    }
  }

  // カスタムプリセットを削除
  function deletePreset(name: string) {
    if (confirm(`"${name}" を削除しますか？`)) {
      equalizer.deleteCustomPreset(name);
    }
  }

  // 現在選択されているプリセットの値（セレクト用）
  const currentSelectValue = $derived(
    $equalizer.currentPreset === null ? '_custom_' : $equalizer.currentPreset
  );
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

  <!-- プリセットコントロール -->
  <div class="eq-controls">
    <select
      class="preset-select"
      value={currentSelectValue}
      onchange={handlePresetChange}
      disabled={!$equalizer.enabled}
    >
      <!-- カスタム状態を表示 -->
      {#if isCustom}
        <option value="_custom_" disabled>カスタム</option>
      {/if}
      <optgroup label="ビルトイン">
        {#each builtinPresetOptions as preset}
          <option value={preset}>{BUILTIN_PRESET_LABELS[preset]}</option>
        {/each}
      </optgroup>
      {#if $equalizer.customPresets.length > 0}
        <optgroup label="保存済み">
          {#each $equalizer.customPresets as customPreset}
            <option value={customPreset.name}>{customPreset.name}</option>
          {/each}
        </optgroup>
      {/if}
    </select>

    <!-- 保存ボタン（カスタム状態でのみ有効） -->
    <button
      class="icon-btn"
      class:highlight={isCustom}
      onclick={() => (showSaveDialog = !showSaveDialog)}
      disabled={!$equalizer.enabled || !isCustom}
      title={isCustom ? 'プリセットを保存' : 'プリセットを変更すると保存可能'}
      aria-label="プリセットを保存"
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
        <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
        <polyline points="17 21 17 13 7 13 7 21" />
        <polyline points="7 3 7 8 15 8" />
      </svg>
    </button>

    <!-- リセットボタン -->
    <button
      class="icon-btn"
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

  <!-- プリセット保存ダイアログ -->
  {#if showSaveDialog && isCustom}
    <div class="save-dialog">
      <input
        type="text"
        class="preset-name-input"
        placeholder="プリセット名を入力"
        bind:value={newPresetName}
        onkeydown={(e) => e.key === 'Enter' && savePreset()}
      />
      <button class="save-btn" onclick={savePreset} disabled={!newPresetName.trim()}>保存</button>
      <button class="cancel-btn" onclick={() => (showSaveDialog = false)}>×</button>
    </div>
  {/if}

  <!-- 選択中のカスタムプリセット名と削除ボタン -->
  {#if $equalizer.currentPreset && !isBuiltinPreset($equalizer.currentPreset)}
    <div class="custom-preset-info">
      <span class="preset-name">📁 {$equalizer.currentPreset}</span>
      <button
        class="delete-preset-btn"
        onclick={() => deletePreset($equalizer.currentPreset!)}
        title="プリセットを削除"
      >
        削除
      </button>
    </div>
  {/if}

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
          <!-- ネイティブスライダー（透明）
               min/maxを反転して上に行くほど値が大きくなるようにする -->
          <input
            type="range"
            min={-MAX_GAIN}
            max={-MIN_GAIN}
            step="1"
            value={gainToSliderValue($equalizer.bands[freq])}
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
    max-height: 380px;
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
    @apply flex items-center gap-1 shrink-0;
  }

  .preset-select {
    @apply flex-1 px-2 py-1 bg-surface border border-border rounded text-xs text-text-primary
           cursor-pointer transition-colors duration-200;
  }

  .preset-select:hover:not(:disabled) {
    @apply border-border-light;
  }

  .preset-select:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .icon-btn {
    @apply p-1 bg-transparent border border-border rounded text-text-secondary
           cursor-pointer transition-all duration-200 flex items-center justify-center;
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

  /* プリセット保存ダイアログ */
  .save-dialog {
    @apply flex items-center gap-1 shrink-0;
  }

  .preset-name-input {
    @apply flex-1 px-2 py-1 bg-surface border border-border rounded text-xs text-text-primary;
  }

  .preset-name-input:focus {
    @apply outline-none border-primary;
  }

  .save-btn {
    @apply px-2 py-1 bg-primary text-white text-xs rounded border-none cursor-pointer;
  }

  .save-btn:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .cancel-btn {
    @apply px-1.5 py-1 bg-transparent text-text-secondary text-xs rounded border border-border cursor-pointer;
  }

  /* カスタムプリセット情報 */
  .custom-preset-info {
    @apply flex items-center justify-between px-2 py-1 bg-surface-active rounded text-xs shrink-0;
  }

  .preset-name {
    @apply text-text-primary truncate;
  }

  .delete-preset-btn {
    @apply px-1.5 py-0.5 bg-transparent text-error text-[0.6rem] rounded border border-error/30 cursor-pointer
           hover:bg-error/10;
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
