<script lang="ts">
  import ArtistGrid from '$lib/components/library/ArtistGrid.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { useArtistsGroupedQuery } from '$lib/queries/tracks';

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('grid');

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const artistCount = $derived(artistsQuery.data?.length ?? 0);

  // 検索状態
  let searchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function handleSearchInput(value: string) {
    searchTerm = value;

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

  function handleDisplayModeChange(mode: 'grid' | 'list') {
    displayMode = mode;
  }
</script>

<div class="artists-page">
  <LibraryHeader
    title="アーティスト"
    count={artistCount}
    countUnit="人"
    searchPlaceholder="アーティストを検索..."
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={true}
  />

  <!-- アーティストグリッド -->
  <div class="grid-container">
    <ArtistGrid {displayMode} />
  </div>
</div>

<style>
@reference "../../../app.css";
  .artists-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }
</style>
