<!--
  @component PlayHistoryList
  再生履歴の一覧。日付ごとの見出しの下に、再生した曲を新しい順に並べる（同じ曲が何度も出る）。

  - 行は履歴の1件ごとに見分ける（選択・キー操作は、曲ではなく行が単位）
  - 再生・キューへの追加・プレイリストへの追加などは、選んだ行の曲（重複なし）を対象にする
  - 見出しと曲の行は同じ高さにする（VirtualListは、行の高さがすべて同じとして扱う）
-->
<script lang="ts">
  import {
    addNextInQueue,
    addToQueue,
    player,
    playTrackFromQueue
  } from '#lib/stores/player.svelte.js';
  import { formatDateWithWeekday, formatDuration, formatTime } from '#lib/utils/format.js';
  import type { PlayHistoryEntry, Track } from '#lib/types/models.js';
  import MetadataEditor from '../MetadataEditor.svelte';
  import ContextMenu from '../ContextMenu.svelte';
  import DeleteTrackDialog from '../DeleteTrackDialog.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import {
    buildPlayHistoryRows,
    daysAgo,
    distinctTracks,
    type PlayHistoryPlayRow,
    type PlayHistoryRow
  } from '#lib/utils/playHistory.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 再生履歴（新しい順。未取得ならnull） */
    entries: PlayHistoryEntry[] | null;
    /** 全曲の一覧（未取得ならnull） */
    tracks: Track[] | null;
    isLoading?: boolean;
    isError?: boolean;
    error?: Error | null;
  }

  let { entries, tracks, isLoading = false, isError = false, error = null }: Props = $props();

  // 行の高さ（見出しと曲の行で同じにする）
  const ROW_HEIGHT = 36;

  const rows = $derived(entries && tracks ? buildPlayHistoryRows(entries, tracks) : []);
  const playRows = $derived(rows.filter((row): row is PlayHistoryPlayRow => row.kind === 'play'));
  /** 行のIDから、一覧の中の位置（見出しを含む）を引く */
  const rowIndexes = $derived(new Map(rows.map((row, index) => [row.id, index])));
  /** 履歴に出てくる曲（重複なし・最近再生した順）。再生キューと、メニューの対象に使う */
  const historyTracks = $derived(distinctTracks(rows));

  let virtualList = $state<VirtualList<PlayHistoryRow>>();

  // 行の選択（クリック・キーボード）。見出しは選択の対象にしない
  const selection = new TrackSelection(() => playRows);
  /** 選択中の行の曲（重複なし・表示順） */
  const selectedTracks = $derived(distinctTracks(playRows.filter((row) => selection.has(row.id))));
  const selectedTrackIds = $derived(new Set(selectedTracks.map((track) => track.id)));

  let showMetadataEditor = $state(false);
  let showDeleteDialog = $state(false);
  let contextMenu = $state<{ x: number; y: number; track: Track } | null>(null);
  let isDragging = $state(false);

  /** 見出しの日付（今日・昨日は、その言葉で出す） */
  function dateLabel(date: Date): string {
    const days = daysAgo(date, new Date());
    if (days === 0) return m.library.today;
    if (days === 1) return m.library.yesterday;
    return formatDateWithWeekday(date);
  }

  function handleRowClick(row: PlayHistoryPlayRow, event: MouseEvent) {
    selection.click(row.id, {
      shiftKey: event.shiftKey,
      toggleKey: event.ctrlKey || event.metaKey
    });
  }

  /** その行の曲から、履歴の曲（重複なし）を再生する */
  function playRow(rowId: string) {
    const row = playRows.find((candidate) => candidate.id === rowId);
    if (!row) return;
    playTrackFromQueue(
      historyTracks,
      historyTracks.findIndex((track) => track.id === row.track.id)
    );
  }

  /** 一覧のキーボード操作（矢印キーで選択を移す・Enterで再生する） */
  function handleListKeydown(event: KeyboardEvent) {
    handleTrackListKeydown(event, selection, {
      onActivate: playRow,
      // 移動先の行は描画されていないことがあるため、一覧の中の位置でスクロールする
      scrollTo: (rowId) => {
        const index = rowIndexes.get(rowId);
        if (index !== undefined) virtualList?.scrollToIndex(index);
      }
    });
  }

  function handleContextMenu(event: MouseEvent, row: PlayHistoryPlayRow) {
    event.preventDefault();
    selection.ensureSelected(row.id);
    contextMenu = { x: event.clientX, y: event.clientY, track: row.track };
  }

  function handleDragStart(event: DragEvent, row: PlayHistoryPlayRow) {
    // 選択中の行の上で始めた場合は選択中の行すべて、そうでなければその行だけの曲を運ぶ
    const rowIds = new Set(selection.beginDrag(row.id));
    const trackIds = distinctTracks(playRows.filter((candidate) => rowIds.has(candidate.id))).map(
      (track) => track.id
    );
    startTrackDrag(event, trackIds, m.common.trackCount(trackIds.length));
    isDragging = true;
  }
</script>

<div class="flex flex-col h-full">
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
    {:else if rows.length > 0}
      <!-- 一覧がフォーカスを受けてキー操作を扱う。枠は出さず、選択中の行の色で示す -->
      <VirtualList
        bind:this={virtualList}
        items={rows}
        getKey={(row) => row.id}
        estimatedRowHeight={ROW_HEIGHT}
        class="outline-none"
        role="listbox"
        aria-multiselectable="true"
        aria-label={m.library.playHistory}
        tabindex={0}
        onkeydown={handleListKeydown}
      >
        {#snippet header()}
          <div class="table-header">
            <div>{m.library.playedAt}</div>
            <div>{m.fields.title}</div>
            <div>{m.fields.artist}</div>
            <div>{m.fields.album}</div>
            <div class="text-right">{m.fields.duration}</div>
          </div>
        {/snippet}

        {#snippet row(item)}
          {#if item.kind === 'date'}
            <div class="date-row" role="presentation" style="height: {ROW_HEIGHT}px;">
              <span class="font-semibold text-text-primary">{dateLabel(item.date)}</span>
              <span class="text-xs text-text-muted">{m.common.trackCount(item.count)}</span>
            </div>
          {:else}
            <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              class="play-row"
              class:selected={selection.has(item.id)}
              class:missing={item.track.isMissing}
              class:playing={player.currentTrack?.id === item.track.id}
              class:dragging={isDragging && selection.has(item.id)}
              title={item.track.isMissing ? m.common.fileMissing : undefined}
              style="height: {ROW_HEIGHT}px;"
              draggable="true"
              ondragstart={(event) => handleDragStart(event, item)}
              ondragend={() => (isDragging = false)}
              onclick={(event) => handleRowClick(item, event)}
              ondblclick={() => playRow(item.id)}
              oncontextmenu={(event) => handleContextMenu(event, item)}
              role="option"
              aria-selected={selection.has(item.id)}
            >
              <div class="text-sm text-text-muted tabular-nums">{formatTime(item.playedAt)}</div>
              <MarqueeText
                text={item.track.title || item.track.fileName}
                class="text-text-primary"
              />
              <MarqueeText
                text={item.track.artist || m.common.unknownArtist}
                class="text-text-secondary text-sm"
              />
              <MarqueeText text={item.track.album || ''} class="text-text-secondary text-sm" />
              <div class="text-right text-text-muted text-sm">
                {formatDuration(item.track.duration)}
              </div>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
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
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <p>{m.library.noPlayHistory}</p>
        <p class="text-sm text-text-dimmed">{m.library.noPlayHistoryHint}</p>
      </div>
    {/if}
  </div>
</div>

{#if showMetadataEditor}
  <MetadataEditor
    tracks={selectedTracks}
    onClose={() => (showMetadataEditor = false)}
    onSave={() => selection.clear()}
  />
{/if}

{#if contextMenu}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    track={contextMenu.track}
    tracks={historyTracks}
    {selectedTrackIds}
    onClose={() => (contextMenu = null)}
    onEditMetadata={() => (showMetadataEditor = selectedTracks.length > 0)}
    onPlayNext={() => addNextInQueue(selectedTracks)}
    onAddToQueue={() => addToQueue(selectedTracks)}
    onDelete={() => (showDeleteDialog = selectedTracks.length > 0)}
  />
{/if}

<DeleteTrackDialog
  bind:open={showDeleteDialog}
  tracks={selectedTracks}
  onClose={() => {
    showDeleteDialog = false;
    selection.clear();
  }}
/>

<style>
  @reference "../../../app.css";
  /* 列の幅（見出しと曲の行でそろえる）: 時刻・タイトル・アーティスト・アルバム・時間 */
  .table-header,
  .play-row {
    grid-template-columns: 3.5rem minmax(0, 3fr) minmax(0, 2fr) minmax(0, 2fr) 3.5rem;
  }

  /* 列の見出し（VirtualListが一覧の上に固定して表示する） */
  .table-header {
    @apply grid gap-3 px-4 py-1.5 text-xs font-semibold uppercase text-text-muted border-b border-border bg-base-100;
  }

  /* 日付の見出し */
  .date-row {
    @apply flex items-end gap-3 px-4 pb-1.5 border-b border-border select-none;
  }

  .play-row {
    @apply grid gap-3 px-4 items-center cursor-pointer rounded transition-colors select-none;
  }

  .play-row:hover {
    @apply bg-surface;
  }

  /* ファイルが見つからない曲（再生できない）は薄く表示する */
  .play-row.missing {
    @apply opacity-50;
  }

  .play-row.selected {
    @apply bg-primary/20;
  }

  .play-row.playing {
    @apply bg-secondary/15;
  }

  .play-row.dragging {
    @apply opacity-50 bg-primary/30;
  }
</style>
