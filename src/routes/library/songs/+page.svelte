<script lang="ts">
  import { useTracksQuery, useSearchQuery } from '$lib/queries/tracks';
  import { sanitizeSearchQuery } from '$lib/utils/validation';
  import TrackList from '$lib/components/library/TrackList.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('list');

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
  const trackCount = $derived(tracks?.length ?? 0);

  function handleSearchInput(value: string) {
    searchTerm = value;

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

  function handleDisplayModeChange(mode: 'grid' | 'list') {
    displayMode = mode;
  }
</script>

<div class="songs-page">
  <LibraryHeader
    title="曲"
    count={trackCount}
    countUnit="曲"
    searchPlaceholder="曲を検索..."
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={true}
  />

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      searchTerm={debouncedSearchTerm}
      {displayMode}
    />
  </div>
</div>

<style>
@reference "../../../app.css";
  .songs-page {
    @apply flex flex-col h-full;
  }

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
