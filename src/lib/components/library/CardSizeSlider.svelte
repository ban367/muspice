<script lang="ts">
  import { gridCardSize, MIN_CARD_SIZE, MAX_CARD_SIZE } from '$lib/stores/ui';

  // サイズを変更
  function handleSizeChange(event: Event) {
    const target = event.target as HTMLInputElement;
    gridCardSize.set(Number(target.value));
  }
</script>

<div class="size-slider">
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="size-icon small">
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </svg>
  <input
    type="range"
    min={MIN_CARD_SIZE}
    max={MAX_CARD_SIZE}
    value={$gridCardSize}
    oninput={handleSizeChange}
    class="size-range"
  />
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="size-icon large">
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
  </svg>
</div>

<style>
  .size-slider {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .size-icon {
    color: #666;
    flex-shrink: 0;
  }

  .size-icon.small {
    width: 14px;
    height: 14px;
  }

  .size-icon.large {
    width: 20px;
    height: 20px;
  }

  .size-range {
    width: 80px;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    cursor: pointer;
  }

  .size-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #3b82f6;
    cursor: pointer;
    transition: transform 0.15s ease;
  }

  .size-range::-webkit-slider-thumb:hover {
    transform: scale(1.2);
  }

  .size-range::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #3b82f6;
    cursor: pointer;
    border: none;
    transition: transform 0.15s ease;
  }

  .size-range::-moz-range-thumb:hover {
    transform: scale(1.2);
  }
</style>
