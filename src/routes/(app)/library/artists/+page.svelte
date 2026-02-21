<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import ArtistGrid from '$lib/components/library/ArtistGrid.svelte';
  import ArtistList from '$lib/components/library/ArtistList.svelte';
  import ArtistDetail from '$lib/components/library/ArtistDetail.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { useArtistsGroupedQuery, invalidateTrackListQueries } from '$lib/queries/tracks';
  import { useQueryClient } from '@tanstack/svelte-query';

  // QueryClient for refetching
  const queryClient = useQueryClient();

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('list');

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const allArtists = $derived(artistsQuery.data ?? []);
  const artistCount = $derived(allArtists.length);

  // 検索でフィルタリングされたアーティスト
  const filteredArtists = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return allArtists;
    return allArtists.filter((artist) => artist.name.toLowerCase().includes(query));
  });

  // 選択されたアーティスト（リストモード用）
  let selectedArtist = $state<ArtistGroup | null>(null);

  // リストモードで最初のアーティストを自動選択
  $effect(() => {
    if (displayMode === 'list' && filteredArtists.length > 0 && !selectedArtist) {
      selectedArtist = filteredArtists[0];
    }
  });

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
    // モード切り替え時に選択をリセット
    selectedArtist = null;
  }

  function handleArtistSelect(artist: ArtistGroup) {
    selectedArtist = artist;
  }

  function handleRefreshComplete() {
    invalidateTrackListQueries(queryClient);
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
    showCardSizeSlider={displayMode === 'grid'}
    onRefreshComplete={handleRefreshComplete}
  />

  {#if displayMode === 'grid'}
    <!-- グリッド表示（従来通り） -->
    <div class="grid-container">
      <ArtistGrid {displayMode} />
    </div>
  {:else}
    <!-- リスト表示（2ペインレイアウト） -->
    <div class="list-layout">
      <!-- 左ペイン: アーティストリスト -->
      <div class="list-pane">
        <ArtistList artists={filteredArtists} {selectedArtist} onSelect={handleArtistSelect} />
      </div>

      <!-- 右ペイン: 詳細表示 -->
      <div class="detail-pane">
        {#if selectedArtist}
          <ArtistDetail artist={selectedArtist} />
        {:else}
          <!-- すべてのアーティストビュー（将来的に実装）-->
          <div class="all-artists-view">
            <div class="empty-detail">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
                <circle cx="9" cy="7" r="4" />
                <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
                <path d="M16 3.13a4 4 0 0 1 0 7.75" />
              </svg>
              <p>アーティストを選択してください</p>
              <span>{artistCount}人のアーティスト</span>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../../../app.css";

  .artists-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }

  .list-layout {
    @apply flex-1 flex overflow-hidden;
  }

  .list-pane {
    @apply w-64 shrink-0 border-r border-border overflow-hidden;
  }

  .detail-pane {
    @apply flex-1 overflow-hidden;
  }

  .all-artists-view {
    @apply h-full flex items-center justify-center;
  }

  .empty-detail {
    @apply flex flex-col items-center text-center text-text-dimmed;
  }

  .empty-detail svg {
    @apply w-16 h-16 text-text-dimmed/50 mb-4;
  }

  .empty-detail p {
    @apply text-base text-text-muted m-0;
  }

  .empty-detail span {
    @apply text-sm mt-2;
  }
</style>
