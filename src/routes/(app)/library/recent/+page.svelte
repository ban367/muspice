<!--
  最近追加した曲の画面。追加した日時の新しい順に、アルバムのカードを並べる
  （アルバムのない曲は、1曲で1枚のカード）。項目は、取得済みの全曲の一覧から計算する。

  - アルバムのカード: クリックで曲の一覧を開く。ダブルクリック・再生ボタンでアルバムを再生する
  - 曲のカード: ダブルクリック・再生ボタンで、その曲を再生する
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import type { AlbumSummary } from '#lib/types/models.js';
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import { playSingleTrack } from '#lib/stores/player.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDate } from '#lib/utils/format.js';
  import { playGroup } from '#lib/utils/groupPlayback.js';
  import { recentlyAdded, type RecentItem } from '#lib/utils/libraryViews.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import AlbumArt from '#lib/components/AlbumArt.svelte';
  import ContextMenu from '#lib/components/ContextMenu.svelte';
  import GroupContextMenu from '#lib/components/GroupContextMenu.svelte';
  import MarqueeText from '#lib/components/MarqueeText.svelte';
  import CardSizeSlider from '#lib/components/library/CardSizeSlider.svelte';
  import GroupDetail from '#lib/components/library/GroupDetail.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // カードの間隔（px）と、アルバムアートを除いた高さの見積もり（描画した後は、実測した高さを使う）
  const GRID_GAP = 12;
  const ESTIMATED_CARD_EXTRA_HEIGHT = 92;

  const queryClient = useQueryClient();
  const tracksQuery = useTracksQuery();
  const items = $derived(tracksQuery.data ? recentlyAdded(tracksQuery.data) : []);
  const cardWidth = $derived(ui.gridCardSize + 16);

  /** アルバムの項目を、アルバムの一覧の1件と同じ形にする（曲の一覧・メニューに渡す） */
  const toAlbum = (item: RecentItem): AlbumSummary => ({
    name: item.name,
    sortName: null,
    artist: item.artist,
    trackCount: item.trackCount,
    totalDuration: item.totalDuration,
    representativeTrackId: item.representativeTrackId
  });

  // 開いているアルバム（キーで持ち、取り直した一覧から引く）
  let openKey = $state<string | null>(null);
  const openAlbum = $derived.by(() => {
    const item = items.find((candidate) => candidate.key === openKey);
    return item?.kind === 'album' ? toAlbum(item) : null;
  });

  let contextMenu = $state<{ x: number; y: number; item: RecentItem } | null>(null);

  function play(item: RecentItem) {
    if (item.track) playSingleTrack(item.track);
    else void playGroup(queryClient, 'album', item.name, item.artist);
  }

  function handleClick(item: RecentItem) {
    if (item.kind === 'album') openKey = item.key;
  }

  function handleContextMenu(event: MouseEvent, item: RecentItem) {
    event.preventDefault();
    contextMenu = { x: event.clientX, y: event.clientY, item };
  }
</script>

<div class="recent-page">
  <div class="page-header">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="header-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
    <h1 class="page-title">{m.library.recentlyAdded}</h1>
    <div class="ml-auto">
      <CardSizeSlider />
    </div>
  </div>

  <div class="grid-container">
    {#if tracksQuery.isLoading}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>{m.common.loading}</p>
      </div>
    {:else if tracksQuery.isError}
      <div class="empty-state text-error">
        <p>{m.common.errorOccurred}</p>
        <p class="text-sm text-text-muted">
          {tracksQuery.error?.message || m.common.unknownError}
        </p>
      </div>
    {:else if items.length === 0}
      <div class="empty-state">
        <p>{m.library.emptyLibrary}</p>
        <p class="text-sm text-text-dimmed">{m.library.emptyLibraryHint}</p>
      </div>
    {:else}
      <VirtualList
        {items}
        getKey={(item) => item.key}
        estimatedRowHeight={ui.gridCardSize + ESTIMATED_CARD_EXTRA_HEIGHT}
        minColumnWidth={cardWidth}
        gap={GRID_GAP}
        overscan={2}
        class="justify-items-center"
        aria-label={m.library.recentlyAdded}
      >
        {#snippet row(item)}
          <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
          <div
            class="grid-card flex flex-col items-center"
            onclick={() => handleClick(item)}
            ondblclick={() => play(item)}
            oncontextmenu={(event) => handleContextMenu(event, item)}
          >
            <div
              class="grid-card-art"
              style="width: {ui.gridCardSize}px; height: {ui.gridCardSize}px;"
            >
              <AlbumArt
                src={albumArtUrl(item.representativeTrackId)}
                alt={item.name}
                placeholderType={item.kind === 'album' ? 'disc' : 'music'}
              />
              <div class="play-overlay">
                <button
                  class="play-button-circle"
                  onclick={(event) => {
                    event.stopPropagation();
                    play(item);
                  }}
                  title={item.kind === 'album' ? m.library.playAlbum : m.common.play}
                >
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                </button>
              </div>
            </div>
            <div class="min-w-0 w-full text-center">
              <MarqueeText
                text={item.name}
                class="text-[0.9375rem] font-semibold text-text-primary m-0"
              />
              <MarqueeText
                text={item.artist || m.common.unknownArtist}
                class="text-[0.8125rem] text-text-muted mt-1 m-0"
              />
              <p class="added-at">{formatDate(item.addedAt)}</p>
            </div>
          </div>
        {/snippet}
      </VirtualList>
    {/if}
  </div>
</div>

<GroupDetail group={openAlbum} type="album" onClose={() => (openKey = null)} />

{#if contextMenu?.item.kind === 'album'}
  <GroupContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    group={toAlbum(contextMenu.item)}
    type="album"
    onClose={() => (contextMenu = null)}
  />
{:else if contextMenu?.item.track}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    track={contextMenu.item.track}
    onClose={() => (contextMenu = null)}
  />
{/if}

<style>
  @reference "../../../../app.css";

  .recent-page {
    @apply flex flex-col h-full;
  }

  .page-header {
    @apply flex items-center gap-3 p-4 border-b border-border;
  }

  .header-icon {
    @apply w-6 h-6 text-primary;
  }

  .page-title {
    @apply text-xl font-bold text-text-primary m-0;
  }

  .grid-container {
    @apply flex-1 min-h-0 p-4;
  }

  .added-at {
    @apply m-0 mt-1 text-xs text-text-dimmed;
  }
</style>
