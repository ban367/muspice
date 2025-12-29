<script lang="ts">
  import { browseMode, type BrowseMode } from '$lib/stores/ui';

  // ブラウズモードの定義
  const modes: { value: BrowseMode; label: string; icon: string }[] = [
    { value: 'songs', label: '曲', icon: 'music' },
    { value: 'albums', label: 'アルバム', icon: 'album' },
    { value: 'artists', label: 'アーティスト', icon: 'artist' },
    { value: 'genres', label: 'ジャンル', icon: 'genre' }
  ];

  function handleModeChange(mode: BrowseMode) {
    browseMode.set(mode);
  }
</script>

<div class="browse-mode-selector">
  {#each modes as mode}
    <button
      class="mode-button"
      class:active={$browseMode === mode.value}
      onclick={() => handleModeChange(mode.value)}
      title={mode.label}
    >
      {#if mode.icon === 'music'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M9 18V5l12-2v13" />
          <circle cx="6" cy="18" r="3" />
          <circle cx="18" cy="16" r="3" />
        </svg>
      {:else if mode.icon === 'album'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      {:else if mode.icon === 'artist'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
      {:else if mode.icon === 'genre'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
          <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
        </svg>
      {/if}
      <span class="mode-label">{mode.label}</span>
    </button>
  {/each}
</div>

<style>
  .browse-mode-selector {
    display: flex;
    gap: 0.25rem;
    background: rgba(255, 255, 255, 0.05);
    padding: 0.25rem;
    border-radius: 0.5rem;
  }

  .mode-button {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.5rem 0.75rem;
    border: none;
    background: transparent;
    color: #888;
    font-size: 0.875rem;
    border-radius: 0.375rem;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .mode-button:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #ccc;
  }

  .mode-button.active {
    background: rgba(59, 130, 246, 0.2);
    color: #3b82f6;
  }

  .mode-button svg {
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
  }

  .mode-label {
    white-space: nowrap;
  }

  /* レスポンシブ対応 */
  @media (max-width: 640px) {
    .mode-label {
      display: none;
    }

    .mode-button {
      padding: 0.5rem;
    }
  }
</style>
