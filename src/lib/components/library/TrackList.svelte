<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { setRating } from '$lib/queries/tracks';
  import {
    playTrackFromQueue,
    currentTrack,
    playQueue,
    currentTrackIndex
  } from '$lib/stores/player';
  import { columnWidths, gridCardSize, type ColumnWidths } from '$lib/stores/ui';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import { formatDuration } from '$lib/utils/format';
  import { get } from 'svelte/store';
  import type { Track } from '$lib/types/models';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import MetadataEditor from '../MetadataEditor.svelte';
  import ContextMenu from '../ContextMenu.svelte';
  import DeleteTrackDialog from '../DeleteTrackDialog.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

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
  }

  let {
    tracks,
    isLoading = false,
    isError = false,
    error = null,
    searchTerm = '',
    emptyMessage = '音楽ライブラリが空です',
    emptyHint = 'フォルダをインポートして音楽を追加してください',
    displayMode = 'list'
  }: Props = $props();

  type SortField = 'title' | 'artist' | 'album' | 'duration' | 'createdAt';
  type SortDirection = 'asc' | 'desc';
  let sortField = $state<SortField>('createdAt');
  let sortDirection = $state<SortDirection>('desc');

  // アルバムアートサイズ
  const artSize = $derived($gridCardSize);

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }

  // トラック選択状態
  let selectedTrackIds = $state<Set<string>>(new Set());
  let showMetadataEditor = $state(false);
  let showDeleteDialog = $state(false);

  // コンテキストメニュー状態
  let contextMenu = $state<{ x: number; y: number; track: Track } | null>(null);

  // ドラッグ状態
  let isDragging = $state(false);
  let draggedTrackIds = $state<string[]>([]);

  // 列リサイズ状態
  let isResizing = $state(false);
  let resizingColumn = $state<keyof ColumnWidths | null>(null);
  let resizeStartX = $state(0);
  let resizeStartWidth = $state(0);

  // グリッドテンプレート列を計算
  const gridTemplateColumns = $derived(
    `${$columnWidths.number}px ${$columnWidths.title}px ${$columnWidths.artist}px ${$columnWidths.rating}px ${$columnWidths.duration}px`
  );

  const queryClient = useQueryClient();

  // ソートされたトラック
  const sortedTracks = $derived.by(() => {
    if (!tracks) return null;

    return [...tracks].sort((a, b) => {
      let aVal: string | number | null;
      let bVal: string | number | null;

      switch (sortField) {
        case 'title':
          aVal = a.title || a.fileName;
          bVal = b.title || b.fileName;
          break;
        case 'artist':
          aVal = a.artist || '';
          bVal = b.artist || '';
          break;
        case 'album':
          aVal = a.album || '';
          bVal = b.album || '';
          break;
        case 'duration':
          aVal = a.duration || 0;
          bVal = b.duration || 0;
          break;
        case 'createdAt':
          aVal = a.createdAt;
          bVal = b.createdAt;
          break;
        default:
          return 0;
      }

      if (aVal === null) aVal = '';
      if (bVal === null) bVal = '';

      let comparison = 0;
      if (typeof aVal === 'string' && typeof bVal === 'string') {
        comparison = aVal.localeCompare(bVal, 'ja');
      } else {
        comparison = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      }

      return sortDirection === 'asc' ? comparison : -comparison;
    });
  });

  /**
   * レーティングを設定
   */
  async function handleSetRating(trackId: string, rating: number, event: MouseEvent) {
    event.stopPropagation();
    try {
      await setRating(trackId, rating);
      queryClient.invalidateQueries({ queryKey: ['tracks'] });
    } catch (error) {
      console.error('レーティングの設定に失敗しました:', error);
    }
  }

  function toggleSort(field: SortField) {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = 'asc';
    }
  }

  function getSortIcon(field: SortField): string {
    if (sortField !== field) return '';
    return sortDirection === 'asc' ? '↑' : '↓';
  }

  function highlightText(text: string, search: string): string {
    if (!search || !text) return text;
    const regex = new RegExp(`(${search})`, 'gi');
    return text.replace(regex, '<mark class="bg-warning text-black px-0.5 rounded-sm">$1</mark>');
  }

  const selectedTracks = $derived.by(() => {
    if (!sortedTracks) return [];
    return sortedTracks.filter((track) => selectedTrackIds.has(track.id));
  });

  function toggleTrackSelection(trackId: string, event: MouseEvent | KeyboardEvent) {
    event.stopPropagation();

    if (event instanceof KeyboardEvent) {
      const newSelection = new Set(selectedTrackIds);
      if (newSelection.has(trackId)) {
        newSelection.clear();
      } else {
        newSelection.clear();
        newSelection.add(trackId);
      }
      selectedTrackIds = newSelection;
      return;
    }

    const newSelection = new Set(selectedTrackIds);

    if (event.shiftKey && selectedTrackIds.size > 0 && sortedTracks) {
      const lastSelectedId = Array.from(selectedTrackIds).pop();
      const lastIndex = sortedTracks.findIndex((t) => t.id === lastSelectedId);
      const currentIndex = sortedTracks.findIndex((t) => t.id === trackId);

      if (lastIndex !== -1 && currentIndex !== -1) {
        const start = Math.min(lastIndex, currentIndex);
        const end = Math.max(lastIndex, currentIndex);

        for (let i = start; i <= end; i++) {
          newSelection.add(sortedTracks[i].id);
        }
      }
    } else if (event.ctrlKey || event.metaKey) {
      if (newSelection.has(trackId)) {
        newSelection.delete(trackId);
      } else {
        newSelection.add(trackId);
      }
    } else {
      if (newSelection.has(trackId) && newSelection.size === 1) {
        newSelection.clear();
      } else {
        newSelection.clear();
        newSelection.add(trackId);
      }
    }

    selectedTrackIds = newSelection;
  }

  function clearSelection() {
    selectedTrackIds = new Set();
  }

  function openMetadataEditor() {
    if (selectedTrackIds.size > 0) {
      showMetadataEditor = true;
    }
  }

  function closeMetadataEditor() {
    showMetadataEditor = false;
  }

  function handleMetadataSaved() {
    clearSelection();
  }

  function handleTrackDoubleClick(track: Track) {
    if (!sortedTracks) return;

    const trackIndex = sortedTracks.findIndex((t) => t.id === track.id);
    if (trackIndex !== -1) {
      playTrackFromQueue(sortedTracks, trackIndex);
    }
  }

  function handleContextMenu(event: MouseEvent, track: Track) {
    event.preventDefault();

    if (!selectedTrackIds.has(track.id)) {
      selectedTrackIds = new Set([track.id]);
    }

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
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);

    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...selectedTracks);
    playQueue.set(newQueue);
  }

  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...selectedTracks]);
  }

  /**
   * 削除ダイアログを開く
   */
  function openDeleteDialog() {
    if (selectedTrackIds.size > 0) {
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
    if (!event.dataTransfer) return;

    event.dataTransfer.effectAllowed = 'copy';

    let trackIds: string[];
    if (selectedTrackIds.size > 0 && selectedTrackIds.has(track.id)) {
      trackIds = Array.from(selectedTrackIds);
    } else {
      trackIds = [track.id];
    }

    event.dataTransfer.setData('application/json', JSON.stringify(trackIds));
    event.dataTransfer.setData('text/plain', trackIds[0]);

    isDragging = true;
    draggedTrackIds = trackIds;

    const dragImage = document.createElement('div');
    dragImage.className = 'drag-preview';
    dragImage.innerHTML = `
      <div class="drag-preview-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
        </svg>
      </div>
      <span class="drag-preview-count">${trackIds.length}曲</span>
    `;
    dragImage.style.position = 'absolute';
    dragImage.style.left = '-9999px';
    dragImage.style.top = '-9999px';
    document.body.appendChild(dragImage);

    event.dataTransfer.setDragImage(dragImage, 40, 25);

    setTimeout(() => {
      document.body.removeChild(dragImage);
    }, 0);
  }

  function handleDragEnd() {
    isDragging = false;
    draggedTrackIds = [];
  }

  // 列リサイズ開始
  function handleResizeStart(event: MouseEvent, column: keyof ColumnWidths) {
    event.preventDefault();
    isResizing = true;
    resizingColumn = column;
    resizeStartX = event.clientX;
    resizeStartWidth = $columnWidths[column];

    document.addEventListener('mousemove', handleResizeMove);
    document.addEventListener('mouseup', handleResizeEnd);
  }

  // 列リサイズ中
  function handleResizeMove(event: MouseEvent) {
    if (!isResizing || !resizingColumn) return;

    const delta = event.clientX - resizeStartX;
    const newWidth = Math.max(50, resizeStartWidth + delta);

    columnWidths.update((widths) => ({
      ...widths,
      [resizingColumn!]: newWidth
    }));
  }

  // 列リサイズ終了
  function handleResizeEnd() {
    isResizing = false;
    resizingColumn = null;
    document.removeEventListener('mousemove', handleResizeMove);
    document.removeEventListener('mouseup', handleResizeEnd);
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

  // トラックカードが表示されたらアートを読み込み
  function handleTrackVisible(trackId: string) {
    loadAlbumArt(trackId);
  }
</script>

<div class="flex flex-col h-full">
  <!-- コンテンツ -->
  <div class="flex-1 overflow-y-auto">
    {#if isLoading}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>読み込み中...</p>
      </div>
    {:else if isError}
      <div class="empty-state text-error">
        <p>エラーが発生しました</p>
        <p class="text-sm text-text-muted">{error?.message || '不明なエラー'}</p>
      </div>
    {:else if sortedTracks && sortedTracks.length > 0}
      {#if displayMode === 'list'}
        <!-- リスト表示 -->
        <div class="track-table">
          <div class="table-header" style="grid-template-columns: {gridTemplateColumns};">
            <div class="col-number">#</div>
            <div class="resizable-header">
              <button class="sortable" onclick={() => toggleSort('title')}>
                タイトル {getSortIcon('title')}
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
                アーティスト {getSortIcon('artist')}
              </button>
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div
                class="resize-handle"
                onmousedown={(e) => handleResizeStart(e, 'artist')}
                role="separator"
                aria-orientation="vertical"
              ></div>
            </div>
            <div class="resizable-header">
              <div class="col-rating">評価</div>
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div
                class="resize-handle"
                onmousedown={(e) => handleResizeStart(e, 'rating')}
                role="separator"
                aria-orientation="vertical"
              ></div>
            </div>
            <button class="sortable text-right" onclick={() => toggleSort('duration')}>
              時間 {getSortIcon('duration')}
            </button>
          </div>
          <div class="flex flex-col">
            {#each sortedTracks as track (track.id)}
              <div
                class="track-row"
                class:selected={selectedTrackIds.has(track.id)}
                class:playing={$currentTrack?.id === track.id}
                class:dragging={isDragging && draggedTrackIds.includes(track.id)}
                style="grid-template-columns: {gridTemplateColumns};"
                draggable="true"
                ondragstart={(e) => handleDragStart(e, track)}
                ondragend={handleDragEnd}
                onclick={(e) => toggleTrackSelection(track.id, e)}
                ondblclick={() => handleTrackDoubleClick(track)}
                oncontextmenu={(e) => handleContextMenu(e, track)}
                onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
                role="button"
                tabindex="0"
              >
                <div class="col-number flex items-center justify-center">
                  {#if $currentTrack?.id === track.id}
                    <PlayingIndicator size="small" />
                  {:else}
                    <span class="track-index">
                      {sortedTracks.indexOf(track) + 1}
                    </span>
                  {/if}
                </div>
                <MarqueeText
                  text={searchTerm ? track.title || track.fileName : track.title || track.fileName}
                  class="text-text-primary"
                />
                <MarqueeText
                  text={searchTerm
                    ? track.artist || '不明なアーティスト'
                    : track.artist || '不明なアーティスト'}
                  class="text-text-secondary text-sm"
                />
                <div class="col-rating flex items-center justify-center">
                  <div class="rating-stars">
                    {#each [1, 2, 3, 4, 5] as star}
                      <button
                        class="star-btn"
                        class:active={track.rating >= star}
                        onclick={(e) =>
                          handleSetRating(track.id, track.rating === star ? 0 : star, e)}
                        title={`${star}つ星`}
                      >
                        ★
                      </button>
                    {/each}
                  </div>
                </div>
                <div class="text-right text-text-muted text-sm">
                  {formatDuration(track.duration)}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <!-- グリッド表示 -->
        <div
          class="grid gap-3 justify-items-center"
          style="grid-template-columns: repeat(auto-fill, minmax({cardWidth}px, 1fr));"
        >
          {#each sortedTracks as track (track.id)}
            <div
              class="track-card"
              class:selected={selectedTrackIds.has(track.id)}
              class:playing={$currentTrack?.id === track.id}
              style="width: {cardWidth}px;"
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track)}
              onclick={(e) => toggleTrackSelection(track.id, e)}
              ondblclick={() => handleTrackDoubleClick(track)}
              oncontextmenu={(e) => handleContextMenu(e, track)}
              onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
              role="button"
              tabindex="0"
              use:intersectionObserver={{ callback: () => handleTrackVisible(track.id) }}
            >
              <div
                class="relative shrink-0 rounded-md overflow-hidden bg-base-400 mb-2"
                style="width: {artSize}px; height: {artSize}px;"
              >
                <AlbumArt src={getArt(track.id)} alt="アルバムアート" placeholderType="music" />
                {#if $currentTrack?.id === track.id}
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
                  text={track.artist || '不明なアーティスト'}
                  class="text-xs text-text-muted"
                />
              </div>
            </div>
          {/each}
        </div>
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
        <p>{emptyMessage}</p>
        <p class="text-sm text-text-dimmed">{emptyHint}</p>
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
    {selectedTrackIds}
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
  /* トラックテーブル */
  .track-table {
    @apply flex flex-col;
  }

  .table-header {
    @apply grid gap-3 px-4 py-2 text-xs font-semibold uppercase text-text-muted border-b border-border sticky top-0 bg-base-100 z-10;
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
    @apply grid gap-3 px-4 py-2 items-center cursor-pointer rounded transition-colors;
  }

  .track-row:hover {
    @apply bg-surface;
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

  .rating-stars {
    @apply flex gap-px;
  }

  .star-btn {
    @apply bg-transparent border-none p-0 text-sm text-base-400 cursor-pointer transition-all leading-none;
  }

  .star-btn:hover {
    @apply text-warning scale-125;
  }

  .star-btn.active {
    @apply text-warning;
  }

  /* トラックカード */
  .track-card {
    @apply p-3 bg-base-300 rounded-lg cursor-pointer transition-all border-2 border-transparent flex flex-col items-center;
  }

  .track-card:hover {
    @apply -translate-y-0.5 shadow-lg bg-surface-hover;
  }

  .track-card.selected {
    @apply bg-primary/20 border-primary;
  }

  .track-card.playing {
    @apply border-secondary;
  }

  /* ドラッグプレビュー */
  :global(.drag-preview) {
    @apply flex items-center gap-2 px-3 py-2 bg-primary text-white rounded-md text-sm font-medium shadow-lg;
  }

  :global(.drag-preview-icon) {
    @apply w-5 h-5;
  }

  :global(.drag-preview-icon svg) {
    @apply w-full h-full;
  }

  :global(.drag-preview-count) {
    @apply whitespace-nowrap;
  }
</style>
