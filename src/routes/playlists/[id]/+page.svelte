<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    usePlaylistsQuery,
    useDeletePlaylistMutation,
    useRemoveTrackFromPlaylistMutation,
    useReorderPlaylistTracksMutation
  } from '$lib/queries/playlists';
  import { useTracksQuery } from '$lib/queries/tracks';
  import type { Playlist, Track } from '$lib/types/models';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { formatDuration, formatTotalDuration } from '$lib/utils/format';

  // URLからプレイリストIDを取得
  const playlistId = $derived($page.params.id);

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const tracksQuery = useTracksQuery();
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const removeTrackMutation = useRemoveTrackFromPlaylistMutation();
  const reorderTracksMutation = useReorderPlaylistTracksMutation();

  // 選択されたプレイリスト
  const selectedPlaylist = $derived.by(() => {
    if (!playlistId || !playlistsQuery.data) return null;
    return playlistsQuery.data.find((p: Playlist) => p.id === playlistId) || null;
  });

  // ドラッグ中のトラックID
  let draggedTrackId = $state<string | null>(null);

  /**
   * トラックIDからトラック情報を取得
   */
  function getTrackById(trackId: string): Track | undefined {
    return tracksQuery.data?.find((t: Track) => t.id === trackId);
  }

  /**
   * プレイリストを削除
   */
  async function handleDeletePlaylist() {
    if (!selectedPlaylist) return;

    if (
      !confirm(
        `プレイリスト「${selectedPlaylist.name}」を削除しますか？\nこの操作は取り消せません。`
      )
    ) {
      return;
    }

    try {
      await deletePlaylistMutation.mutateAsync(selectedPlaylist.id);
      goto('/playlists');
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
   * トラックをダブルクリックで再生
   */
  function handleTrackDoubleClick(track: Track) {
    if (!selectedPlaylist) return;

    // プレイリストのトラックリストから再生キューを作成
    const playlistTracks = selectedPlaylist.tracks
      .map((pt) => getTrackById(pt.trackId))
      .filter((t): t is Track => t !== undefined);

    const trackIndex = playlistTracks.findIndex((t) => t.id === track.id);
    if (trackIndex !== -1) {
      playTrackFromQueue(playlistTracks, trackIndex);
    }
  }

  /**
   * プレイリストの合計時間を計算
   */
  const totalDuration = $derived.by(() => {
    if (!selectedPlaylist) return 0;
    return selectedPlaylist.tracks.reduce((total, pt) => {
      const track = getTrackById(pt.trackId);
      return total + (track?.duration || 0);
    }, 0);
  });
</script>

<div class="playlist-detail-page">
  {#if playlistsQuery.isLoading}
    <div class="loading">読み込み中...</div>
  {:else if !selectedPlaylist}
    <div class="no-selection">
      <h2>プレイリストが見つかりません</h2>
      <p>選択されたプレイリストは存在しないか、削除された可能性があります</p>
      <a href="/playlists" class="back-link">プレイリスト一覧に戻る</a>
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
            {selectedPlaylist.tracks.length}曲 • {formatTotalDuration(totalDuration)}
          </p>
        </div>
      </div>
      <div class="playlist-actions">
        <button
          class="btn-play-all"
          onclick={() => {
            if (selectedPlaylist && selectedPlaylist.tracks.length > 0) {
              const playlistTracks = selectedPlaylist.tracks
                .map((pt) => getTrackById(pt.trackId))
                .filter((t): t is Track => t !== undefined);
              if (playlistTracks.length > 0) {
                playTrackFromQueue(playlistTracks, 0);
              }
            }
          }}
          disabled={selectedPlaylist.tracks.length === 0}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="icon-play"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M8 5v14l11-7z" />
          </svg>
          すべて再生
        </button>
        <button class="btn-delete" onclick={handleDeletePlaylist} title="プレイリストを削除">
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
          <p>このプレイリストにはまだトラックがありません</p>
          <p class="hint">ライブラリからトラックをドラッグ&ドロップして追加できます</p>
        </div>
      {:else}
        <!-- テーブルヘッダー -->
        <div class="track-header">
          <div class="col-number">#</div>
          <div class="col-title">タイトル</div>
          <div class="col-artist">アーティスト</div>
          <div class="col-album">アルバム</div>
          <div class="col-duration">時間</div>
          <div class="col-actions"></div>
        </div>

        <!-- トラック一覧 -->
        {#each selectedPlaylist.tracks as playlistTrack, index (playlistTrack.trackId)}
          {@const track = getTrackById(playlistTrack.trackId)}
          {#if track}
            <div
              class="track-row"
              draggable="true"
              ondragstart={() => {
                draggedTrackId = track.id;
              }}
              ondragover={handleDragOver}
              ondrop={(e) => handleDropOnTrack(e, track.id)}
              ondblclick={() => handleTrackDoubleClick(track)}
              role="button"
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
            >
              <div class="col-number">{index + 1}</div>
              <div class="col-title">
                <span class="track-name">{track.title || track.fileName}</span>
              </div>
              <div class="col-artist">{track.artist || '不明なアーティスト'}</div>
              <div class="col-album">{track.album || '不明なアルバム'}</div>
              <div class="col-duration">{formatDuration(track.duration)}</div>
              <div class="col-actions">
                <button
                  class="btn-remove-track"
                  onclick={() => handleRemoveTrack(track.id)}
                  title="プレイリストから削除"
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
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  @reference "../../../app.css";
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
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
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
    @apply m-0 text-2xl font-bold text-white;
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

  /* トラックリスト */
  .track-list {
    @apply flex-1 overflow-y-auto;
  }

  .track-header {
    @apply grid gap-4 px-4 py-3 text-xs font-semibold uppercase text-text-muted border-b border-border;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
  }

  .track-row {
    @apply grid gap-4 px-4 py-3 items-center cursor-pointer rounded transition-colors;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
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
