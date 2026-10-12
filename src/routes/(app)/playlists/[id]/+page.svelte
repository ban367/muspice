<script lang="ts">
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import {
    usePlaylistsQuery,
    usePlaylistTracksQuery,
    useDeletePlaylistMutation,
    useRemoveTrackFromPlaylistMutation,
    useReorderPlaylistTracksMutation
  } from '#lib/queries/playlists.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import type { Playlist, Track } from '#lib/types/models.js';
  import { playTrackFromQueue } from '#lib/stores/player.svelte.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // URLからプレイリストIDを取得
  const playlistId = $derived(page.params.id);

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_ROW_HEIGHT = 52;

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const removeTrackMutation = useRemoveTrackFromPlaylistMutation();
  const reorderTracksMutation = useReorderPlaylistTracksMutation();

  // 選択されたプレイリスト
  const selectedPlaylist = $derived.by(() => {
    if (!playlistId || !playlistsQuery.data) return null;
    return playlistsQuery.data.find((p: Playlist) => p.id === playlistId) || null;
  });

  // プレイリストの曲の情報（見つかったプレイリストの分だけを取得する）
  const selectedPlaylistId = $derived(selectedPlaylist?.id ?? null);
  const tracksQuery = $derived(
    selectedPlaylistId === null ? null : usePlaylistTracksQuery(selectedPlaylistId)
  );
  const trackById = $derived(
    new Map((tracksQuery?.data ?? []).map((track: Track) => [track.id, track]))
  );

  // ドラッグ中のトラックID
  let draggedTrackId = $state<string | null>(null);

  // プレイリストのトラック（表示順）
  // 並びはプレイリストの一覧から取る（曲の削除を、曲の情報の取り直しを待たずに反映する）。
  // 情報をまだ取得していないトラック・ライブラリにないトラックは除く
  const playlistTracks = $derived.by(() =>
    selectedPlaylist
      ? selectedPlaylist.tracks
          .map((pt) => trackById.get(pt.trackId))
          .filter((t): t is Track => t !== undefined)
      : []
  );

  // 見えている行だけを描画する一覧
  let virtualList = $state<VirtualList<Track>>();

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => playlistTracks);

  // 別のプレイリストを開いたら、選択を消す
  $effect(() => {
    void playlistId;
    // 選択の中身には反応させない（選択を変えるたびに消えてしまう）
    untrack(() => selection.reset());
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
    const indexOf = (trackId: string) => playlistTracks.findIndex((t) => t.id === trackId);
    handleTrackListKeydown(event, selection, {
      onActivate: (trackId) => playFromIndex(indexOf(trackId)),
      // 移動先の行は描画されていないことがあるため、一覧の中の位置でスクロールする
      scrollTo: (trackId) => virtualList?.scrollToIndex(indexOf(trackId))
    });
  }

  /**
   * プレイリストを削除
   */
  async function handleDeletePlaylist() {
    if (!selectedPlaylist) return;

    if (!(await confirmDestructive(m.playlists.confirmDelete(selectedPlaylist.name)))) {
      return;
    }

    try {
      await deletePlaylistMutation.mutateAsync(selectedPlaylist.id);
      goto(resolve('playlists'));
    } catch (error) {
      console.error('プレイリストの削除に失敗しました:', error);
    }
  }

  /**
   * トラックを削除
   */
  async function handleRemoveTrack(trackId: string) {
    if (!selectedPlaylist) return;

    try {
      await removeTrackMutation.mutateAsync({
        playlistId: selectedPlaylist.id,
        trackId
      });
    } catch (error) {
      console.error('トラックの削除に失敗しました:', error);
    }
  }

  /**
   * ドラッグ開始
   *
   * このプレイリストの中での並び替え（ドラッグした行だけ）と、サイドバーの別のプレイリストへの
   * 追加（選択中の曲すべて）の両方に使う。
   */
  function handleDragStart(event: DragEvent, trackId: string) {
    draggedTrackId = trackId;
    const trackIds = selection.beginDrag(trackId);
    startTrackDrag(event, trackIds, m.common.trackCount(trackIds.length), 'copyMove');
  }

  /**
   * ドラッグオーバー
   */
  function handleDragOver(event: DragEvent) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'move';
    }
  }

  /**
   * トラックの並び替え
   */
  async function handleDropOnTrack(event: DragEvent, targetTrackId: string) {
    event.preventDefault();

    if (!selectedPlaylist || !draggedTrackId || draggedTrackId === targetTrackId) {
      draggedTrackId = null;
      return;
    }

    const tracks = selectedPlaylist.tracks;
    const draggedIndex = tracks.findIndex((t) => t.trackId === draggedTrackId);
    const targetIndex = tracks.findIndex((t) => t.trackId === targetTrackId);

    if (draggedIndex === -1 || targetIndex === -1) {
      draggedTrackId = null;
      return;
    }

    // 新しい順序を作成
    const newOrder = [...tracks];
    const [removed] = newOrder.splice(draggedIndex, 1);
    newOrder.splice(targetIndex, 0, removed);

    try {
      await reorderTracksMutation.mutateAsync({
        playlistId: selectedPlaylist.id,
        trackIds: newOrder.map((t) => t.trackId)
      });
    } catch (error) {
      console.error('トラックの並び替えに失敗しました:', error);
    }

    draggedTrackId = null;
  }

  /**
   * プレイリストの中の位置を指定して、その曲からプレイリストを再生
   */
  function playFromIndex(index: number) {
    if (index < 0 || index >= playlistTracks.length) return;
    playTrackFromQueue(playlistTracks, index);
  }

  /**
   * プレイリストの合計時間を計算
   */
  const totalDuration = $derived(
    playlistTracks.reduce((total, track) => total + (track.duration || 0), 0)
  );
</script>

<div class="playlist-detail-page">
  {#if playlistsQuery.isLoading}
    <div class="loading">{m.common.loading}</div>
  {:else if !selectedPlaylist}
    <div class="no-selection">
      <h2>{m.playlists.notFound}</h2>
      <p>{m.playlists.notFoundHint}</p>
      <a href={resolve('playlists')} class="back-link">{m.playlists.backToList}</a>
    </div>
  {:else}
    <!-- プレイリスト詳細 -->
    <div class="playlist-header">
      <div class="playlist-info">
        <div class="playlist-icon">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-playlist"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 6h16M4 10h16M4 14h16M4 18h16"
            />
          </svg>
        </div>
        <div class="playlist-details">
          <h1 class="playlist-title">{selectedPlaylist.name}</h1>
          <p class="playlist-meta">
            {m.common.trackCountAndDuration(
              selectedPlaylist.tracks.length,
              formatTotalDuration(totalDuration)
            )}
          </p>
        </div>
      </div>
      <div class="playlist-actions">
        <button
          class="btn-play-all"
          onclick={() => playFromIndex(0)}
          disabled={playlistTracks.length === 0}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-play"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M8 5v14l11-7z" />
          </svg>
          {m.common.playAll}
        </button>
        <button
          class="btn-delete"
          onclick={handleDeletePlaylist}
          title={m.playlists.deletePlaylist}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-delete"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
        </button>
      </div>
    </div>

    <div class="track-list">
      {#if selectedPlaylist.tracks.length === 0}
        <div class="empty-playlist">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-empty"
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
          <p>{m.playlists.noTracks}</p>
          <p class="hint">{m.playlists.noTracksHint}</p>
        </div>
      {:else}
        <!-- トラック一覧（一覧がフォーカスを受けてキー操作を扱う。枠は出さず、選択中の行の色で示す） -->
        <VirtualList
          bind:this={virtualList}
          items={playlistTracks}
          getKey={(track) => track.id}
          estimatedRowHeight={ESTIMATED_ROW_HEIGHT}
          class="outline-none"
          role="listbox"
          aria-multiselectable="true"
          aria-label={selectedPlaylist.name}
          tabindex={0}
          onkeydown={handleListKeydown}
        >
          {#snippet header()}
            <div class="track-header">
              <div class="col-number">#</div>
              <div class="col-title">{m.fields.title}</div>
              <div class="col-artist">{m.fields.artist}</div>
              <div class="col-album">{m.fields.album}</div>
              <div class="col-duration">{m.fields.duration}</div>
              <div class="col-actions"></div>
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
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track.id)}
              ondragend={() => (draggedTrackId = null)}
              ondragover={handleDragOver}
              ondrop={(e) => handleDropOnTrack(e, track.id)}
              onclick={(e) => handleTrackClick(track.id, e)}
              ondblclick={() => playFromIndex(index)}
              role="option"
              aria-selected={selection.has(track.id)}
              data-track-id={track.id}
            >
              <div class="col-number">{index + 1}</div>
              <div class="col-title">
                <span class="track-name">{track.title || track.fileName}</span>
              </div>
              <div class="col-artist">{track.artist || m.common.unknownArtist}</div>
              <div class="col-album">{track.album || m.common.unknownAlbum}</div>
              <div class="col-duration">{formatDuration(track.duration)}</div>
              <div class="col-actions">
                <button
                  class="btn-remove-track"
                  onclick={() => handleRemoveTrack(track.id)}
                  title={m.playlists.removeFromPlaylist}
                >
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="icon-remove"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                  >
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>
            </div>
          {/snippet}
        </VirtualList>
      {/if}
    </div>
  {/if}
</div>

<style>
  @reference "../../../../app.css";
  .playlist-detail-page {
    @apply h-full flex flex-col;
  }

  .no-selection,
  .loading {
    @apply flex-1 flex flex-col items-center justify-center text-text-muted text-center p-8;
  }

  .no-selection h2 {
    @apply m-0 mb-2 text-2xl text-text-secondary;
  }

  .no-selection p {
    @apply m-0 text-text-dimmed;
  }

  .back-link {
    @apply mt-4 text-primary hover:underline;
  }

  /* プレイリストヘッダー */
  .playlist-header {
    @apply flex items-center justify-between p-6 rounded-lg mb-4;
    background: linear-gradient(135deg, var(--color-base-200) 0%, var(--color-base-300) 100%);
  }

  .playlist-info {
    @apply flex items-center gap-4;
  }

  .playlist-icon {
    @apply w-20 h-20 flex items-center justify-center rounded-lg;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  }

  .icon-playlist {
    @apply w-10 h-10 text-white;
  }

  .playlist-details {
    @apply flex flex-col gap-1;
  }

  .playlist-title {
    @apply m-0 text-2xl font-bold text-text-primary;
  }

  .playlist-meta {
    @apply m-0 text-sm text-text-muted;
  }

  .playlist-actions {
    @apply flex gap-2;
  }

  .btn-play-all {
    @apply flex items-center gap-2 px-6 py-3 bg-success text-white border-none rounded-full text-sm font-semibold cursor-pointer transition-all;
  }

  .btn-play-all:hover:not(:disabled) {
    @apply bg-success/80 scale-102;
  }

  .btn-play-all:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .icon-play {
    @apply w-5 h-5;
  }

  .btn-delete {
    @apply flex items-center justify-center w-10 h-10 p-0 bg-transparent border border-border rounded-full text-text-muted cursor-pointer transition-all;
  }

  .btn-delete:hover {
    @apply bg-error/10 border-error text-error;
  }

  .icon-delete {
    @apply w-5 h-5;
  }

  /* トラックリスト（スクロールはVirtualListが行う） */
  .track-list {
    @apply flex-1 min-h-0;
  }

  /* 列の見出し（VirtualListが一覧の上に固定して表示する） */
  .track-header {
    @apply grid gap-4 px-4 py-3 text-xs font-semibold uppercase text-text-muted border-b border-border bg-base-100;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
  }

  .track-row {
    @apply grid gap-4 px-4 py-3 items-center cursor-pointer rounded transition-colors select-none;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
  }

  /* ファイルが見つからない曲（再生できない）は薄く表示する */
  .track-row.missing {
    @apply opacity-50;
  }

  .track-row.selected {
    @apply bg-primary/20;
  }

  .col-number {
    @apply text-center text-text-muted text-sm;
  }

  .col-title {
    @apply overflow-hidden;
  }

  .track-name {
    @apply block truncate text-text-primary;
  }

  .col-artist,
  .col-album {
    @apply truncate text-text-secondary text-sm;
  }

  .col-duration {
    @apply text-right text-text-muted text-sm;
  }

  .col-actions {
    @apply flex justify-center;
  }

  .btn-remove-track {
    @apply flex items-center justify-center w-7 h-7 p-0 bg-transparent border-none text-text-dimmed cursor-pointer rounded opacity-0 transition-all;
  }

  .track-row:hover .btn-remove-track {
    @apply opacity-100;
  }

  .btn-remove-track:hover {
    @apply bg-error/10 text-error;
  }

  .icon-remove {
    @apply w-4 h-4;
  }

  /* 空のプレイリスト */
  .empty-playlist {
    @apply flex flex-col items-center justify-center py-16 text-center text-text-muted;
  }

  .icon-empty {
    @apply w-16 h-16 mb-4 opacity-50;
  }

  .empty-playlist p {
    @apply m-1;
  }

  .empty-playlist .hint {
    @apply text-sm text-text-dimmed;
  }
</style>
