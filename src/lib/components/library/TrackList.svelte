<script lang="ts">
  import { useSetRatingMutation } from '#lib/queries/tracks.js';
  import {
    addNextInQueue,
    addToQueue,
    player,
    playTrackFromQueue
  } from '#lib/stores/player.svelte.js';
  import { ui, type ColumnWidths } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDuration } from '#lib/utils/format.js';
  import type { Track } from '#lib/types/models.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import FavoriteButton from './FavoriteButton.svelte';
  import MetadataEditor from '../MetadataEditor.svelte';
  import ContextMenu from '../ContextMenu.svelte';
  import DeleteTrackDialog from '../DeleteTrackDialog.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import {
    createTrackSorter,
    type SortDirection,
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
     * 最初の並び順（省略時は、追加した日時の新しい順）
     *
     * nullなら、渡した順のまま並べる（見出しをクリックすると、その項目で並べ替える）。
     */
    defaultSort?: { field: TrackSortField; direction: SortDirection } | null;
    /** リスト表示に、再生回数の列を出すか */
    showPlayCount?: boolean;
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
    defaultSort = { field: 'createdAt', direction: 'desc' },
    showPlayCount = false
  }: Props = $props();

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_LIST_ROW_HEIGHT = 36;
  // グリッド表示のカードの、アルバムアートを除いた高さの見積もり
  const ESTIMATED_CARD_EXTRA_HEIGHT = 76;
  // グリッド表示のカードの間隔（px）
  const GRID_GAP = 12;
  // グリッド表示で、見えている範囲の前後に余分に描画する行の数（1行に何枚も並ぶため、少なくする）
  const GRID_OVERSCAN_ROWS = 2;

  // 最初の並び順だけを受け取る（その後は、見出しのクリックで変える）
  // svelte-ignore state_referenced_locally
  let sortField = $state<TrackSortField | null>(defaultSort?.field ?? null);
  // svelte-ignore state_referenced_locally
  let sortDirection = $state<SortDirection>(defaultSort?.direction ?? 'asc');

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
  let resizingColumn = $state<keyof ColumnWidths | null>(null);
  let resizeStartX = $state(0);
  let resizeStartWidth = $state(0);

  // お気に入りのハートの列の幅（px）
  const FAVORITE_COLUMN_WIDTH = 20;
  // 再生回数の列の幅（px）
  const PLAY_COUNT_COLUMN_WIDTH = 72;

  // グリッドテンプレート列を計算
  const gridTemplateColumns = $derived(
    `${ui.columnWidths.number}px ${ui.columnWidths.title}px ${ui.columnWidths.artist}px ` +
      `${FAVORITE_COLUMN_WIDTH}px ${ui.columnWidths.rating}px ` +
      (showPlayCount ? `${PLAY_COUNT_COLUMN_WIDTH}px ` : '') +
      `${ui.columnWidths.duration}px`
  );

  // レーティングミューテーション
  let setRatingMutation = $derived(useSetRatingMutation());

  // ソートされたトラック
  const sortTracks = createTrackSorter();
  const sortedTracks = $derived(
    tracks && sortField ? sortTracks(tracks, sortField, sortDirection) : tracks
  );

  function toggleSort(field: TrackSortField) {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = 'asc';
    }
  }

  function getSortIcon(field: TrackSortField): string {
    if (sortField !== field) return '';
    return sortDirection === 'asc' ? '↑' : '↓';
  }

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

  function handleDragStart(event: DragEvent, track: Track) {
    // 選択中の曲の上で始めた場合は選択中の曲すべて、そうでなければその曲だけを運ぶ
    const trackIds = selection.beginDrag(track.id);
    startTrackDrag(event, trackIds, m.common.trackCount(trackIds.length));

    isDragging = true;
  }

  function handleDragEnd() {
    isDragging = false;
  }

  // 列リサイズ開始
  function handleResizeStart(event: MouseEvent, column: keyof ColumnWidths) {
    event.preventDefault();
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
    const newWidth = Math.max(50, resizeStartWidth + delta);

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
          aria-label={m.library.songs}
          tabindex={0}
          onkeydown={handleListKeydown}
        >
          {#snippet header()}
            <div class="table-header" style="grid-template-columns: {gridTemplateColumns};">
              <div class="col-number">#</div>
              <div class="resizable-header">
                <button class="sortable" onclick={() => toggleSort('title')}>
                  {m.fields.title}
                  {getSortIcon('title')}
                </button>
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                  class="resize-handle"
                  onmousedown={(e) => handleResizeStart(e, 'title')}
                  role="separator"
                  aria-orientation="vertical"
                ></div>
              </div>
              <div class="resizable-header">
                <button class="sortable" onclick={() => toggleSort('artist')}>
                  {m.fields.artist}
                  {getSortIcon('artist')}
                </button>
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                  class="resize-handle"
                  onmousedown={(e) => handleResizeStart(e, 'artist')}
                  role="separator"
                  aria-orientation="vertical"
                ></div>
              </div>
              <!-- お気に入りのハートの列（見出しは出さない） -->
              <div></div>
              <div class="resizable-header">
                <div class="col-rating">{m.fields.rating}</div>
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                  class="resize-handle"
                  onmousedown={(e) => handleResizeStart(e, 'rating')}
                  role="separator"
                  aria-orientation="vertical"
                ></div>
              </div>
              {#if showPlayCount}
                <button class="sortable text-right" onclick={() => toggleSort('playCount')}>
                  {m.fields.playCount}
                  {getSortIcon('playCount')}
                </button>
              {/if}
              <button class="sortable text-right" onclick={() => toggleSort('duration')}>
                {m.fields.duration}
                {getSortIcon('duration')}
              </button>
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
              style="grid-template-columns: {gridTemplateColumns};"
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
              <div class="col-number flex items-center justify-center">
                {#if player.currentTrack?.id === track.id}
                  <PlayingIndicator size="small" />
                {:else}
                  <span class="track-index">
                    {index + 1}
                  </span>
                {/if}
              </div>
              <MarqueeText
                text={searchTerm ? track.title || track.fileName : track.title || track.fileName}
                class="text-text-primary"
              />
              <MarqueeText
                text={searchTerm
                  ? track.artist || m.common.unknownArtist
                  : track.artist || m.common.unknownArtist}
                class="text-text-secondary text-sm"
              />
              <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
              <div class="col-rating flex items-center justify-center">
                <RatingStars
                  rating={track.rating}
                  onChange={(rating) =>
                    setRatingMutation.mutateAsync({ trackId: track.id, rating })}
                />
              </div>
              {#if showPlayCount}
                <div class="text-right text-text-muted text-sm tabular-nums">
                  {track.playCount}
                </div>
              {/if}
              <div class="text-right text-text-muted text-sm">
                {formatDuration(track.duration)}
              </div>
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
          aria-label={m.library.songs}
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
  .table-header {
    @apply grid gap-3 px-4 py-1.5 text-xs font-semibold uppercase text-text-muted border-b border-border bg-base-100;
  }

  .resizable-header {
    @apply relative flex items-center;
  }

  .resize-handle {
    @apply absolute right-0 top-0 bottom-0 w-1 cursor-col-resize bg-transparent transition-colors;
    transform: translateX(50%);
  }

  .resize-handle:hover {
    @apply bg-primary;
  }

  .sortable {
    @apply bg-transparent border-none text-text-muted cursor-pointer text-left text-xs font-semibold uppercase p-0 transition-colors hover:text-text-primary;
  }

  .track-row {
    @apply grid gap-3 px-4 py-1.5 items-center cursor-pointer rounded transition-colors select-none;
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

  .track-row.playing > div:nth-child(3) {
    @apply text-secondary;
  }

  .track-row.dragging {
    @apply opacity-50 bg-primary/30;
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
