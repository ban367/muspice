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
  import { formatDuration } from '#lib/utils/format.js';
  import { playGroup } from '#lib/utils/groupPlayback.js';
  import LibraryGrid from './LibraryGrid.svelte';
  import GroupDetail from './GroupDetail.svelte';
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

  // 選択中のアルバム（モーダル表示用）
  // 名前で持ち、取り直したデータから引く（曲数などの変更がモーダルの中にも反映される）
  let selectedAlbumName = $state<string | null>(null);
  const selectedAlbum = $derived(
    selectedAlbumName === null
      ? null
      : (allAlbums.find((album) => album.name === selectedAlbumName) ?? null)
  );

  // LibraryGridコンポーネントの参照
  let libraryGrid: LibraryGrid<AlbumSummary>;

  // カードサイズの計算
  const cardWidth = $derived(ui.gridCardSize + 16);
  // カードの、アルバムアートを除いた高さの見積もり（描画した後は、実測した高さを使う）
  const ESTIMATED_CARD_EXTRA_HEIGHT = 74;
  // リスト表示の行の高さの見積もり
  const ESTIMATED_LIST_ROW_HEIGHT = 68;

  // アルバムをクリック
  function handleAlbumClick(album: AlbumSummary) {
    selectedAlbumName = album.name;
  }

  // アルバムをダブルクリック（すべて再生）
  function handleAlbumDoubleClick(album: AlbumSummary) {
    void playGroup(queryClient, 'album', album.name);
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, album: AlbumSummary) {
    event.stopPropagation();
    handleAlbumDoubleClick(album);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedAlbumName = null;
  }

  // 検索フィルター
  function filterAlbum(album: AlbumSummary, query: string): boolean {
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
  itemLabel={m.library.albums}
  emptyMessage={m.library.noAlbums}
  emptyHint={m.library.noAlbumsHint}
  filterFn={filterAlbum}
  minCardWidth={cardWidth}
  estimatedCardHeight={ui.gridCardSize + ESTIMATED_CARD_EXTRA_HEIGHT}
  estimatedRowHeight={ESTIMATED_LIST_ROW_HEIGHT}
  groupType="album"
  onOpen={handleAlbumClick}
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
    >
      <div class="grid-card-art" style="width: {ui.gridCardSize}px; height: {ui.gridCardSize}px;">
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
      <div class="min-w-0 w-full text-center">
        <MarqueeText
          text={album.name}
          class="text-[0.9375rem] font-semibold text-text-primary m-0"
        />
        <MarqueeText
          text={album.artist || m.common.unknownArtist}
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

  {#snippet footer()}
    <GroupDetail group={selectedAlbum} type="album" onClose={handleCloseDetail} />
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
