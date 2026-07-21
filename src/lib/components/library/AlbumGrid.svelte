<!--
  @component AlbumGrid
  アルバム一覧のグリッド/リスト表示コンポーネント。
  LibraryGridを使用して共通ロジックを委譲し、アルバム固有の表示をSnippetで実装。
-->
<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { useAlbumsGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize } from '$lib/stores/ui';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import { formatDuration } from '$lib/utils/format';
  import LibraryGrid from './LibraryGrid.svelte';
  import GroupDetail from './GroupDetail.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { intersectionObserver } from '$lib/utils/actions';

  // Props
  interface Props {
    displayMode?: 'grid' | 'list';
  }

  let { displayMode = 'grid' }: Props = $props();

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const isLoading = $derived(albumsQuery.isLoading);
  const isError = $derived(albumsQuery.isError);
  const allAlbums = $derived(albumsQuery.data ?? []);

  // 選択中のアルバム（モーダル表示用）
  let selectedAlbum = $state<AlbumGroup | null>(null);

  // LibraryGridコンポーネントの参照
  let libraryGrid: LibraryGrid<AlbumGroup>;

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // カードサイズの計算
  const cardWidth = $derived($gridCardSize + 16);

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }

  // アルバムカードが表示されたらアートを読み込み
  function handleAlbumVisible(album: AlbumGroup) {
    if (album.representativeTrackId) {
      loadAlbumArt(album.representativeTrackId);
    }
  }

  // アルバムをクリック
  function handleAlbumClick(album: AlbumGroup) {
    selectedAlbum = album;
  }

  // アルバムをダブルクリック（すべて再生）
  function handleAlbumDoubleClick(album: AlbumGroup) {
    if (album.tracks.length > 0) {
      playTrackFromQueue(album.tracks, 0);
    }
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, album: AlbumGroup) {
    event.stopPropagation();
    handleAlbumDoubleClick(album);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedAlbum = null;
  }

  // アルバムの総再生時間を計算
  function getTotalDuration(album: AlbumGroup): number {
    return album.tracks.reduce((sum, track) => sum + (track.duration || 0), 0);
  }

  // 検索フィルター
  function filterAlbum(album: AlbumGroup, query: string): boolean {
    return (
      album.name.toLowerCase().includes(query) ||
      (album.artist != null && album.artist.toLowerCase().includes(query))
    );
  }
</script>

<LibraryGrid
  bind:this={libraryGrid}
  items={allAlbums}
  {isLoading}
  {isError}
  {displayMode}
  itemLabel="アルバム"
  emptyMessage="アルバムがありません"
  emptyHint="音楽をインポートしてアルバムを追加してください"
  filterFn={filterAlbum}
  gridStyle="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;"
  gridClass="album-grid"
  groupType="album"
>
  {#snippet emptyIcon()}
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
  {/snippet}

  {#snippet gridCard(album)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="grid-card flex flex-col items-center"
      onclick={() => handleAlbumClick(album)}
      ondblclick={() => handleAlbumDoubleClick(album)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, album)}
      use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
    >
      <div class="grid-card-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
        <AlbumArt src={getArt(album.representativeTrackId)} alt={album.name} />
        <div class="play-overlay">
          <button
            class="play-button-circle"
            onclick={(e) => handlePlayClick(e, album)}
            title="アルバムを再生"
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
          </button>
        </div>
      </div>
      <div class="min-w-0 w-full text-center">
        <MarqueeText
          text={album.name}
          class="text-[0.9375rem] font-semibold text-text-primary m-0"
        />
        <MarqueeText
          text={album.artist || '不明なアーティスト'}
          class="text-[0.8125rem] text-text-muted mt-1 m-0"
        />
      </div>
    </div>
  {/snippet}

  {#snippet listRow(album)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="list-row"
      onclick={() => handleAlbumClick(album)}
      ondblclick={() => handleAlbumDoubleClick(album)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, album)}
      use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
    >
      <div class="list-art">
        <AlbumArt src={getArt(album.representativeTrackId)} alt={album.name} />
      </div>
      <div class="list-info">
        <MarqueeText text={album.name} class="list-title" />
        <MarqueeText text={album.artist || '不明なアーティスト'} class="list-artist" />
      </div>
      <div class="list-meta">
        <span>{album.trackCount}曲</span>
      </div>
      <div class="list-duration">
        {formatDuration(getTotalDuration(album))}
      </div>
      <button
        class="list-play-btn"
        onclick={(e) => handlePlayClick(e, album)}
        title="アルバムを再生"
      >
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
    </div>
  {/snippet}

  {#snippet footer()}
    <GroupDetail group={selectedAlbum} type="album" onClose={handleCloseDetail} />
  {/snippet}
</LibraryGrid>

<style>
  @reference "../../../app.css";
  :global(.album-grid) {
    @apply grid grid-cols-[repeat(auto-fill,minmax(var(--card-width),1fr))] gap-3;
  }

  .list-row {
    @apply grid gap-3 px-3 py-2.5 items-center rounded-md cursor-pointer transition-colors duration-100
           grid-cols-[3rem_1fr_5rem_4rem_2.5rem];
  }

  .list-row:hover {
    @apply bg-surface-hover;
  }

  .list-art {
    @apply w-12 h-12 rounded overflow-hidden shrink-0 flex items-center justify-center;
  }

  .list-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .list-meta {
    @apply text-xs text-text-dimmed text-right;
  }

  .list-duration {
    @apply text-xs text-text-dimmed text-right;
  }

  .list-play-btn {
    @apply w-8 h-8 flex items-center justify-center bg-transparent border-none rounded-full text-text-muted cursor-pointer transition-all duration-150 opacity-0;
  }

  .list-row:hover .list-play-btn {
    @apply opacity-100;
  }

  .list-play-btn:hover {
    @apply bg-primary text-primary-content;
  }

  .list-play-btn svg {
    @apply w-4 h-4;
  }
</style>
