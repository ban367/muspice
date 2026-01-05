<script lang="ts">
  import GenreGrid from '$lib/components/library/GenreGrid.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';
  import { useQueryClient } from '@tanstack/svelte-query';

  // QueryClient for refetching
  const queryClient = useQueryClient();

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('grid');

  // クエリ
  const genresQuery = useGenresGroupedQuery();
  const genreCount = $derived(genresQuery.data?.length ?? 0);

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

  function handleRefreshComplete() {
    queryClient.invalidateQueries({ queryKey: ['tracks'] });
    queryClient.invalidateQueries({ queryKey: ['albums'] });
    queryClient.invalidateQueries({ queryKey: ['artists'] });
    queryClient.invalidateQueries({ queryKey: ['genres'] });
  }
</script>

<div class="genres-page">
  <LibraryHeader
    title="ジャンル"
    count={genreCount}
    countUnit="種類"
    searchPlaceholder="ジャンルを検索..."
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={false}
    onRefreshComplete={handleRefreshComplete}
  />

  <!-- ジャンルグリッド -->
  <div class="grid-container">
    <GenreGrid {displayMode} />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .genres-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }
</style>
