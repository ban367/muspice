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

<div class="flex gap-1 bg-surface p-1 rounded-lg">
  {#each modes as mode}
    <button
      class="mode-button"
      class:active={$browseMode === mode.value}
      onclick={() => handleModeChange(mode.value)}
      title={mode.label}
    >
      {#if mode.icon === 'music'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4 shrink-0">
          <path d="M9 18V5l12-2v13" />
          <circle cx="6" cy="18" r="3" />
          <circle cx="18" cy="16" r="3" />
        </svg>
      {:else if mode.icon === 'album'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4 shrink-0">
          <circle cx="12" cy="12" r="10" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      {:else if mode.icon === 'artist'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4 shrink-0">
          <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
      {:else if mode.icon === 'genre'}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4 shrink-0">
          <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
          <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
        </svg>
      {/if}
      <span class="mode-label">{mode.label}</span>
    </button>
  {/each}
</div>

<style>
@reference "../../../app.css";
  .mode-button {
    @apply flex items-center gap-1.5 py-2 px-3 border-none bg-transparent text-text-muted text-sm rounded-md cursor-pointer transition-all duration-150;
  }

  .mode-button:hover {
    @apply bg-surface-hover text-text-secondary;
  }

  .mode-button.active {
    @apply bg-primary/20 text-primary;
  }

  .mode-label {
    @apply whitespace-nowrap;
  }

  /* レスポンシブ対応 */
  @media (max-width: 640px) {
    .mode-label {
      @apply hidden;
    }

    .mode-button {
      @apply p-2;
    }
  }
</style>
