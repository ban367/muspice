<!--
  最近追加した曲の画面。追加した日時の新しい順に、アルバムのカードを並べる
  （アルバムのない曲は、1曲で1枚のカード）。項目は、取得済みの全曲の一覧から計算する。

  - アルバムのカード: クリック・Enterで、右サイドバーの情報パネルに曲を表示する。ダブルクリック・再生ボタンでアルバムを再生する
  - 曲のカード（アルバムのない曲）: クリックで、情報パネルにその曲を表示する。ダブルクリック・Enter・再生ボタンで、その曲を再生する
  - キーボード: ほかのグリッド（`LibraryGrid`）と同じく、矢印キー・Home・Endで現在位置を移す
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import type { AlbumSummary } from '#lib/types/models.js';
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import { playSingleTrack } from '#lib/stores/player.svelte.js';
  import { GRID_CARD_GAP, trackInfoTarget, ui, type InfoTarget } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDate } from '#lib/utils/format.js';
  import { playGroup } from '#lib/utils/groupPlayback.js';
  import { recentlyAdded, type RecentItem } from '#lib/utils/libraryViews.js';
  import { openableListAction } from '#lib/utils/listNavigation.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import AlbumArt from '#lib/components/AlbumArt.svelte';
  import ContextMenu from '#lib/components/ContextMenu.svelte';
  import GroupContextMenu from '#lib/components/GroupContextMenu.svelte';
  import MarqueeText from '#lib/components/MarqueeText.svelte';
  import CardSizeSlider from '#lib/components/library/CardSizeSlider.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // カードの、アルバムアートを除いた高さの見積もり（描画した後は、実測した高さを使う）
  const ESTIMATED_CARD_EXTRA_HEIGHT = 68;

  const queryClient = useQueryClient();
  const tracksQuery = useTracksQuery();
  const items = $derived(tracksQuery.data ? recentlyAdded(tracksQuery.data) : []);

  /** アルバムの項目を、アルバムの一覧の1件と同じ形にする（情報パネル・メニューに渡す） */
  const toAlbum = (item: RecentItem): AlbumSummary => ({
    name: item.name,
    sortName: null,
    artist: item.artist,
    trackCount: item.trackCount,
    totalDuration: item.totalDuration,
    representativeTrackId: item.representativeTrackId
  });

  let contextMenu = $state<{ x: number; y: number; item: RecentItem } | null>(null);

  function play(item: RecentItem) {
    if (item.track) playSingleTrack(item.track);
    else void playGroup(queryClient, 'album', item.name, item.artist);
  }

  /** 項目を、情報パネルに表示する項目にする（アルバム。曲の項目は、アルバムのない曲そのもの） */
  const toInfoTarget = (item: RecentItem): InfoTarget =>
    item.track ? trackInfoTarget(item.track) : { type: 'album', album: toAlbum(item) };

  function handleClick(item: RecentItem) {
    ui.showInfo(toInfoTarget(item));
  }

  // 見えている項目だけを描画する一覧
  let virtualList = $state<VirtualList<RecentItem>>();

  // キーボードで移動する時の現在位置（項目のキー。クリックした項目もここに入る）
  let activeKey = $state<string | null>(null);

  /**
   * 一覧のキーボード操作（`LibraryGrid`と同じ）
   *
   * 矢印キー・Home・Endで現在位置を移し、Enterで開く。アルバムはクリックと同じく情報パネルに表示し、
   * 曲は（開く先がないため）曲の一覧のEnterと同じく再生する。
   * 移した時は、情報パネルを開いている間だけ、表示する項目を移した先に入れ替える。
   */
  function handleKeydown(event: KeyboardEvent) {
    const current = items.findIndex((item) => item.key === activeKey);
    const columns = virtualList?.getColumns() ?? 1;
    const action = openableListAction(event, current, items.length, columns);
    if (action === null) return;

    const item = items[action.index];
    if (action.type === 'open') {
      if (item.kind === 'album') handleClick(item);
      else play(item);
      return;
    }
    // 既定の動作（一覧のスクロール）は止めてあるため、移動先の項目を見える位置へ出す
    activeKey = item.key;
    virtualList?.scrollToIndex(action.index);
    ui.followInfo(toInfoTarget(item));
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
      <!-- 一覧がフォーカスを受けてキー操作を扱い、現在位置のカードを枠で示す -->
      <VirtualList
        bind:this={virtualList}
        {items}
        getKey={(item) => item.key}
        estimatedRowHeight={ui.gridCardSize + ESTIMATED_CARD_EXTRA_HEIGHT}
        minColumnWidth={ui.gridCardWidth}
        gap={GRID_CARD_GAP}
        overscan={2}
        scrollerClass="p-3"
        class="outline-none justify-items-center"
        role="listbox"
        aria-label={m.library.recentlyAdded}
        tabindex={0}
        onkeydown={handleKeydown}
      >
        {#snippet row(item)}
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div
            class="grid-item"
            class:active={item.key === activeKey}
            role="option"
            aria-selected={item.key === activeKey}
            onpointerdown={() => (activeKey = item.key)}
          >
            <!-- キー操作は一覧（listbox）で受けるため、カードはフォーカスを受けない -->
            <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
            <!-- 幅を決めておく（決めないと、長い名前の幅までカードが広がり、隣の列に重なる） -->
            <div
              class="grid-card"
              style="width: {ui.gridCardWidth}px;"
              onclick={() => handleClick(item)}
              ondblclick={() => play(item)}
              oncontextmenu={(event) => handleContextMenu(event, item)}
            >
              <div class="grid-card-art">
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
              <div class="text-center">
                <MarqueeText text={item.name} class="grid-card-title" />
                <MarqueeText
                  text={item.artist || m.common.unknownArtist}
                  class="grid-card-subtitle"
                />
                <p class="added-at">{formatDate(item.addedAt)}</p>
              </div>
            </div>
          </div>
        {/snippet}
      </VirtualList>
    {/if}
  </div>
</div>

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

  /* 余白は、スクロールする領域（VirtualList）の中に付ける（カードの影が端で切れないように） */
  .grid-container {
    @apply flex-1 min-h-0;
  }

  .added-at {
    @apply m-0 mt-0.5 text-[0.6875rem] leading-tight text-text-dimmed;
  }
</style>
