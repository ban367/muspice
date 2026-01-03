<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import AlbumGrid from '$lib/components/library/AlbumGrid.svelte';
  import AlbumList from '$lib/components/library/AlbumList.svelte';
  import AlbumDetail from '$lib/components/library/AlbumDetail.svelte';
  import LibraryHeader from '$lib/components/library/LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { useAlbumsGroupedQuery } from '$lib/queries/tracks';

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('list');

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const allAlbums = $derived(albumsQuery.data ?? []);
  const albumCount = $derived(allAlbums.length);

  // 検索でフィルタリングされたアルバム
  const filteredAlbums = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return allAlbums;
    return allAlbums.filter(
      (album) =>
        album.name.toLowerCase().includes(query) ||
        (album.artist && album.artist.toLowerCase().includes(query))
    );
  });

  // 選択されたアルバム（リストモード用）
  let selectedAlbum = $state<AlbumGroup | null>(null);

  // リストモードで最初のアルバムを自動選択
  $effect(() => {
    if (displayMode === 'list' && filteredAlbums.length > 0 && !selectedAlbum) {
      selectedAlbum = filteredAlbums[0];
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
    selectedAlbum = null;
  }

  function handleAlbumSelect(album: AlbumGroup) {
    selectedAlbum = album;
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
    showCardSizeSlider={displayMode === 'grid'}
  />

  {#if displayMode === 'grid'}
    <!-- グリッド表示（従来通り） -->
    <div class="grid-container">
      <AlbumGrid {displayMode} />
    </div>
  {:else}
    <!-- リスト表示（2ペインレイアウト） -->
    <div class="list-layout">
      <!-- 左ペイン: アルバムリスト -->
      <div class="list-pane">
        <AlbumList albums={filteredAlbums} {selectedAlbum} onSelect={handleAlbumSelect} />
      </div>

      <!-- 右ペイン: 詳細表示 -->
      <div class="detail-pane">
        {#if selectedAlbum}
          <AlbumDetail album={selectedAlbum} />
        {:else}
          <!-- すべてのアルバムビュー -->
          <div class="all-albums-view">
            <div class="empty-detail">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <circle cx="12" cy="12" r="10" />
                <circle cx="12" cy="12" r="3" />
              </svg>
              <p>アルバムを選択してください</p>
              <span>{albumCount}枚のアルバム</span>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../../app.css";

  .albums-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }

  .list-layout {
    @apply flex-1 flex overflow-hidden;
  }

  .list-pane {
    @apply w-72 shrink-0 border-r border-border overflow-hidden;
  }

  .detail-pane {
    @apply flex-1 overflow-hidden;
  }

  .all-albums-view {
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
