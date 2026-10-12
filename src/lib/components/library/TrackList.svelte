<!--
  @component TrackList
  曲の一覧（リスト表示・グリッド表示）。

  - リスト表示の列は、見出しの右クリックで選び、見出しのドラッグで並べ替え、境目のドラッグで幅を変える
  - 見出しのクリックで、その列で並べ替える（どの列でも並べ替えられる）
  - 列と並び順は、`viewId`ごとに覚える（`#lib/stores/trackListView.svelte`）。幅は、どの画面でも共通
  - `onReorder`を渡した一覧（プレイリスト）は、渡した順のまま並べている間、行のドラッグで並べ替えられる
  - 再生中の曲へのジャンプ（`ui.revealTrackId`）を受けて、その曲の行までスクロールして選ぶ
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { useSetRatingMutation } from '#lib/queries/tracks.js';
  import {
    addNextInQueue,
    addToQueue,
    player,
    playTrackFromQueue
  } from '#lib/stores/player.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import type { Track } from '#lib/types/models.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import FavoriteButton from './FavoriteButton.svelte';
  import MetadataEditor from '../MetadataEditor.svelte';
  import ContextMenu from '../ContextMenu.svelte';
  import DeleteTrackDialog from '../DeleteTrackDialog.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import TrackColumnMenu from './TrackColumnMenu.svelte';
  import { TrackListView, trackListView } from '#lib/stores/trackListView.svelte.js';
  import {
    TRACK_COLUMNS,
    moveColumn,
    trackCellText,
    type TrackColumnId
  } from '#lib/utils/trackColumns.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import { moveItems } from '#lib/utils/reorder.js';
  import {
    createTrackSorter,
    initialSortDirection,
    type TrackSort,
    type TrackSortField
  } from '#lib/utils/trackSort.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    tracks: Track[] | null;
    isLoading?: boolean;
    isError?: boolean;
    error?: Error | null;
    searchTerm?: string;
    emptyMessage?: string;
    emptyHint?: string;
    displayMode?: 'grid' | 'list';
    /**
     * 列と並び順を覚える単位（画面の名前。同じ名前の一覧は、同じ設定を使う）
     *
     * 省略時は、覚えない（開くたびに既定の列・並び順になる）。
     */
    viewId?: string;
    /** 最初に表示する列（省略時は、タイトル・アーティスト・お気に入り・評価・時間） */
    defaultColumns?: readonly TrackColumnId[];
    /**
     * 最初の並び順（省略時は、追加した日時の新しい順）
     *
     * nullなら、渡した順のまま並べる（見出しをクリックすると、その項目で並べ替える）。
     */
    defaultSort?: TrackSort | null;
    /**
     * 並び順だけを覚える単位（省略時は`viewId`と同じ）
     *
     * 列はどのプレイリストでも共通にし、並び順だけをプレイリストごとに覚える、という場合に使う。
     */
    sortViewId?: string;
    /** 一覧の名前（スクリーンリーダー向け。省略時は「曲」） */
    label?: string;
    /**
     * 渡した順のまま並べている間に、行のドラッグで並べ替えた時に呼ばれる（新しい並びの全トラックID）
     *
     * 指定した一覧（プレイリスト）だけ、行のドラッグで並べ替えられる。
     */
    onReorder?: (trackIds: string[]) => void;
    /** 選択した曲を、この一覧（プレイリスト）から外す操作（右クリックのメニュー・Deleteキー） */
    onRemove?: (tracks: Track[]) => void;
    /** 一覧から外す操作の表示名 */
    removeLabel?: string;
    /**
     * 再生中の曲へのジャンプで、この一覧にその曲がない場合に、ジャンプを取り消すか
     * （ライブラリの全曲の一覧に指定する。ほかの一覧は、全曲の一覧へ移動するために残す）
     */
    isRevealFallback?: boolean;
  }

  let {
    tracks,
    isLoading = false,
    isError = false,
    error = null,
    searchTerm = '',
    emptyMessage,
    emptyHint,
    displayMode = 'list',
    viewId,
    sortViewId,
    label,
    defaultColumns,
    defaultSort,
    onReorder,
    onRemove,
    removeLabel,
    isRevealFallback = false
  }: Props = $props();

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_LIST_ROW_HEIGHT = 36;
  // グリッド表示のカードの、アルバムアートを除いた高さの見積もり
  const ESTIMATED_CARD_EXTRA_HEIGHT = 76;
  // グリッド表示のカードの間隔（px）
  const GRID_GAP = 12;
  // グリッド表示で、見えている範囲の前後に余分に描画する行の数（1行に何枚も並ぶため、少なくする）
  const GRID_OVERSCAN_ROWS = 2;

  // 列と並び順（画面ごとに覚える。既定の値は、最初に受け取ったものを使う）
  // svelte-ignore state_referenced_locally
  const viewDefaults = { columns: defaultColumns, sort: defaultSort };
  // svelte-ignore state_referenced_locally
  const view = viewId ? trackListView(viewId, viewDefaults) : new TrackListView(null, viewDefaults);
  // 並び順を別の単位で覚える場合は、その設定の並び順だけを使う
  // svelte-ignore state_referenced_locally
  const sortView = sortViewId ? trackListView(sortViewId, viewDefaults) : view;
  const columns = $derived(view.columns);
  const sort = $derived(sortView.sort);

  // 行のドラッグで並べ替えられるか（渡した順のまま並べている間だけ）
  const canReorder = $derived(onReorder !== undefined && sort === null);
  // 並べ替えで運んでいる曲と、落とす先の行
  let reorderIds = $state<string[] | null>(null);
  let dropTargetId = $state<string | null>(null);
  // 下へ動かしているか（落とした行の後ろに入るため、線を行の下に出す）
  let dropAfter = $state(false);

  const columnLabels: Record<TrackColumnId, string> = {
    title: m.fields.title,
    artist: m.fields.artist,
    album: m.fields.album,
    albumArtist: m.fields.albumArtist,
    genre: m.fields.genre,
    year: m.fields.year,
    trackNumber: m.fields.trackNumber,
    discNumber: m.fields.discNumber,
    favorite: m.fields.favorite,
    rating: m.fields.rating,
    playCount: m.fields.playCount,
    skipCount: m.fields.skipCount,
    lastPlayedAt: m.fields.lastPlayedAt,
    createdAt: m.fields.createdAt,
    duration: m.fields.duration,
    format: m.fields.format,
    bitrate: m.fields.bitrate,
    sampleRate: m.fields.sampleRate,
    fileSize: m.fields.fileSize
  };

  // アルバムアートサイズ
  const artSize = $derived(ui.gridCardSize);

  // 見えている行だけを描画する一覧（リスト表示・グリッド表示のどちらか）
  let virtualList = $state<VirtualList<Track>>();

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => sortedTracks ?? []);
  let showMetadataEditor = $state(false);
  let showDeleteDialog = $state(false);

  // コンテキストメニュー状態
  let contextMenu = $state<{ x: number; y: number; track: Track } | null>(null);

  // ドラッグ状態（運んでいるのは選択中の曲）
  let isDragging = $state(false);

  // 列リサイズ状態
  let isResizing = $state(false);
  let resizingColumn = $state<TrackColumnId | null>(null);
  let resizeStartX = $state(0);
  let resizeStartWidth = $state(0);

  // 列を選ぶメニュー（見出しの右クリック）
  let columnMenu = $state<{ x: number; y: number } | null>(null);

  // 見出しのドラッグでの、列の入れ替え
  let draggingColumn = $state<TrackColumnId | null>(null);
  let dropTargetColumn = $state<TrackColumnId | null>(null);

  // グリッドテンプレート列を計算（番号の列の後に、選んだ列を並べる）
  const gridTemplateColumns = $derived(
    [ui.columnWidths.number, ...columns.map((column) => ui.columnWidths[column])]
      .map((width) => `${width}px`)
      .join(' ')
  );

  // レーティングミューテーション
  let setRatingMutation = $derived(useSetRatingMutation());

  // ソートされたトラック
  const sortTracks = createTrackSorter();
  const sortedTracks = $derived(
    tracks && sort ? sortTracks(tracks, sort.field, sort.direction) : tracks
  );

  /** 見出しのクリック: その列で並べ替える（同じ列なら、向きを逆にする） */
  function toggleSort(field: TrackSortField) {
    sortView.sort =
      sort?.field === field
        ? { field, direction: sort.direction === 'asc' ? 'desc' : 'asc' }
        : { field, direction: initialSortDirection(field) };
  }

  function getSortIcon(field: TrackSortField): string {
    if (sort?.field !== field) return '';
    return sort.direction === 'asc' ? '↑' : '↓';
  }

  // 再生中の曲へのジャンプ: この一覧にその曲があれば、その行までスクロールして選ぶ
  $effect(() => {
    const trackId = ui.revealTrackId;
    if (trackId === null || !sortedTracks || !virtualList) return;
    const index = sortedTracks.findIndex((track) => track.id === trackId);
    untrack(() => {
      if (index >= 0) {
        selection.click(trackId, { shiftKey: false, toggleKey: false });
        virtualList?.scrollToIndex(index);
        ui.revealTrackId = null;
      } else if (isRevealFallback && !searchTerm) {
        // 全曲の一覧にもない曲（ライブラリから外した曲）は、移動できない
        ui.revealTrackId = null;
      }
    });
  });

  const selectedTracks = $derived.by(() => {
    if (!sortedTracks) return [];
    return sortedTracks.filter((track) => selection.has(track.id));
  });

  function handleTrackClick(trackId: string, event: MouseEvent) {
    selection.click(trackId, {
      shiftKey: event.shiftKey,
      toggleKey: event.ctrlKey || event.metaKey
    });
  }

  /**
   * 一覧のキーボード操作（矢印キーで選択を移す・Enterで再生する）
   */
  function handleListKeydown(event: KeyboardEvent) {
    // Delete / Backspace: 選択した曲を、この一覧（プレイリスト）から外す
    if (
      onRemove &&
      (event.key === 'Delete' || event.key === 'Backspace') &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      selectedTracks.length > 0
    ) {
      event.preventDefault();
      handleRemove();
      return;
    }

    const indexOf = (trackId: string) => sortedTracks?.findIndex((t) => t.id === trackId) ?? -1;
    handleTrackListKeydown(event, selection, {
      columns: () => (displayMode === 'grid' ? (virtualList?.getColumns() ?? null) : null),
      onActivate: (trackId) => playFromIndex(indexOf(trackId)),
      // 移動先の行は描画されていないことがあるため、一覧の中の位置でスクロールする
      scrollTo: (trackId) => virtualList?.scrollToIndex(indexOf(trackId))
    });
  }

  function clearSelection() {
    selection.clear();
  }

  function openMetadataEditor() {
    if (selection.size > 0) {
      showMetadataEditor = true;
    }
  }

  function closeMetadataEditor() {
    showMetadataEditor = false;
  }

  function handleMetadataSaved() {
    clearSelection();
  }

  /** 一覧の中の位置を指定して、その曲から一覧を再生する */
  function playFromIndex(index: number) {
    if (!sortedTracks || index < 0 || index >= sortedTracks.length) return;
    playTrackFromQueue(sortedTracks, index);
  }

  function handleContextMenu(event: MouseEvent, track: Track) {
    event.preventDefault();

    selection.ensureSelected(track.id);

    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      track
    };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function handlePlayNext() {
    addNextInQueue(selectedTracks);
  }

  function handleAddToQueue() {
    addToQueue(selectedTracks);
  }

  /**
   * 削除ダイアログを開く
   */
  function openDeleteDialog() {
    if (selection.size > 0) {
      showDeleteDialog = true;
    }
  }

  /**
   * 削除ダイアログを閉じる
   */
  function closeDeleteDialog() {
    showDeleteDialog = false;
    clearSelection();
  }

  const cardWidth = $derived(artSize + 24);

  /** 選択した曲を、この一覧（プレイリスト）から外す */
  function handleRemove() {
    if (!onRemove || selectedTracks.length === 0) return;
    onRemove(selectedTracks);
    clearSelection();
  }

  function handleDragStart(event: DragEvent, track: Track) {
    // 選択中の曲の上で始めた場合は選択中の曲すべて、そうでなければその曲だけを運ぶ
    const trackIds = selection.beginDrag(track.id);
    // 並べ替えられる一覧では、この一覧の中への移動（並べ替え）も、ほかのプレイリストへの追加もできる
    startTrackDrag(
      event,
      trackIds,
      m.common.trackCount(trackIds.length),
      canReorder ? 'copyMove' : 'copy'
    );

    isDragging = true;
    reorderIds = canReorder ? trackIds : null;
  }

  function handleDragEnd() {
    isDragging = false;
    reorderIds = null;
    dropTargetId = null;
  }

  /** 並べ替え: この一覧から運んでいる曲を、行の上へ持ってきた */
  function handleRowDragOver(event: DragEvent, track: Track) {
    if (reorderIds === null) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropTargetId = reorderIds.includes(track.id) ? null : track.id;
    const indexOf = (id: string) => sortedTracks?.findIndex((t) => t.id === id) ?? -1;
    const firstMoved = Math.min(...reorderIds.map(indexOf).filter((index) => index >= 0));
    dropAfter = firstMoved < indexOf(track.id);
  }

  /** 並べ替え: 運んでいる曲を、落とした行の位置へ動かす */
  function handleRowDrop(event: DragEvent, track: Track) {
    if (reorderIds === null || !sortedTracks) return;
    event.preventDefault();
    const order = sortedTracks.map((t) => t.id);
    const next = moveItems(order, reorderIds, track.id);
    handleDragEnd();
    if (next.some((id, index) => id !== order[index])) onReorder?.(next);
  }

  function openColumnMenu(event: MouseEvent) {
    event.preventDefault();
    columnMenu = { x: event.clientX, y: event.clientY };
  }

  /** 列の入れ替えで運ぶデータの種類（曲のドラッグと区別する） */
  const COLUMN_DRAG_TYPE = 'application/x-muspice-column';

  function handleColumnDragStart(event: DragEvent, column: TrackColumnId) {
    // 幅を変えている間は、列を運ばない
    if (isResizing || !event.dataTransfer) {
      event.preventDefault();
      return;
    }
    event.dataTransfer.setData(COLUMN_DRAG_TYPE, column);
    event.dataTransfer.effectAllowed = 'move';
    draggingColumn = column;
  }

  function handleColumnDragOver(event: DragEvent, column: TrackColumnId) {
    if (draggingColumn === null) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropTargetColumn = column;
  }

  function handleColumnDrop(event: DragEvent, column: TrackColumnId) {
    if (draggingColumn === null) return;
    event.preventDefault();
    view.columns = moveColumn(columns, draggingColumn, column);
    handleColumnDragEnd();
  }

  function handleColumnDragEnd() {
    draggingColumn = null;
    dropTargetColumn = null;
  }

  // 列リサイズ開始
  function handleResizeStart(event: MouseEvent, column: TrackColumnId) {
    event.preventDefault();
    event.stopPropagation();
    isResizing = true;
    resizingColumn = column;
    resizeStartX = event.clientX;
    resizeStartWidth = ui.columnWidths[column];

    document.addEventListener('mousemove', handleResizeMove);
    document.addEventListener('mouseup', handleResizeEnd);
  }

  // 列リサイズ中
  function handleResizeMove(event: MouseEvent) {
    if (!isResizing || !resizingColumn) return;

    const delta = event.clientX - resizeStartX;
    const newWidth = Math.max(TRACK_COLUMNS[resizingColumn].minWidth, resizeStartWidth + delta);

    ui.columnWidths = {
      ...ui.columnWidths,
      [resizingColumn]: newWidth
    };
  }

  // 列リサイズ終了
  function handleResizeEnd() {
    isResizing = false;
    resizingColumn = null;
    document.removeEventListener('mousemove', handleResizeMove);
    document.removeEventListener('mouseup', handleResizeEnd);
  }
</script>

<div class="flex flex-col h-full">
  <!-- コンテンツ -->
  <div class="flex-1 min-h-0">
    {#if isLoading}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>{m.common.loading}</p>
      </div>
    {:else if isError}
      <div class="empty-state text-error">
        <p>{m.common.errorOccurred}</p>
        <p class="text-sm text-text-muted">{error?.message || m.common.unknownError}</p>
      </div>
    {:else if sortedTracks && sortedTracks.length > 0}
      {#if displayMode === 'list'}
        <!-- リスト表示（一覧がフォーカスを受けてキー操作を扱う。枠は出さず、選択中の行の色で示す） -->
        <VirtualList
          bind:this={virtualList}
          items={sortedTracks}
          getKey={(track) => track.id}
          estimatedRowHeight={ESTIMATED_LIST_ROW_HEIGHT}
          class="outline-none"
          role="listbox"
          aria-multiselectable="true"
          aria-label={label ?? m.library.songs}
          tabindex={0}
          onkeydown={handleListKeydown}
        >
          {#snippet header()}
            <!-- 見出しの右クリックで、列を選ぶメニューを開く -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="table-header"
              style="grid-template-columns: {gridTemplateColumns};"
              oncontextmenu={openColumnMenu}
            >
              <div class="col-number">#</div>
              {#each columns as column (column)}
                <!-- 見出しは、ドラッグで列を入れ替えられる -->
                <div
                  class="header-cell align-{TRACK_COLUMNS[column].align}"
                  class:dragging={draggingColumn === column}
                  class:drop-target={dropTargetColumn === column && draggingColumn !== column}
                  draggable="true"
                  ondragstart={(e) => handleColumnDragStart(e, column)}
                  ondragover={(e) => handleColumnDragOver(e, column)}
                  ondrop={(e) => handleColumnDrop(e, column)}
                  ondragend={handleColumnDragEnd}
                  data-column={column}
                >
                  <button
                    class="sortable"
                    onclick={() => toggleSort(column)}
                    title={columnLabels[column]}
                    aria-label={columnLabels[column]}
                  >
                    <span class="truncate">
                      {column === 'favorite' ? '♥' : columnLabels[column]}
                    </span>
                    {getSortIcon(column)}
                  </button>
                  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                  <div
                    class="resize-handle"
                    onmousedown={(e) => handleResizeStart(e, column)}
                    role="separator"
                    aria-orientation="vertical"
                  ></div>
                </div>
              {/each}
            </div>
          {/snippet}

          {#snippet row(track, index)}
            <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              class="track-row"
              class:selected={selection.has(track.id)}
              class:missing={track.isMissing}
              title={track.isMissing ? m.common.fileMissing : undefined}
              class:playing={player.currentTrack?.id === track.id}
              class:dragging={isDragging && selection.has(track.id)}
              class:drop-target={dropTargetId === track.id}
              class:drop-after={dropTargetId === track.id && dropAfter}
              style="grid-template-columns: {gridTemplateColumns};"
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track)}
              ondragend={handleDragEnd}
              ondragover={(e) => handleRowDragOver(e, track)}
              ondrop={(e) => handleRowDrop(e, track)}
              onclick={(e) => handleTrackClick(track.id, e)}
              ondblclick={() => playFromIndex(index)}
              oncontextmenu={(e) => handleContextMenu(e, track)}
              role="option"
              aria-selected={selection.has(track.id)}
              data-track-id={track.id}
            >
              <div class="col-number flex items-center justify-center">
                {#if player.currentTrack?.id === track.id}
                  <PlayingIndicator size="small" />
                {:else}
                  <span class="track-index">
                    {index + 1}
                  </span>
                {/if}
              </div>
              {#each columns as column (column)}
                {#if column === 'title'}
                  <MarqueeText text={track.title || track.fileName} class="text-text-primary" />
                {:else if column === 'artist'}
                  <MarqueeText
                    text={track.artist || m.common.unknownArtist}
                    class="cell-artist text-text-secondary text-sm"
                  />
                {:else if column === 'album' || column === 'albumArtist' || column === 'genre'}
                  <MarqueeText
                    text={trackCellText(track, column)}
                    class="text-text-secondary text-sm"
                  />
                {:else if column === 'favorite'}
                  <div class="flex items-center justify-center">
                    <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
                  </div>
                {:else if column === 'rating'}
                  <div class="col-rating flex items-center justify-center">
                    <RatingStars
                      rating={track.rating}
                      onChange={(rating) =>
                        setRatingMutation.mutateAsync({ trackId: track.id, rating })}
                    />
                  </div>
                {:else}
                  <div class="cell align-{TRACK_COLUMNS[column].align}">
                    {trackCellText(track, column)}
                  </div>
                {/if}
              {/each}
            </div>
          {/snippet}
        </VirtualList>
      {:else}
        <!-- グリッド表示 -->
        <VirtualList
          bind:this={virtualList}
          items={sortedTracks}
          getKey={(track) => track.id}
          estimatedRowHeight={artSize + ESTIMATED_CARD_EXTRA_HEIGHT}
          minColumnWidth={cardWidth}
          gap={GRID_GAP}
          overscan={GRID_OVERSCAN_ROWS}
          class="outline-none justify-items-center"
          role="listbox"
          aria-multiselectable="true"
          aria-label={label ?? m.library.songs}
          tabindex={0}
          onkeydown={handleListKeydown}
        >
          {#snippet row(track, index)}
            <!-- キー操作は一覧（listbox）で受けるため、カードはフォーカスを受けない -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              class="track-card"
              class:selected={selection.has(track.id)}
              class:missing={track.isMissing}
              title={track.isMissing ? m.common.fileMissing : undefined}
              class:playing={player.currentTrack?.id === track.id}
              style="width: {cardWidth}px;"
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track)}
              ondragend={handleDragEnd}
              onclick={(e) => handleTrackClick(track.id, e)}
              ondblclick={() => playFromIndex(index)}
              oncontextmenu={(e) => handleContextMenu(e, track)}
              role="option"
              aria-selected={selection.has(track.id)}
              data-track-id={track.id}
            >
              <div
                class="relative shrink-0 rounded-md overflow-hidden bg-base-400 mb-2"
                style="width: {artSize}px; height: {artSize}px;"
              >
                <AlbumArt
                  src={albumArtUrl(track.id)}
                  alt={m.common.albumArt}
                  placeholderType="music"
                />
                {#if player.currentTrack?.id === track.id}
                  <div class="absolute inset-0 bg-black/50 flex items-center justify-center">
                    <PlayingIndicator size="large" />
                  </div>
                {/if}
              </div>
              <div class="w-full text-center min-w-0">
                <MarqueeText
                  text={track.title || track.fileName}
                  class="font-semibold mb-1 text-sm text-text-primary"
                />
                <MarqueeText
                  text={track.artist || m.common.unknownArtist}
                  class="text-xs text-text-muted"
                />
              </div>
            </div>
          {/snippet}
        </VirtualList>
      {/if}
    {:else}
      <div class="empty-state">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-16 h-16 text-text-dimmed/50 mb-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
          />
        </svg>
        <p>{emptyMessage ?? m.library.emptyLibrary}</p>
        <p class="text-sm text-text-dimmed">{emptyHint ?? m.library.emptyLibraryHint}</p>
      </div>
    {/if}
  </div>
</div>

<!-- メタデータエディタ -->
{#if showMetadataEditor}
  <MetadataEditor
    tracks={selectedTracks}
    onClose={closeMetadataEditor}
    onSave={handleMetadataSaved}
  />
{/if}

<!-- コンテキストメニュー -->
{#if contextMenu}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    track={contextMenu.track}
    tracks={sortedTracks || []}
    selectedTrackIds={selection.ids}
    onClose={closeContextMenu}
    onEditMetadata={openMetadataEditor}
    onPlayNext={handlePlayNext}
    onAddToQueue={handleAddToQueue}
    onDelete={openDeleteDialog}
    onRemoveFromList={onRemove ? handleRemove : undefined}
    removeFromListLabel={removeLabel}
  />
{/if}

<!-- 列を選ぶメニュー -->
{#if columnMenu}
  <TrackColumnMenu
    x={columnMenu.x}
    y={columnMenu.y}
    {view}
    {sortView}
    labels={columnLabels}
    onClose={() => (columnMenu = null)}
  />
{/if}

<!-- 削除ダイアログ -->
<DeleteTrackDialog
  bind:open={showDeleteDialog}
  tracks={selectedTracks}
  onClose={closeDeleteDialog}
/>

<style>
  @reference "../../../app.css";
  /* 列の見出し（VirtualListが一覧の上に固定して表示する） */
  /* 列が多くて幅に収まらない場合は、見出しと行を列の幅の合計まで広げ、横にスクロールする */
  .table-header {
    @apply grid gap-3 px-4 py-1.5 text-xs font-semibold uppercase text-text-muted border-b border-border bg-base-100;
    min-width: max-content;
  }

  .header-cell {
    @apply relative flex items-center min-w-0;
  }

  .header-cell.align-right {
    @apply justify-end;
  }

  .header-cell.align-center {
    @apply justify-center;
  }

  /* 入れ替えで運んでいる列と、落とす先の列 */
  .header-cell.dragging {
    @apply opacity-50;
  }

  .header-cell.drop-target {
    box-shadow: inset 2px 0 0 var(--color-primary);
  }

  .resize-handle {
    @apply absolute right-0 top-0 bottom-0 w-1 cursor-col-resize bg-transparent transition-colors;
    transform: translateX(50%);
  }

  .resize-handle:hover {
    @apply bg-primary;
  }

  .sortable {
    @apply flex items-center gap-1 min-w-0 bg-transparent border-none text-text-muted cursor-pointer text-left text-xs font-semibold uppercase p-0 transition-colors hover:text-text-primary;
  }

  .track-row {
    @apply grid gap-3 px-4 py-1.5 items-center cursor-pointer rounded transition-colors select-none;
    min-width: max-content;
  }

  /* 文字で出す列のセル */
  .cell {
    @apply min-w-0 truncate text-text-muted text-sm tabular-nums;
  }

  .cell.align-right {
    @apply text-right;
  }

  .cell.align-center {
    @apply text-center;
  }

  .track-row:hover {
    @apply bg-surface;
  }

  /* ファイルが見つからない曲（再生できない）は薄く表示する */
  .track-row.missing {
    @apply opacity-50;
  }

  .track-row.selected {
    @apply bg-primary/20;
  }

  .track-row.playing {
    @apply bg-secondary/15;
  }

  .track-row.playing :global(.cell-artist) {
    @apply text-secondary;
  }

  .track-row.dragging {
    @apply opacity-50 bg-primary/30;
  }

  /* 並べ替えで、運んでいる曲を落とす先の行 */
  .track-row.drop-target {
    box-shadow: inset 0 2px 0 var(--color-primary);
  }

  .track-row.drop-target.drop-after {
    box-shadow: inset 0 -2px 0 var(--color-primary);
  }

  /* 番号列 */
  .col-number {
    @apply text-xs;
  }

  .track-index {
    @apply text-sm text-text-muted min-w-5 text-center;
  }

  /* レーティング */
  .col-rating {
    @apply text-xs text-text-muted;
  }

  /* トラックカード */
  .track-card {
    @apply p-3 bg-base-300 rounded-lg cursor-pointer transition-all border-2 border-transparent flex flex-col items-center;
  }

  .track-card:hover {
    @apply -translate-y-0.5 shadow-lg bg-surface-hover;
  }

  /* ファイルが見つからない曲（再生できない）は薄く表示する */
  .track-card.missing {
    @apply opacity-50;
  }

  .track-card.selected {
    @apply bg-primary/20 border-primary;
  }

  .track-card.playing {
    @apply border-secondary;
  }
</style>
