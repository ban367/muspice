<script lang="ts">
  import {
    usePlaylistsQuery,
    useCreatePlaylistMutation,
    useDeletePlaylistMutation,
    useAddTrackToPlaylistMutation,
    useRemoveTrackFromPlaylistMutation,
    useReorderPlaylistTracksMutation
  } from '$lib/queries/playlists';
  import { useTracksQuery } from '$lib/queries/tracks';
  import type { Playlist, Track } from '$lib/types/models';
  import { validatePlaylistName, toSafeString } from '$lib/utils/validation';
  import { formatDuration } from '$lib/utils/format';
  import { playTrackFromQueue, currentTrack } from '$lib/stores/player';
  import PlayingIndicator from './library/PlayingIndicator.svelte';

  // クエリとミューテーション
  let playlistsQuery = $derived(usePlaylistsQuery());
  let tracksQuery = $derived(useTracksQuery());
  let createPlaylistMutation = $derived(useCreatePlaylistMutation());
  let deletePlaylistMutation = $derived(useDeletePlaylistMutation());
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

  // プレイリストを削除
  async function deletePlaylist(playlist: Playlist) {
    if (!confirm(`プレイリスト「${playlist.name}」を削除しますか？\nこの操作は取り消せません。`)) {
      return;
    }

    try {
      await deletePlaylistMutation.mutateAsync(playlist.id);
      // 削除したプレイリストが選択されていた場合は選択解除
      if (selectedPlaylist?.id === playlist.id) {
        selectedPlaylist = null;
      }
    } catch (error) {
      console.error('プレイリストの削除に失敗しました:', error);
      alert('プレイリストの削除に失敗しました');
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

<div class="flex h-full gap-4">
  <!-- プレイリスト一覧 -->
  <div class="playlist-sidebar">
    <div class="p-4 border-b border-border">
      <h2 class="text-xl font-semibold text-text-primary m-0 mb-3">プレイリスト</h2>
      <button
        class="btn-primary w-full flex items-center justify-center gap-1"
        onclick={openCreateDialog}
      >
        <span class="text-xl font-bold">+</span> 新規作成
      </button>
    </div>

    <div class="flex-1 overflow-y-auto">
      {#if playlistsQuery.isLoading}
        <div class="p-4 text-center text-text-muted">読み込み中...</div>
      {:else if playlistsQuery.isError}
        <div class="p-4 text-center text-error-light">プレイリストの読み込みに失敗しました</div>
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
            <div class="flex-1 min-w-0">
              <div
                class="text-text-primary font-medium overflow-hidden text-ellipsis whitespace-nowrap"
              >
                {playlist.name}
              </div>
              <div class="text-xs text-text-muted">{playlist.tracks.length} 曲</div>
            </div>
            <button
              class="delete-btn"
              onclick={(e) => {
                e.stopPropagation();
                deletePlaylist(playlist);
              }}
              title="プレイリストを削除"
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
                  d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                />
              </svg>
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- プレイリスト詳細 -->
  <div class="flex-1 flex flex-col overflow-hidden">
    {#if selectedPlaylist}
      <div class="p-4 border-b border-border">
        <h2 class="text-2xl font-semibold text-text-primary m-0 mb-1">{selectedPlaylist.name}</h2>
        <div class="text-sm text-text-muted">{selectedPlaylist.tracks.length} 曲</div>
      </div>

      <div class="flex-1 overflow-y-auto p-4">
        {#if selectedPlaylist.tracks.length === 0}
          <div class="state-container">
            <p>このプレイリストにはまだトラックがありません</p>
            <span>左側のライブラリからトラックをドラッグ&ドロップして追加できます</span>
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
                <div class="track-number">
                  {#if $currentTrack?.id === track.id}
                    <PlayingIndicator size="small" />
                  {:else}
                    {index + 1}
                  {/if}
                </div>
                <div class="flex-1 min-w-0">
                  <div
                    class="text-text-primary font-medium overflow-hidden text-ellipsis whitespace-nowrap"
                  >
                    {track.title || track.fileName}
                  </div>
                  <div
                    class="text-sm text-text-muted overflow-hidden text-ellipsis whitespace-nowrap"
                  >
                    {track.artist || '不明なアーティスト'}
                  </div>
                </div>
                <div class="track-album">{track.album || '不明なアルバム'}</div>
                <div class="track-duration">{formatDuration(track.duration)}</div>
                <button
                  class="remove-btn"
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
      <div class="flex items-center justify-center h-full text-text-muted">
        <p>プレイリストを選択してください</p>
      </div>
    {/if}
  </div>
</div>

<!-- 新規プレイリスト作成ダイアログ -->
{#if showCreateDialog}
  <div
    class="custom-modal-backdrop"
    onclick={closeCreateDialog}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === 'Escape' && closeCreateDialog()}
  >
    <div
      class="modal-content max-w-md"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
    >
      <div class="p-6">
        <h3 class="text-xl font-semibold text-text-primary m-0 mb-4">新規プレイリスト作成</h3>
        <input
          type="text"
          bind:value={newPlaylistName}
          placeholder="プレイリスト名"
          class="form-input mb-4"
          onkeydown={(e) => e.key === 'Enter' && createPlaylist()}
        />
        <div class="flex gap-2 justify-end">
          <button class="btn-secondary" onclick={closeCreateDialog}>キャンセル</button>
          <button
            class="btn-primary"
            onclick={createPlaylist}
            disabled={!newPlaylistName.trim() || createPlaylistMutation.isPending}
          >
            {createPlaylistMutation.isPending ? '作成中...' : '作成'}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  @reference "../../app.css";
  .playlist-sidebar {
    @apply w-64 border-r border-border flex flex-col;
  }

  .playlist-item {
    @apply flex items-center justify-between py-3 px-4 cursor-pointer border-b border-border/50 transition-colors;
  }

  .playlist-item:hover {
    @apply bg-surface-hover;
  }

  .playlist-item.selected {
    @apply bg-primary/10 border-l-3 border-l-primary;
  }

  .delete-btn {
    @apply shrink-0 w-7 h-7 p-0 border-none bg-transparent text-text-muted cursor-pointer rounded flex items-center justify-center opacity-0 transition-all;
  }

  .playlist-item:hover .delete-btn {
    @apply opacity-100;
  }

  .delete-btn:hover {
    @apply bg-error/20 text-error-light;
  }

  .track-item {
    @apply flex items-center gap-4 py-3 px-3 rounded-md cursor-move transition-colors;
  }

  .track-item:hover {
    @apply bg-surface-hover;
  }

  .track-number {
    @apply w-8 text-center text-text-muted text-sm;
  }

  .track-album {
    @apply flex-[0_0_200px] text-sm text-text-muted overflow-hidden text-ellipsis whitespace-nowrap;
  }

  .track-duration {
    @apply flex-[0_0_60px] text-right text-sm text-text-muted;
  }

  .remove-btn {
    @apply flex-[0_0_auto] w-8 h-8 border-none bg-transparent text-text-muted text-2xl cursor-pointer rounded flex items-center justify-center transition-all;
  }

  .remove-btn:hover {
    @apply bg-error/20 text-error-light;
  }
</style>
