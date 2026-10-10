<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ui, type RightSidebarPanel } from '#lib/stores/ui.svelte.js';
  import {
    player,
    removeFromQueue,
    clearQueue,
    moveUpcomingTrack,
    playQueueIndex
  } from '#lib/stores/player.svelte.js';
  import { useCreatePlaylistWithTracksMutation } from '#lib/queries/playlists.js';
  import { promptText } from '#lib/utils/dialog.svelte.js';
  import { validatePlaylistName, toSafeString } from '#lib/utils/validation.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import AlbumArt from './AlbumArt.svelte';
  import MarqueeText from './MarqueeText.svelte';
  import EqualizerPanel from './EqualizerPanel.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 「次に再生」の行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_QUEUE_ROW_HEIGHT = 50;

  // パネルを開く/切り替え
  function openPanel(panel: RightSidebarPanel) {
    if (ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === panel) {
      // 同じパネルをクリックした場合は閉じる
      ui.isRightSidebarExpanded = false;
    } else {
      // パネルを切り替えて開く
      ui.activeRightSidebarPanel = panel;
      ui.isRightSidebarExpanded = true;
    }
  }

  // 閉じる（バックドロップクリック時）
  function closeByBackdrop() {
    // 固定時はバックドロップクリックで閉じない
    if (ui.isRightSidebarPinned) return;
    ui.isRightSidebarExpanded = false;
  }

  // ピン状態をトグル
  function togglePin() {
    ui.isRightSidebarPinned = !ui.isRightSidebarPinned;
  }

  /**
   * 「次に再生」の曲から再生する（キューの並びは変えない）
   * @param upcomingIndex - 「次に再生」の一覧の中の位置
   */
  function playUpcoming(upcomingIndex: number) {
    playQueueIndex(player.currentTrackIndex + 1 + upcomingIndex);
  }

  // 行の中のボタン（キューから削除）の操作では再生しない
  function isRowButton(event: Event): boolean {
    return event.target instanceof Element && event.target.closest('button') !== null;
  }

  // ---- 「次に再生」の並べ替え（行のドラッグ） ----

  /** ドラッグ中の行の位置を運ぶデータの種類（アプリの中だけで使う） */
  const QUEUE_DRAG_TYPE = 'application/x-muspice-queue-index';

  /** ドラッグしている行の、「次に再生」の一覧の中の位置 */
  let draggedIndex = $state<number | null>(null);
  /** 落とそうとしている行の位置 */
  let dropIndex = $state<number | null>(null);

  const isQueueDrag = (event: DragEvent) =>
    event.dataTransfer?.types.includes(QUEUE_DRAG_TYPE) ?? false;

  function handleQueueDragStart(event: DragEvent, index: number) {
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(QUEUE_DRAG_TYPE, String(index));
    draggedIndex = index;
  }

  function handleQueueDragOver(event: DragEvent, index: number) {
    if (!isQueueDrag(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropIndex = index;
  }

  function handleQueueDrop(event: DragEvent, index: number) {
    if (!isQueueDrag(event)) return;
    event.preventDefault();
    const from = Number(event.dataTransfer?.getData(QUEUE_DRAG_TYPE));
    endQueueDrag();
    moveUpcomingTrack(from, index);
  }

  function endQueueDrag() {
    draggedIndex = null;
    dropIndex = null;
  }

  /**
   * 落とそうとしている行の、目印の出し方（動かした曲は、落とした行の位置に来る。
   * 下へ動かす時は行の下、上へ動かす時は行の上に線を出す）
   */
  function dropWhere(index: number): 'before' | 'after' | undefined {
    if (dropIndex !== index || draggedIndex === null || draggedIndex === index) return undefined;
    return draggedIndex < index ? 'after' : 'before';
  }

  // ---- 再生キューを、プレイリストとして保存 ----

  const savePlaylistMutation = useCreatePlaylistWithTracksMutation();

  async function handleSaveAsPlaylist() {
    // 再生し終えた曲を含む、キューの全体を保存する（同じ曲は、最初の1回だけが入る）
    const trackIds = player.playQueue.map((track) => track.id);
    if (trackIds.length === 0) return;

    // 日付は、プレイリスト名に使えない「/」を含まない形にする
    const today = new Date().toLocaleDateString('sv-SE');
    const name = await promptText({
      title: m.rightSidebar.saveAsPlaylistTitle,
      label: m.sidebar.playlistName,
      defaultValue: m.rightSidebar.saveAsPlaylistDefaultName(today),
      confirmLabel: m.rightSidebar.save,
      validate: (value) => validatePlaylistName(value).error ?? null
    });
    if (name === null) return;

    savePlaylistMutation.mutate({ name: toSafeString(name.trim(), 100), trackIds });
  }
</script>

<!-- アイコンバー（常に右端に固定表示） -->
<div class="icon-bar">
  <!-- 上部アイコン -->
  <div class="icon-bar-top">
    <!-- 再生キューボタン -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'queue'}
      onclick={() => openPanel('queue')}
      title={m.rightSidebar.queueTitle}
      aria-label={m.rightSidebar.openQueue}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M4 6h16M4 10h16M4 14h10M4 18h7" stroke-linecap="round" />
      </svg>
    </button>

    <!-- イコライザボタン -->
    <button
      class="icon-button"
      class:active={ui.isRightSidebarExpanded && ui.activeRightSidebarPanel === 'equalizer'}
      onclick={() => openPanel('equalizer')}
      title={m.rightSidebar.equalizerTitle}
      aria-label={m.rightSidebar.openEqualizer}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <line x1="4" y1="21" x2="4" y2="14" stroke-linecap="round" />
        <line x1="4" y1="10" x2="4" y2="3" stroke-linecap="round" />
        <line x1="12" y1="21" x2="12" y2="12" stroke-linecap="round" />
        <line x1="12" y1="8" x2="12" y2="3" stroke-linecap="round" />
        <line x1="20" y1="21" x2="20" y2="16" stroke-linecap="round" />
        <line x1="20" y1="12" x2="20" y2="3" stroke-linecap="round" />
        <line x1="1" y1="14" x2="7" y2="14" stroke-linecap="round" />
        <line x1="9" y1="8" x2="15" y2="8" stroke-linecap="round" />
        <line x1="17" y1="16" x2="23" y2="16" stroke-linecap="round" />
      </svg>
    </button>
  </div>

  <!-- 下部アイコン（ピンボタン） -->
  <div class="icon-bar-bottom">
    <button
      class="icon-button"
      class:active={ui.isRightSidebarPinned}
      onclick={togglePin}
      title={ui.isRightSidebarPinned ? m.rightSidebar.unpin : m.rightSidebar.pin}
      aria-label={ui.isRightSidebarPinned ? m.rightSidebar.unpin : m.rightSidebar.pin}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill={ui.isRightSidebarPinned ? 'currentColor' : 'none'}
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M12 17v5" stroke-linecap="round" />
        <path d="M5 17h14" stroke-linecap="round" />
        <path
          d="M7 11V7a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v4"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <path d="M7 11l-2 6h14l-2-6" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
  </div>
</div>

<!-- バックドロップ（固定時は表示しない） -->
{#if ui.isRightSidebarExpanded && !ui.isRightSidebarPinned}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={closeByBackdrop}></div>
{/if}

<!-- パネル（展開時のみ表示、固定時はアニメーションなし） -->
{#if ui.isRightSidebarExpanded}
  <aside
    class="side-panel"
    class:pinned={ui.isRightSidebarPinned}
    transition:fly={{ x: 200, duration: ui.isRightSidebarPinned ? 0 : 200 }}
  >
    {#if ui.activeRightSidebarPanel === 'queue'}
      <!-- 再生キューパネル -->
      <!-- ヘッダー -->
      <div class="queue-header">
        <h3>{m.rightSidebar.queue}</h3>
        <div class="queue-actions">
          {#if player.playQueue.length > 0}
            <button
              class="clear-btn icon"
              onclick={handleSaveAsPlaylist}
              disabled={savePlaylistMutation.isPending}
              title={m.rightSidebar.saveAsPlaylist}
              aria-label={m.rightSidebar.saveAsPlaylist}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="w-4 h-4"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M4 6h16M4 10h16M4 14h8m4 0v6m-3-3h6"
                />
              </svg>
            </button>
          {/if}
          {#if player.playQueue.length > 1}
            <button class="clear-btn" onclick={clearQueue}>{m.rightSidebar.clear}</button>
          {/if}
        </div>
      </div>

      <!-- 再生中 -->
      {#if player.currentTrack}
        <div class="now-playing">
          <div class="section-label">{m.rightSidebar.nowPlaying}</div>
          <div class="now-playing-track">
            <div class="queue-art">
              <AlbumArt
                src={albumArtUrl(player.currentTrack.id)}
                alt={m.common.albumArt}
                rounded="sm"
                placeholderType="music"
              />
            </div>
            <div class="track-details">
              <MarqueeText
                text={player.currentTrack.title || player.currentTrack.fileName}
                class="track-title"
              />
              <MarqueeText
                text={player.currentTrack.artist || m.common.unknownArtist}
                class="track-artist"
              />
            </div>
          </div>
        </div>
      {/if}

      <!-- 次に再生 -->
      <div class="upcoming-section">
        {#if player.upcomingTracks.length > 0}
          <div class="section-label">{m.rightSidebar.upNext(player.upcomingTracks.length)}</div>
          <!-- 一覧の曲をすべてキューに入れた場合は数万曲になるため、見えている行だけを描画する -->
          <div class="upcoming-list">
            <VirtualList
              items={player.upcomingTracks}
              getKey={(track) => track.id}
              estimatedRowHeight={ESTIMATED_QUEUE_ROW_HEIGHT}
              scrollerClass="px-2 pb-2"
            >
              {#snippet row(track, index)}
                <!-- 行のドラッグで、「次に再生」の中の順を入れ替えられる -->
                <div
                  class="queue-track"
                  class:dragging={draggedIndex === index}
                  data-drop={dropWhere(index)}
                  draggable="true"
                  ondragstart={(e) => handleQueueDragStart(e, index)}
                  ondragover={(e) => handleQueueDragOver(e, index)}
                  ondrop={(e) => handleQueueDrop(e, index)}
                  ondragend={endQueueDrag}
                  ondblclick={(e) => !isRowButton(e) && playUpcoming(index)}
                  onkeydown={(e) => e.key === 'Enter' && !isRowButton(e) && playUpcoming(index)}
                  role="button"
                  tabindex="0"
                >
                  <span class="track-number">{index + 1}</span>
                  <div class="queue-art">
                    <AlbumArt
                      src={albumArtUrl(track.id)}
                      alt={m.common.albumArt}
                      rounded="sm"
                      placeholderType="music"
                    />
                  </div>
                  <div class="track-details">
                    <MarqueeText text={track.title || track.fileName} class="track-title" />
                    <MarqueeText
                      text={track.artist || m.common.unknownArtist}
                      class="track-artist"
                    />
                  </div>
                  <button
                    class="remove-btn"
                    onclick={() => removeFromQueue(track.id)}
                    title={m.rightSidebar.removeFromQueue}
                  >
                    ✕
                  </button>
                </div>
              {/snippet}
            </VirtualList>
          </div>
        {:else if player.currentTrack}
          <div class="empty-queue">{m.rightSidebar.noUpcoming}</div>
        {:else}
          <div class="empty-queue">{m.player.noTrack}</div>
        {/if}
      </div>
    {:else if ui.activeRightSidebarPanel === 'equalizer'}
      <!-- イコライザパネル -->
      <EqualizerPanel />
    {/if}
  </aside>
{/if}

<style>
  @reference "../../app.css";

  /* アイコンバー（右端に固定） */
  .icon-bar {
    @apply fixed right-0 top-0 z-40 w-12 h-full
           flex flex-col items-center justify-between py-4
           bg-base-300 border-l border-border;
    padding-bottom: var(--spacing-player-height);
  }

  .icon-bar-top {
    @apply flex flex-col items-center gap-2;
  }

  .icon-bar-bottom {
    @apply flex flex-col items-center gap-2;
  }

  .icon-button {
    @apply w-10 h-10 flex items-center justify-center
           bg-transparent border-none rounded-md
           text-text-secondary cursor-pointer
           transition-all duration-200;
  }

  .icon-button:hover {
    @apply bg-surface-active text-text-primary;
  }

  .icon-button.active {
    @apply text-secondary;
  }

  /* バックドロップ（モーダル的に閉じる） */
  .backdrop {
    @apply fixed inset-0 z-20 bg-black/30;
  }

  /* サイドパネル（アイコンバーの左に表示） */
  .side-panel {
    @apply fixed top-0 z-30 h-full
           flex flex-col bg-base-100 border-l border-border;
    right: 3rem; /* アイコンバーの幅 */
    width: 17rem; /* 20rem - 3rem */
    padding-bottom: var(--spacing-player-height);
  }

  .queue-header {
    @apply flex items-center justify-between px-4 py-3 border-b border-border;
  }

  .queue-header h3 {
    @apply m-0 text-sm font-semibold text-text-primary;
  }

  .clear-btn {
    @apply px-2 py-1 bg-transparent border border-border-light rounded
           text-text-secondary text-xs cursor-pointer transition-all duration-200;
  }

  .clear-btn:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .clear-btn:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .clear-btn.icon {
    @apply flex items-center justify-center px-1.5;
  }

  .queue-actions {
    @apply flex items-center gap-1.5;
  }

  /* 再生中セクション */
  .now-playing {
    @apply px-4 py-3 bg-secondary/10 border-b border-border;
  }

  .section-label {
    @apply text-[0.625rem] font-semibold uppercase text-text-muted mb-1.5;
  }

  .now-playing-track {
    @apply flex items-center gap-2;
  }

  /* ジャケット画像（再生中・次に再生の各行） */
  .queue-art {
    @apply w-9 h-9 shrink-0 rounded-sm overflow-hidden;
  }

  :global(.track-title) {
    @apply text-[0.8rem] text-text-primary;
  }

  :global(.track-artist) {
    @apply text-[0.7rem] text-text-muted;
  }

  /* 次に再生セクション */
  .upcoming-section {
    @apply flex-1 flex flex-col overflow-hidden;
  }

  .upcoming-section .section-label {
    @apply pt-3 pb-1.5 px-4;
  }

  /* スクロールはVirtualListが行う */
  .upcoming-list {
    @apply flex-1 min-h-0;
  }

  .queue-track {
    @apply flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer select-none transition-colors duration-200;
  }

  .queue-track:hover {
    @apply bg-surface;
  }

  /* 並べ替えのドラッグ中の行と、落とす位置の線 */
  .queue-track.dragging {
    @apply opacity-50;
  }

  .queue-track[data-drop='before'] {
    box-shadow: inset 0 2px 0 var(--color-primary);
  }

  .queue-track[data-drop='after'] {
    box-shadow: inset 0 -2px 0 var(--color-primary);
  }

  .track-number {
    @apply text-xs text-text-dimmed w-5 text-center shrink-0;
  }

  .track-details {
    @apply flex-1 min-w-0 flex flex-col gap-0.5;
  }

  .remove-btn {
    @apply bg-transparent border-none text-text-dimmed text-xs cursor-pointer p-1
           opacity-0 transition-all duration-200 shrink-0;
  }

  .queue-track:hover .remove-btn {
    @apply opacity-100;
  }

  .remove-btn:hover {
    @apply text-error;
  }

  .empty-queue {
    @apply py-8 px-4 text-center text-text-dimmed text-sm;
  }
</style>
