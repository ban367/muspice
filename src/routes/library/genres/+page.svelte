<script lang="ts">
  import GenreGrid from '$lib/components/library/GenreGrid.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';

  // 検索状態
  let searchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchTerm = target.value;

    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
      browseSearchQuery.set(searchTerm);
    }, 300);
  }

  function clearSearch() {
    searchTerm = '';
    browseSearchQuery.set('');
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  }
</script>

<div class="genres-page">
  <!-- 検索バー -->
  <div class="header-section">
    <div class="search-box">
      <svg xmlns="http://www.w3.org/2000/svg" class="search-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        type="text"
        placeholder="ジャンルを検索..."
        value={searchTerm}
        oninput={handleSearchInput}
        class="search-input"
      />
      {#if searchTerm}
        <button onclick={clearSearch} class="search-clear" aria-label="検索をクリア">✕</button>
      {/if}
    </div>
  </div>

  <!-- ジャンルグリッド -->
  <div class="grid-container">
    <GenreGrid />
  </div>
</div>

<style>
@reference "../../../app.css";
  .genres-page {
    @apply flex flex-col h-full;
  }

  .header-section {
    @apply flex items-center gap-4 p-4 pb-0;
  }

  .search-box {
    @apply relative flex items-center max-w-md;
  }

  .search-icon {
    @apply absolute left-3 w-4 h-4 text-text-dimmed;
  }

  .search-input {
    @apply w-full py-2 pl-9 pr-8 bg-base-400 border border-border rounded-md text-sm text-text-primary transition-all duration-200;
  }

  .search-input:focus {
    @apply outline-none border-primary;
  }

  .search-input::placeholder {
    @apply text-text-dimmed;
  }

  .search-clear {
    @apply absolute right-2 p-1 text-text-dimmed hover:text-text-primary bg-transparent border-none cursor-pointer;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }
</style>
