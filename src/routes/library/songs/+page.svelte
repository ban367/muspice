<script lang="ts">
  import { useTracksQuery, useSearchQuery } from '$lib/queries/tracks';
  import { sanitizeSearchQuery } from '$lib/utils/validation';
  import TrackList from '$lib/components/library/TrackList.svelte';

  // 検索状態
  let searchTerm = $state('');
  let debouncedSearchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // クエリ
  const tracksQuery = useTracksQuery();

  // 検索クエリ
  const searchQuery = $derived.by(() => {
    if (debouncedSearchTerm.length > 0) {
      return useSearchQuery(debouncedSearchTerm);
    }
    return null;
  });

  // アクティブなクエリ
  const activeQuery = $derived(searchQuery || tracksQuery);
  const tracks = $derived(activeQuery.data ?? null);
  const isLoading = $derived(activeQuery.isLoading);
  const isError = $derived(activeQuery.isError);
  const error = $derived(activeQuery.error);

  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchTerm = target.value;

    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
      debouncedSearchTerm = sanitizeSearchQuery(searchTerm);
    }, 300);
  }

  function clearSearch() {
    searchTerm = '';
    debouncedSearchTerm = '';
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  }
</script>

<div class="songs-page">
  <!-- 検索バー -->
  <div class="search-section">
    <div class="search-box">
      <svg xmlns="http://www.w3.org/2000/svg" class="search-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        type="text"
        placeholder="曲を検索..."
        value={searchTerm}
        oninput={handleSearchInput}
        class="search-input"
      />
      {#if searchTerm}
        <button onclick={clearSearch} class="search-clear" aria-label="検索をクリア">✕</button>
      {/if}
    </div>
  </div>

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      searchTerm={debouncedSearchTerm}
    />
  </div>
</div>

<style>
@reference "../../../app.css";
  .songs-page {
    @apply flex flex-col h-full;
  }

  .search-section {
    @apply p-4 pb-0;
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
    @apply outline-none border-primary w-80;
  }

  .search-input::placeholder {
    @apply text-text-dimmed;
  }

  .search-clear {
    @apply absolute right-2 p-1 text-text-dimmed hover:text-text-primary bg-transparent border-none cursor-pointer;
  }

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
