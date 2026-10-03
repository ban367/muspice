<script lang="ts">
  import { ui, MIN_CARD_SIZE, MAX_CARD_SIZE } from '#lib/stores/ui.svelte.js';

  // サイズを変更
  function handleSizeChange(event: Event) {
    const target = event.target as HTMLInputElement;
    ui.gridCardSize = Number(target.value);
  }
</script>

<div class="flex items-center gap-2">
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    class="w-3.5 h-3.5 text-text-dimmed shrink-0"
  >
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </svg>
  <input
    type="range"
    min={MIN_CARD_SIZE}
    max={MAX_CARD_SIZE}
    value={ui.gridCardSize}
    oninput={handleSizeChange}
    class="size-range"
  />
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    class="w-5 h-5 text-text-dimmed shrink-0"
  >
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </svg>
</div>

<style>
  @reference "../../../app.css";
  .size-range {
    @apply w-20 h-1 rounded-sm cursor-pointer;
    -webkit-appearance: none;
    appearance: none;
    @apply bg-surface-active;
  }

  .size-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer transition-transform duration-150;
  }

  .size-range::-webkit-slider-thumb:hover {
    @apply scale-125;
  }

  .size-range::-moz-range-thumb {
    @apply w-3 h-3 rounded-full bg-primary cursor-pointer border-none transition-transform duration-150;
  }

  .size-range::-moz-range-thumb:hover {
    @apply scale-125;
  }
</style>
