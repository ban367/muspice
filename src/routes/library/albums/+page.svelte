<script lang="ts">
  import AlbumGrid from '$lib/components/library/AlbumGrid.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { useAlbumsGroupedQuery } from '$lib/queries/tracks';

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('grid');

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const albumCount = $derived(albumsQuery.data?.length ?? 0);

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

<div class="albums-page">
  <LibraryHeader
    title="アルバム"
    count={albumCount}
    countUnit="枚"
    searchPlaceholder="アルバムを検索..."
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={true}
  />

  <!-- アルバムグリッド -->
  <div class="grid-container">
    <AlbumGrid {displayMode} />
  </div>
</div>

<style>
@reference "../../../app.css";
  .albums-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }
</style>
