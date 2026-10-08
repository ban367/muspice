<!--
  @component ArtistGrid
  アーティスト一覧のグリッド/リスト表示コンポーネント。
  LibraryGridを使用して共通ロジックを委譲し、アーティスト固有の表示をSnippetで実装。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import type { ArtistSummary } from '#lib/types/models.js';
  import { useArtistsQuery } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
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
  const artistsQuery = useArtistsQuery();
  const isLoading = $derived(artistsQuery.isLoading);
  const isError = $derived(artistsQuery.isError);
  const allArtists = $derived(artistsQuery.data ?? []);

  // 選択中のアーティスト（モーダル表示用）
  // 名前で持ち、取り直したデータから引く（曲数などの変更がモーダルの中にも反映される）
  let selectedArtistName = $state<string | null>(null);
  const selectedArtist = $derived(
    selectedArtistName === null
      ? null
      : (allArtists.find((artist) => artist.name === selectedArtistName) ?? null)
  );

  // LibraryGridコンポーネントの参照
  let libraryGrid: LibraryGrid<ArtistSummary>;

  // カードサイズの計算
  const cardWidth = $derived(ui.gridCardSize + 16);
  // カードの、アーティストの画像を除いた高さの見積もり（描画した後は、実測した高さを使う）
  const ESTIMATED_CARD_EXTRA_HEIGHT = 72;
  // リスト表示の行の高さの見積もり
  const ESTIMATED_LIST_ROW_HEIGHT = 68;

  // アーティストをクリック
  function handleArtistClick(artist: ArtistSummary) {
    selectedArtistName = artist.name;
  }

  // アーティストをダブルクリック（すべて再生）
  function handleArtistDoubleClick(artist: ArtistSummary) {
    void playGroup(queryClient, 'artist', artist.name);
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, artist: ArtistSummary) {
    event.stopPropagation();
    handleArtistDoubleClick(artist);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedArtistName = null;
  }

  // 絞り込みの対象（アーティスト名）
  const artistSearchFields = (artist: ArtistSummary) => [artist.name];
</script>

<LibraryGrid
  bind:this={libraryGrid}
  items={allArtists}
  {isLoading}
  {isError}
  {displayMode}
  itemLabel={m.library.artists}
  emptyMessage={m.library.noArtists}
  emptyHint={m.library.noArtistsHint}
  searchFields={artistSearchFields}
  minCardWidth={cardWidth}
  estimatedCardHeight={ui.gridCardSize + ESTIMATED_CARD_EXTRA_HEIGHT}
  estimatedRowHeight={ESTIMATED_LIST_ROW_HEIGHT}
  groupType="artist"
  onOpen={handleArtistClick}
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
    >
      <div class="artist-art" style="width: {ui.gridCardSize}px; height: {ui.gridCardSize}px;">
        <AlbumArt
          src={albumArtUrl(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
        <div class="play-overlay rounded-full">
          <button
            class="play-button-circle"
            onclick={(e) => handlePlayClick(e, artist)}
            title={m.library.playArtist}
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
          {m.library.albumsAndTracks(artist.albumCount, artist.trackCount)}
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
    >
      <div class="list-art artist-list-art">
        <AlbumArt
          src={albumArtUrl(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
      </div>
      <div class="list-info">
        <MarqueeText text={artist.name} class="list-title" />
        <span class="list-artist"
          >{m.library.albumsAndTracks(artist.albumCount, artist.trackCount)}</span
        >
      </div>
      <button
        class="list-play-btn"
        onclick={(e) => handlePlayClick(e, artist)}
        title={m.library.playArtist}
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
