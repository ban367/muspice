<!--
  @component ArtistGrid
  アーティスト一覧のグリッド/リスト表示コンポーネント。
  LibraryGridを使用して共通ロジックを委譲し、アーティスト固有の表示をSnippetで実装。
-->
<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import { useArtistsGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize } from '$lib/stores/ui';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import LibraryGrid from './LibraryGrid.svelte';
  import GroupDetail from './GroupDetail.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

  // Props
  interface Props {
    displayMode?: 'grid' | 'list';
  }

  let { displayMode = 'grid' }: Props = $props();

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const isLoading = $derived(artistsQuery.isLoading);
  const isError = $derived(artistsQuery.isError);
  const allArtists = $derived(artistsQuery.data ?? []);

  // 選択中のアーティスト（モーダル表示用）
  let selectedArtist = $state<ArtistGroup | null>(null);

  // LibraryGridコンポーネントの参照
  let libraryGrid: LibraryGrid<ArtistGroup>;

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // カードサイズの計算
  const cardWidth = $derived($gridCardSize + 16);

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }

  // アーティストカードが表示されたらアートを読み込み
  function handleArtistVisible(artist: ArtistGroup) {
    if (artist.representativeTrackId) {
      loadAlbumArt(artist.representativeTrackId);
    }
  }

  // アーティストをクリック
  function handleArtistClick(artist: ArtistGroup) {
    selectedArtist = artist;
  }

  // アーティストをダブルクリック（すべて再生）
  function handleArtistDoubleClick(artist: ArtistGroup) {
    const allTracks = artist.albums.flatMap((album) => album.tracks);
    if (allTracks.length > 0) {
      playTrackFromQueue(allTracks, 0);
    }
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, artist: ArtistGroup) {
    event.stopPropagation();
    handleArtistDoubleClick(artist);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedArtist = null;
  }

  // 検索フィルター
  function filterArtist(artist: ArtistGroup, query: string): boolean {
    return artist.name.toLowerCase().includes(query);
  }

  // Intersection Observer アクション
  function intersectionObserver(node: HTMLElement, options: { callback: () => void }) {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            options.callback();
            observer.unobserve(node);
          }
        });
      },
      { rootMargin: '100px' }
    );

    observer.observe(node);

    return {
      destroy() {
        observer.disconnect();
      }
    };
  }
</script>

<LibraryGrid
  bind:this={libraryGrid}
  items={allArtists}
  {isLoading}
  {isError}
  {displayMode}
  itemLabel="アーティスト"
  emptyMessage="アーティストがいません"
  emptyHint="音楽をインポートしてアーティストを追加してください"
  filterFn={filterArtist}
  gridStyle="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;"
  gridClass="artist-grid"
  groupType="artist"
>
  {#snippet emptyIcon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
    >
      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </svg>
  {/snippet}

  {#snippet gridCard(artist)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="grid-card artist-card"
      onclick={() => handleArtistClick(artist)}
      ondblclick={() => handleArtistDoubleClick(artist)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, artist)}
      use:intersectionObserver={{ callback: () => handleArtistVisible(artist) }}
    >
      <div class="artist-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
        <AlbumArt
          src={getArt(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
        <div class="play-overlay rounded-full">
          <button
            class="play-button-circle"
            onclick={(e) => handlePlayClick(e, artist)}
            title="アーティストを再生"
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
          </button>
        </div>
      </div>
      <div class="text-center min-w-0 w-full">
        <MarqueeText
          text={artist.name}
          class="text-[0.9375rem] font-semibold text-text-primary m-0"
        />
        <p class="text-xs text-text-dimmed mt-1.5 m-0">
          {artist.albumCount}アルバム · {artist.trackCount}曲
        </p>
      </div>
    </div>
  {/snippet}

  {#snippet listRow(artist)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="list-row"
      onclick={() => handleArtistClick(artist)}
      ondblclick={() => handleArtistDoubleClick(artist)}
      oncontextmenu={(e) => libraryGrid.handleContextMenu(e, artist)}
      use:intersectionObserver={{ callback: () => handleArtistVisible(artist) }}
    >
      <div class="list-art artist-list-art">
        <AlbumArt
          src={getArt(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
      </div>
      <div class="list-info">
        <MarqueeText text={artist.name} class="list-title" />
        <span class="list-artist">{artist.albumCount}アルバム · {artist.trackCount}曲</span>
      </div>
      <button
        class="list-play-btn"
        onclick={(e) => handlePlayClick(e, artist)}
        title="アーティストを再生"
      >
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
    </div>
  {/snippet}

  {#snippet footer()}
    <GroupDetail group={selectedArtist} type="artist" onClose={handleCloseDetail} />
  {/snippet}
</LibraryGrid>

<style>
  @reference "../../../app.css";
  :global(.artist-grid) {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width), 1fr));
    gap: 0.75rem;
  }

  .artist-card {
    @apply flex flex-col items-center;
  }

  .artist-art {
    @apply relative aspect-square rounded-full overflow-hidden mb-3;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .list-row {
    @apply grid gap-3 px-3 py-2.5 items-center rounded-md cursor-pointer transition-colors duration-100;
    grid-template-columns: 3rem 1fr 2.5rem;
  }

  .list-row:hover {
    @apply bg-surface-hover;
  }

  .list-art {
    @apply w-12 h-12 rounded overflow-hidden shrink-0;
  }

  .artist-list-art {
    @apply rounded-full;
  }

  .list-info {
    @apply flex flex-col gap-0.5 min-w-0;
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
