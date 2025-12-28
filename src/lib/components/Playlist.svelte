<script lang="ts">
  import {
    usePlaylistsQuery,
    useCreatePlaylistMutation,
    useAddTrackToPlaylistMutation,
    useRemoveTrackFromPlaylistMutation,
    useReorderPlaylistTracksMutation
  } from '$lib/queries/playlists';
  import { useTracksQuery } from '$lib/queries/tracks';
  import type { Playlist, Track } from '$lib/types/models';
  import { validatePlaylistName, toSafeString } from '$lib/utils/validation';
  import { playTrackFromQueue } from '$lib/stores/player';

  // クエリとミューテーション
  let playlistsQuery = $derived(usePlaylistsQuery());
  let tracksQuery = $derived(useTracksQuery());
  let createPlaylistMutation = $derived(useCreatePlaylistMutation());
  let addTrackMutation = $derived(useAddTrackToPlaylistMutation());
  let removeTrackMutation = $derived(useRemoveTrackFromPlaylistMutation());
  let reorderTracksMutation = $derived(useReorderPlaylistTracksMutation());

  // 状態管理
  let selectedPlaylist = $state<Playlist | null>(null);
  let showCreateDialog = $state(false);
  let newPlaylistName = $state('');
  let draggedTrackId = $state<string | null>(null);
  let draggedPlaylistTrackId = $state<string | null>(null);

  // プレイリストを選択
  function selectPlaylist(playlist: Playlist) {
    selectedPlaylist = playlist;
  }

  // 新規プレイリスト作成ダイアログを開く
  function openCreateDialog() {
    showCreateDialog = true;
    newPlaylistName = '';
  }

  // 新規プレイリスト作成ダイアログを閉じる
  function closeCreateDialog() {
    showCreateDialog = false;
    newPlaylistName = '';
  }

  // プレイリストを作成
  async function createPlaylist() {
    const trimmedName = newPlaylistName.trim();

    if (!trimmedName) {
      return;
    }

    // プレイリスト名をバリデーション
    const validation = validatePlaylistName(trimmedName);
    if (!validation.valid) {
      alert(validation.error);
      return;
    }

    // 入力をサニタイズ
    const safeName = toSafeString(trimmedName, 100);

    try {
      await createPlaylistMutation.mutateAsync(safeName);
      closeCreateDialog();
    } catch (error) {
      console.error('プレイリストの作成に失敗しました:', error);
      alert('プレイリストの作成に失敗しました');
    }
  }

  // ドラッグオーバー
  function handleDragOver(event: DragEvent) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = draggedTrackId ? 'copy' : 'move';
    }
  }

  // プレイリストにトラックをドロップ
  async function handleDropOnPlaylist(event: DragEvent, playlist: Playlist) {
    event.preventDefault();

    const trackId = event.dataTransfer?.getData('text/plain');
    if (trackId) {
      try {
        await addTrackMutation.mutateAsync({
          playlistId: playlist.id,
          trackId: trackId
        });
      } catch (error) {
        console.error('トラックの追加に失敗しました:', error);
        alert('トラックの追加に失敗しました');
      }
    }
  }

  // プレイリスト内でトラックを並び替え
  async function handleDropOnPlaylistTrack(event: DragEvent, targetTrackId: string) {
    event.preventDefault();

    if (!selectedPlaylist || !draggedPlaylistTrackId) {
      return;
    }

    const tracks = selectedPlaylist.tracks;
    const draggedIndex = tracks.findIndex(
      (t: { trackId: string }) => t.trackId === draggedPlaylistTrackId
    );
    const targetIndex = tracks.findIndex((t: { trackId: string }) => t.trackId === targetTrackId);

    if (draggedIndex === -1 || targetIndex === -1 || draggedIndex === targetIndex) {
      draggedPlaylistTrackId = null;
      return;
    }

    // 新しい順序を作成
    const newOrder = [...tracks];
    const [removed] = newOrder.splice(draggedIndex, 1);
    newOrder.splice(targetIndex, 0, removed);

    try {
      await reorderTracksMutation.mutateAsync({
        playlistId: selectedPlaylist.id,
        trackIds: newOrder.map((t: { trackId: string }) => t.trackId)
      });
    } catch (error) {
      console.error('トラックの並び替えに失敗しました:', error);
      alert('トラックの並び替えに失敗しました');
    }

    draggedPlaylistTrackId = null;
  }

  // プレイリストからトラックを削除
  async function removeTrack(playlistId: string, trackId: string) {
    if (!confirm('このトラックをプレイリストから削除しますか？')) {
      return;
    }

    try {
      await removeTrackMutation.mutateAsync({ playlistId, trackId });
    } catch (error) {
      console.error('トラックの削除に失敗しました:', error);
      alert('トラックの削除に失敗しました');
    }
  }

  // トラックIDからトラック情報を取得
  function getTrackById(trackId: string): Track | undefined {
    return tracksQuery.data?.find((t: Track) => t.id === trackId);
  }

  // 時間をフォーマット
  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(track: Track) {
    if (!selectedPlaylist) return;

    // プレイリストのトラックリストから再生キューを作成
    const playlistTracks = selectedPlaylist.tracks
      .map((pt: { trackId: string }) => getTrackById(pt.trackId))
      .filter((t): t is Track => t !== undefined);

    const trackIndex = playlistTracks.findIndex((t) => t.id === track.id);
    if (trackIndex !== -1) {
      playTrackFromQueue(playlistTracks, trackIndex);
    }
  }
</script>

<div class="playlist-container">
  <!-- プレイリスト一覧 -->
  <div class="playlist-sidebar">
    <div class="sidebar-header">
      <h2>プレイリスト</h2>
      <button class="btn-create" onclick={openCreateDialog}>
        <span>+</span> 新規作成
      </button>
    </div>

    <div class="playlist-list">
      {#if playlistsQuery.isLoading}
        <div class="loading">読み込み中...</div>
      {:else if playlistsQuery.isError}
        <div class="error">プレイリストの読み込みに失敗しました</div>
      {:else if playlistsQuery.data}
        {#each playlistsQuery.data as Playlist[] as playlist (playlist.id)}
          <div
            class="playlist-item"
            class:selected={selectedPlaylist?.id === playlist.id}
            onclick={() => selectPlaylist(playlist)}
            onkeydown={(e) => e.key === 'Enter' && selectPlaylist(playlist)}
            ondragover={handleDragOver}
            ondrop={(e) => handleDropOnPlaylist(e, playlist)}
            role="button"
            tabindex="0"
          >
            <div class="playlist-name">{playlist.name}</div>
            <div class="playlist-count">{playlist.tracks.length} 曲</div>
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- プレイリスト詳細 -->
  <div class="playlist-detail">
    {#if selectedPlaylist}
      <div class="detail-header">
        <h2>{selectedPlaylist.name}</h2>
        <div class="track-count">{selectedPlaylist.tracks.length} 曲</div>
      </div>

      <div class="track-list">
        {#if selectedPlaylist.tracks.length === 0}
          <div class="empty-message">
            <p>このプレイリストにはまだトラックがありません</p>
            <p class="hint">左側のライブラリからトラックをドラッグ&ドロップして追加できます</p>
          </div>
        {:else}
          {#each selectedPlaylist.tracks as playlistTrack, index}
            {@const track = getTrackById(playlistTrack.trackId)}
            {#if track}
              <div
                class="track-item"
                draggable="true"
                ondragstart={() => {
                  draggedPlaylistTrackId = track.id;
                }}
                ondragover={handleDragOver}
                ondrop={(e) => handleDropOnPlaylistTrack(e, track.id)}
                ondblclick={() => handleTrackDoubleClick(track)}
                role="button"
                tabindex="0"
              >
                <div class="track-number">{index + 1}</div>
                <div class="track-info">
                  <div class="track-title">{track.title || track.fileName}</div>
                  <div class="track-artist">{track.artist || '不明なアーティスト'}</div>
                </div>
                <div class="track-album">{track.album || '不明なアルバム'}</div>
                <div class="track-duration">{formatDuration(track.duration)}</div>
                <button
                  class="btn-remove"
                  onclick={() => removeTrack(selectedPlaylist!.id, track.id)}
                  title="削除"
                >
                  ×
                </button>
              </div>
            {/if}
          {/each}
        {/if}
      </div>
    {:else}
      <div class="no-selection">
        <p>プレイリストを選択してください</p>
      </div>
    {/if}
  </div>
</div>

<!-- 新規プレイリスト作成ダイアログ -->
{#if showCreateDialog}
  <div
    class="dialog-overlay"
    onclick={closeCreateDialog}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === 'Escape' && closeCreateDialog()}
  >
    <div
      class="dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
    >
      <h3>新規プレイリスト作成</h3>
      <input
        type="text"
        bind:value={newPlaylistName}
        placeholder="プレイリスト名"
        class="input-name"
        onkeydown={(e) => e.key === 'Enter' && createPlaylist()}
      />
      <div class="dialog-actions">
        <button class="btn-cancel" onclick={closeCreateDialog}>キャンセル</button>
        <button
          class="btn-submit"
          onclick={createPlaylist}
          disabled={!newPlaylistName.trim() || createPlaylistMutation.isPending}
        >
          {createPlaylistMutation.isPending ? '作成中...' : '作成'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .playlist-container {
    display: flex;
    height: 100%;
    gap: 1rem;
  }

  .playlist-sidebar {
    width: 250px;
    border-right: 1px solid #e5e7eb;
    display: flex;
    flex-direction: column;
  }

  .sidebar-header {
    padding: 1rem;
    border-bottom: 1px solid #e5e7eb;
  }

  .sidebar-header h2 {
    margin: 0 0 0.5rem 0;
    font-size: 1.25rem;
  }

  .btn-create {
    width: 100%;
    padding: 0.5rem;
    background: #3b82f6;
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.25rem;
  }

  .btn-create:hover {
    background: #2563eb;
  }

  .btn-create span {
    font-size: 1.25rem;
    font-weight: bold;
  }

  .playlist-list {
    flex: 1;
    overflow-y: auto;
  }

  .playlist-item {
    padding: 0.75rem 1rem;
    cursor: pointer;
    border-bottom: 1px solid #f3f4f6;
    transition: background-color 0.2s;
  }

  .playlist-item:hover {
    background: #f9fafb;
  }

  .playlist-item.selected {
    background: #eff6ff;
    border-left: 3px solid #3b82f6;
  }

  .playlist-name {
    font-weight: 500;
    margin-bottom: 0.25rem;
  }

  .playlist-count {
    font-size: 0.75rem;
    color: #6b7280;
  }

  .playlist-detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .detail-header {
    padding: 1rem;
    border-bottom: 1px solid #e5e7eb;
  }

  .detail-header h2 {
    margin: 0 0 0.25rem 0;
    font-size: 1.5rem;
  }

  .track-count {
    font-size: 0.875rem;
    color: #6b7280;
  }

  .track-list {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
  }

  .track-item {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem;
    border-radius: 0.375rem;
    cursor: move;
    transition: background-color 0.2s;
  }

  .track-item:hover {
    background: #f9fafb;
  }

  .track-number {
    width: 2rem;
    text-align: center;
    color: #6b7280;
    font-size: 0.875rem;
  }

  .track-info {
    flex: 1;
    min-width: 0;
  }

  .track-title {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-artist {
    font-size: 0.875rem;
    color: #6b7280;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-album {
    flex: 0 0 200px;
    font-size: 0.875rem;
    color: #6b7280;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-duration {
    flex: 0 0 60px;
    text-align: right;
    font-size: 0.875rem;
    color: #6b7280;
  }

  .btn-remove {
    flex: 0 0 auto;
    width: 2rem;
    height: 2rem;
    border: none;
    background: transparent;
    color: #6b7280;
    font-size: 1.5rem;
    cursor: pointer;
    border-radius: 0.25rem;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .btn-remove:hover {
    background: #fee2e2;
    color: #dc2626;
  }

  .empty-message {
    text-align: center;
    padding: 3rem 1rem;
    color: #6b7280;
  }

  .empty-message p {
    margin: 0.5rem 0;
  }

  .hint {
    font-size: 0.875rem;
  }

  .no-selection {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #6b7280;
  }

  .loading,
  .error {
    padding: 1rem;
    text-align: center;
    color: #6b7280;
  }

  .error {
    color: #dc2626;
  }

  /* ダイアログ */
  .dialog-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: white;
    padding: 1.5rem;
    border-radius: 0.5rem;
    width: 400px;
    max-width: 90%;
  }

  .dialog h3 {
    margin: 0 0 1rem 0;
    font-size: 1.25rem;
  }

  .input-name {
    width: 100%;
    padding: 0.5rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    font-size: 1rem;
    margin-bottom: 1rem;
  }

  .input-name:focus {
    outline: none;
    border-color: #3b82f6;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
  }

  .dialog-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  .btn-cancel,
  .btn-submit {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-cancel {
    background: #f3f4f6;
    color: #374151;
  }

  .btn-cancel:hover {
    background: #e5e7eb;
  }

  .btn-submit {
    background: #3b82f6;
    color: white;
  }

  .btn-submit:hover:not(:disabled) {
    background: #2563eb;
  }

  .btn-submit:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
