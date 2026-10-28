<!--
  @component AlbumGrid
  アルバム一覧のグリッド/リスト表示コンポーネント。
  LibraryGridを使用して共通ロジックを委譲し、アルバム固有の表示をSnippetで実装。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import type { AlbumSummary } from '#lib/types/models.js';
  import { useAlbumsQuery } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { albumSearchFields } from '#lib/utils/albumArtists.js';
  import { albumKey } from '#lib/utils/albumKey.js';
  import { formatDuration } from '#lib/utils/format.js';
  import { playGroup } from '#lib/utils/groupPlayback.js';
  import LibraryGrid from './LibraryGrid.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    displayMode?: 'grid' | 'list';
  }

  let { displayMode = 'grid' }: Props = $props();

  // クエリ
  const queryClient = useQueryClient();
  const albumsQuery = useAlbumsQuery();
  const isLoading = $derived(albumsQuery.isLoading);
  const isError = $derived(albumsQuery.isError);
  const allAlbums = $derived(albumsQuery.data ?? []);

  // LibraryGridコンポーネントの参照
  let libraryGrid: LibraryGrid<AlbumSummary>;

  // カードの、アルバムアートを除いた高さの見積もり（描画した後は、実測した高さを使う）
  const ESTIMATED_CARD_EXTRA_HEIGHT = 52;
  // リスト表示の行の高さの見積もり
  const ESTIMATED_LIST_ROW_HEIGHT = 68;

  // アルバムを再生する（ダブルクリック・Enter）
  function playAlbum(album: AlbumSummary) {
    void playGroup(queryClient, 'album', album.name, album.artist);
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, album: AlbumSummary) {
    event.stopPropagation();
    playAlbum(album);
  }
</script>

<LibraryGrid
  bind:this={libraryGrid}
  items={allAlbums}
  {isLoading}
  {isError}
  {displayMode}
  itemLabel={m.library.albums}
  emptyMessage={m.library.noAlbums}
  emptyHint={m.library.noAlbumsHint}
  searchFields={albumSearchFields}
  getKey={albumKey}
  minCardWidth={ui.gridCardWidth}
  centerCards
  estimatedCardHeight={ui.gridCardSize + ESTIMATED_CARD_EXTRA_HEIGHT}
  estimatedRowHeight={ESTIMATED_LIST_ROW_HEIGHT}
  groupType="album"
  onOpen={playAlbum}
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
    <div
      class="grid-card"
      style="width: {ui.gridCardWidth}px;"
      ondblclick={() => playAlbum(album)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, album)}
    >
      <div class="grid-card-art">
        <AlbumArt src={albumArtUrl(album.representativeTrackId)} alt={album.name} />
        <div class="play-overlay">
          <button
            class="play-button-circle"
            onclick={(e) => handlePlayClick(e, album)}
            title={m.library.playAlbum}
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
          </button>
        </div>
      </div>
      <div class="text-center">
        <MarqueeText text={album.name} class="grid-card-title" />
        <MarqueeText text={album.artist || m.common.unknownArtist} class="grid-card-subtitle" />
      </div>
    </div>
  {/snippet}

  {#snippet listRow(album)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="list-row"
      ondblclick={() => playAlbum(album)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, album)}
    >
      <div class="list-art">
        <AlbumArt src={albumArtUrl(album.representativeTrackId)} alt={album.name} />
      </div>
      <div class="list-info">
        <MarqueeText text={album.name} class="list-title" />
        <MarqueeText text={album.artist || m.common.unknownArtist} class="list-artist" />
      </div>
      <div class="list-meta">
        <span>{m.common.trackCount(album.trackCount)}</span>
      </div>
      <div class="list-duration">
        {formatDuration(album.totalDuration)}
      </div>
      <button
        class="list-play-btn"
        onclick={(e) => handlePlayClick(e, album)}
        title={m.library.playAlbum}
      >
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
    </div>
  {/snippet}
</LibraryGrid>

<style>
  @reference "../../../app.css";
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
