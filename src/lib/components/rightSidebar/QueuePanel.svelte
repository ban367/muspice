<!--
  @component QueuePanel
  右サイドバーの再生キューのパネル。上部に再生中の曲を固定して出し、その下にキューの曲のリストを
  出す（再生し終えた曲も含めて、再生する順に並べ、再生中の曲の行を強調する）。
  リストの曲は、ダブルクリック・Enterで再生し、ドラッグで並べ替え、Delete / Backspaceでキューから外せる。
  キューの全体は、プレイリストとして保存できる。
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import {
    player,
    removeFromQueue,
    clearQueue,
    moveQueueEntry,
    playQueueIndex
  } from '#lib/stores/player.svelte.js';
  import { useCreatePlaylistWithTracksMutation } from '#lib/queries/playlists.js';
  import { confirmDestructive, promptText } from '#lib/utils/dialog.svelte.js';
  import { validatePlaylistName, toSafeString } from '#lib/utils/validation.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { openableListAction } from '#lib/utils/listNavigation.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import AlbumArt from '../AlbumArt.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import PlayingIndicator from '../library/PlayingIndicator.svelte';
  import SidePanel from './SidePanel.svelte';
  import SidePanelButton from './SidePanelButton.svelte';
  import SidePanelMessage from './SidePanelMessage.svelte';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelTrack from './SidePanelTrack.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // キューの行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_QUEUE_ROW_HEIGHT = 50;

  // 行の中のボタン（キューから削除）の操作では再生しない
  function isRowButton(event: Event): boolean {
    return event.target instanceof Element && event.target.closest('button') !== null;
  }

  let queueList = $state<VirtualList<(typeof player.playQueue)[number]>>();

  // ---- 再生中の曲の行 ----

  /** 再生中の曲の要素のID（何も再生していなければnull） */
  const playingEntryId = $derived(player.playQueue[player.currentTrackIndex]?.entryId ?? null);

  // パネルを開いた時と、再生中の曲が変わった時に、その行を見える位置へ出す。隠れていれば上端に
  // そろえ、この後に再生する曲も見せる（見えていれば動かさない。並べ替え・削除で位置だけが
  // 変わった時は、スクロールしない）
  $effect(() => {
    const entryId = playingEntryId;
    const list = queueList;
    if (entryId === null || !list) return;
    untrack(() => {
      // ドラッグ中は、落とす先の行を動かさない
      if (draggedEntryId === null) list.scrollToIndex(player.currentTrackIndex, 'start');
    });
  });

  // ---- キーボード操作 ----
  // 一覧（listbox）がフォーカスを受け、↑↓・Home・Endで現在位置を移し、Enterで再生し、
  // Delete / Backspaceでキューから削除する。現在位置も、位置ではなくキューの要素のIDで持つ

  /** 現在位置の行の要素のID */
  let activeEntryId = $state<number | null>(null);
  /** 現在位置の行の、キューの中の位置（ない場合は-1） */
  const activeIndex = $derived(
    activeEntryId === null
      ? -1
      : player.playQueue.findIndex((entry) => entry.entryId === activeEntryId)
  );

  function handleQueueKeydown(event: KeyboardEvent) {
    const entries = player.playQueue;
    if (
      (event.key === 'Delete' || event.key === 'Backspace') &&
      !event.defaultPrevented &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey
    ) {
      if (activeIndex === -1) return;
      event.preventDefault();
      // 現在位置は、消した行の次（最後の行なら前）の行へ移す
      const next = entries[activeIndex + 1] ?? entries[activeIndex - 1] ?? null;
      removeFromQueue(entries[activeIndex].entryId);
      activeEntryId = next?.entryId ?? null;
      return;
    }

    const action = openableListAction(event, activeIndex, entries.length, null);
    if (action === null) return;
    if (action.type === 'open') {
      playQueueIndex(action.index);
    } else {
      activeEntryId = entries[action.index].entryId;
      queueList?.scrollToIndex(action.index);
    }
  }

  // ---- 並べ替え（行のドラッグ） ----
  // 行は位置ではなくキューの要素のID（エントリID）で指す。ドラッグしている間に曲が進んで
  // 位置がずれても、ドラッグした曲を動かす

  /** ドラッグ中の行の要素のIDを運ぶデータの種類（アプリの中だけで使う） */
  const QUEUE_DRAG_TYPE = 'application/x-muspice-queue-entry';

  /** ドラッグしている行の要素のID */
  let draggedEntryId = $state<number | null>(null);
  /** 落とそうとしている行の要素のID */
  let dropEntryId = $state<number | null>(null);
  /** ドラッグしている行の、キューの中の今の位置（ない場合は-1） */
  const draggedIndex = $derived(
    draggedEntryId === null
      ? -1
      : player.playQueue.findIndex((entry) => entry.entryId === draggedEntryId)
  );

  const isQueueDrag = (event: DragEvent) =>
    event.dataTransfer?.types.includes(QUEUE_DRAG_TYPE) ?? false;

  function handleQueueDragStart(event: DragEvent, entryId: number) {
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(QUEUE_DRAG_TYPE, String(entryId));
    draggedEntryId = entryId;
  }

  function handleQueueDragOver(event: DragEvent, entryId: number) {
    if (!isQueueDrag(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropEntryId = entryId;
  }

  function handleQueueDrop(event: DragEvent, targetEntryId: number) {
    if (!isQueueDrag(event)) return;
    event.preventDefault();
    const entryId = Number(event.dataTransfer?.getData(QUEUE_DRAG_TYPE));
    endQueueDrag();
    moveQueueEntry(entryId, targetEntryId);
  }

  function endQueueDrag() {
    draggedEntryId = null;
    dropEntryId = null;
  }

  /**
   * 落とそうとしている行の、目印の出し方（動かした曲は、落とした行の位置に来る。
   * 下へ動かす時は行の下、上へ動かす時は行の上に線を出す）
   * @param entryId - 行の要素のID
   * @param index - 行の、キューの中の位置
   */
  function dropWhere(entryId: number, index: number): 'before' | 'after' | undefined {
    if (dropEntryId !== entryId || draggedIndex === -1 || draggedIndex === index) return undefined;
    return draggedIndex < index ? 'after' : 'before';
  }

  // 再生キューをクリアする（再生中の曲も外して止めるため、確かめてから）
  async function handleClearQueue() {
    if (!(await confirmDestructive(m.rightSidebar.clearConfirm, m.rightSidebar.clear))) return;
    clearQueue();
  }

  // ---- 再生キューを、プレイリストとして保存 ----

  const savePlaylistMutation = useCreatePlaylistWithTracksMutation();

  async function handleSaveAsPlaylist() {
    // 再生し終えた曲を含む、キューの全体を保存する（同じ曲は、最初の1回だけが入る）
    const trackIds = player.playQueue.map((entry) => entry.track.id);
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

<SidePanel title={m.rightSidebar.queueTitle} scroll={false}>
  {#snippet actions()}
    <!-- キューが空の間も、ボタンは消さずに使えなくする（見出しの並びを変えない） -->
    <SidePanelButton
      label={m.rightSidebar.saveAsPlaylist}
      onclick={handleSaveAsPlaylist}
      disabled={player.playQueue.length === 0 || savePlaylistMutation.isPending}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M4 6h16M4 10h16M4 14h8m4 0v6m-3-3h6" />
      </svg>
    </SidePanelButton>
    <SidePanelButton
      label={m.rightSidebar.clearQueue}
      onclick={handleClearQueue}
      disabled={player.playQueue.length === 0}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6" />
      </svg>
    </SidePanelButton>
  {/snippet}

  {#snippet top()}
    <SidePanelTrack
      label={m.rightSidebar.nowPlaying}
      track={player.currentTrack}
      emptyText={m.rightSidebar.nothingPlaying}
    />
  {/snippet}

  {#if player.playQueue.length > 0}
    <SidePanelSection title={m.rightSidebar.queueTracks(player.playQueue.length)} flush fill>
      <!-- 一覧の曲をすべてキューに入れた場合は数万曲になるため、見えている行だけを描画する -->
      <!-- 同じ曲が何度も入ることがあるため、キーは曲のIDではなく要素のID -->
      <VirtualList
        bind:this={queueList}
        items={player.playQueue}
        getKey={(entry) => entry.entryId}
        estimatedRowHeight={ESTIMATED_QUEUE_ROW_HEIGHT}
        scrollerClass="px-2 pb-2"
        class="queue-listbox"
        role="listbox"
        aria-label={m.rightSidebar.queueTracks(player.playQueue.length)}
        tabindex={0}
        onkeydown={handleQueueKeydown}
      >
        {#snippet row({ entryId, track }, index)}
          {@const isPlaying = entryId === playingEntryId}
          <!-- 行のドラッグで、キューの中の順を入れ替えられる -->
          <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
          <div
            class="queue-track"
            class:active={activeEntryId === entryId}
            class:playing={isPlaying}
            class:dragging={draggedEntryId === entryId}
            data-drop={dropWhere(entryId, index)}
            draggable="true"
            ondragstart={(e) => handleQueueDragStart(e, entryId)}
            ondragover={(e) => handleQueueDragOver(e, entryId)}
            ondrop={(e) => handleQueueDrop(e, entryId)}
            ondragend={endQueueDrag}
            onclick={(e) => !isRowButton(e) && (activeEntryId = entryId)}
            ondblclick={(e) => !isRowButton(e) && playQueueIndex(index)}
            role="option"
            aria-selected={activeEntryId === entryId}
            aria-current={isPlaying ? 'true' : undefined}
          >
            <span class="track-number">
              {#if isPlaying}
                <PlayingIndicator size="small" />
              {:else}
                {index + 1}
              {/if}
            </span>
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
              <MarqueeText text={track.artist || m.common.unknownArtist} class="track-artist" />
            </div>
            <button
              class="remove-btn"
              onclick={() => removeFromQueue(entryId)}
              title={m.rightSidebar.removeFromQueue}
              aria-label={m.rightSidebar.removeFromQueue}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="w-3 h-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                aria-hidden="true"
              >
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
        {/snippet}
      </VirtualList>
    </SidePanelSection>
  {:else}
    <SidePanelMessage text={m.player.noTrack} />
  {/if}
</SidePanel>

<style>
  @reference "../../../app.css";

  .queue-track {
    @apply flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer select-none transition-colors duration-200;
  }

  .queue-track:hover {
    @apply bg-surface;
  }

  /* 現在位置の行（クリック・矢印キーで移す）。一覧にフォーカスがある間は、枠でも示す */
  .queue-track.active {
    @apply bg-surface-active;
  }

  :global(.queue-listbox) {
    @apply outline-none;
  }

  :global(.queue-listbox:focus-visible) .queue-track.active {
    box-shadow: inset 0 0 0 2px var(--color-primary);
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
    @apply flex justify-center text-xs text-text-dimmed w-5 shrink-0;
  }

  /* ジャケット画像（キューの各行） */
  .queue-art {
    @apply w-9 h-9 shrink-0 rounded-sm overflow-hidden;
  }

  .track-details {
    @apply flex-1 min-w-0 flex flex-col gap-0.5;
  }

  /* 曲名・アーティスト（`MarqueeText`に渡したクラス）。ほかの画面の同じ名前のクラスに当たらないよう、
     キューの行の中に限る（限らないと、読み込んだ順で、ほかの画面の大きさに上書きされる） */
  .queue-track :global(.track-title) {
    @apply text-[0.8rem] text-text-primary;
  }

  .queue-track :global(.track-artist) {
    @apply text-[0.7rem] text-text-muted;
  }

  /* 再生中の曲の行（番号の代わりに、再生中の印を出す。色はほかの曲の一覧と同じ） */
  .queue-track.playing {
    @apply bg-secondary/15;
  }

  .queue-track.playing .track-number,
  .queue-track.playing :global(.track-title) {
    @apply text-secondary;
  }

  .remove-btn {
    @apply bg-transparent border-none text-text-dimmed text-xs cursor-pointer p-1
           opacity-0 transition-all duration-200 shrink-0;
  }

  /* ポインタを置いた行・現在位置の行・キーボードでフォーカスした時に出す */
  .queue-track:hover .remove-btn,
  .queue-track.active .remove-btn,
  .remove-btn:focus-visible {
    @apply opacity-100;
  }

  .remove-btn:hover {
    @apply text-error;
  }
</style>
