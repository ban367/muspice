<!--
  @component SidePanelSlider
  右サイドバーのまとまりに置く、名前と値の付いた横向きのスライダー（プリアンプ・クロスフェードの秒数など）。
  動かしている間は値の表示だけを変え、離した時に`onchange`を呼ぶ（設定の保存を、動かすたびに行わないため）。
-->
<script lang="ts">
  interface Props {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    disabled?: boolean;
    /** 値の表示（「+3 dB」「5秒」など） */
    format: (value: number) => string;
    onchange: (value: number) => void;
  }

  let { label, value, min, max, step = 1, disabled = false, format, onchange }: Props = $props();

  // 動かしている間の値（離すまでは、保存した値ではなくこちらを表示する）
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? value);

  const inputId = $props.id();
</script>

<div class="slider-field">
  <div class="slider-header">
    <label class="slider-label" for={inputId}>{label}</label>
    <span class="slider-value" class:disabled aria-hidden="true">{format(shown)}</span>
  </div>
  <input
    id={inputId}
    type="range"
    class="slider"
    {min}
    {max}
    {step}
    value={shown}
    {disabled}
    aria-valuetext={format(shown)}
    oninput={(e) => (dragging = Number(e.currentTarget.value))}
    onchange={(e) => {
      dragging = null;
      onchange(Number(e.currentTarget.value));
    }}
  />
</div>

<style>
  @reference "../../../app.css";

  .slider-header {
    @apply flex items-baseline justify-between gap-2 mb-1.5;
  }

  .slider-label {
    @apply text-xs text-text-secondary;
  }

  .slider-value {
    @apply text-xs text-primary tabular-nums;
  }

  .slider-value.disabled {
    @apply text-text-muted;
  }

  /* 再生速度のスライダー（`PlaybackAidsMenu`）と同じ見た目 */
  .slider {
    @apply block w-full h-1 rounded-sm cursor-pointer bg-surface-active;
    -webkit-appearance: none;
    appearance: none;
  }

  .slider:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer transition-transform duration-150;
  }

  .slider:not(:disabled)::-webkit-slider-thumb:hover {
    @apply scale-125;
  }

  .slider::-moz-range-thumb {
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer border-none transition-transform duration-150;
  }

  .slider:not(:disabled)::-moz-range-thumb:hover {
    @apply scale-125;
  }
</style>
