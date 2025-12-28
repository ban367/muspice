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

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const tracksQuery = useTracksQuery();
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const removeTrackMutation = useRemoveTrackFromPlaylistMutation();
  const reorderTracksMutation = useReorderPlaylistTracksMutation();

  // 選択されたプレイリストID
  const selectedPlaylistId = $derived($page.url.searchParams.get('id'));

  // 選択されたプレイリスト
  const selectedPlaylist = $derived.by(() => {
    if (!selectedPlaylistId || !playlistsQuery.data) return null;
    return playlistsQuery.data.find((p: Playlist) => p.id === selectedPlaylistId) || null;
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
   * 再生時間をフォーマット
   */
  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`; 
  }

  /**
   * プレイリストを削除
   */
  async function handleDeletePlaylist() {
    if (!selectedPlaylist) return;
    
    if (!confirm(`プレイリスト「${selectedPlaylist.name}」を削除しますか？\nこの操作は取り消せません。`)) {
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

  /**
   * 合計時間をフォーマット
   */
  function formatTotalDuration(seconds: number): string {
    if (!seconds) return '0分';
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    if (hours > 0) {
      return `${hours}時間${mins}分`;
    }
    return `${mins}分`;
  }
</script>

<div class="playlists-page">
  {#if !selectedPlaylistId}
    <!-- プレイリスト未選択時 -->
    <div class="no-selection">
      <svg xmlns="http://www.w3.org/2000/svg" class="icon-large" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
      </svg>
      <h2>プレイリストを選択</h2>
      <p>左側のサイドバーからプレイリストを選択してください</p>
    </div>
  {:else if playlistsQuery.isLoading}
    <div class="loading">読み込み中...</div>
  {:else if !selectedPlaylist}
    <div class="no-selection">
      <h2>プレイリストが見つかりません</h2>
      <p>選択されたプレイリストは存在しないか、削除された可能性があります</p>
    </div>
  {:else}
    <!-- プレイリスト詳細 -->
    <div class="playlist-header">
      <div class="playlist-info">
        <div class="playlist-icon">
          <svg xmlns="http://www.w3.org/2000/svg" class="icon-playlist" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
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
          <svg xmlns="http://www.w3.org/2000/svg" class="icon-play" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
          すべて再生
        </button>
        <button class="btn-delete" onclick={handleDeletePlaylist} title="プレイリストを削除">
          <svg xmlns="http://www.w3.org/2000/svg" class="icon-delete" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    </div>

    <div class="track-list">
      {#if selectedPlaylist.tracks.length === 0}
        <div class="empty-playlist">
          <svg xmlns="http://www.w3.org/2000/svg" class="icon-empty" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
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
              ondragstart={() => { draggedTrackId = track.id; }}
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
                  <svg xmlns="http://www.w3.org/2000/svg" class="icon-remove" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
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
  .playlists-page {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .no-selection,
  .loading {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  .icon-large {
    width: 4rem;
    height: 4rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }

  .no-selection h2 {
    margin: 0 0 0.5rem 0;
    font-size: 1.5rem;
    color: #ccc;
  }

  .no-selection p {
    margin: 0;
    color: #666;
  }

  /* プレイリストヘッダー */
  .playlist-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1.5rem;
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  .playlist-info {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .playlist-icon {
    width: 5rem;
    height: 5rem;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    border-radius: 0.5rem;
  }

  .icon-playlist {
    width: 2.5rem;
    height: 2.5rem;
    color: #fff;
  }

  .playlist-details {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .playlist-title {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 700;
    color: #fff;
  }

  .playlist-meta {
    margin: 0;
    font-size: 0.875rem;
    color: #aaa;
  }

  .playlist-actions {
    display: flex;
    gap: 0.5rem;
  }

  .btn-play-all {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1.5rem;
    background-color: #1db954;
    color: #fff;
    border: none;
    border-radius: 2rem;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-play-all:hover:not(:disabled) {
    background-color: #1ed760;
    transform: scale(1.02);
  }

  .btn-play-all:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .icon-play {
    width: 1.25rem;
    height: 1.25rem;
  }

  .btn-delete {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    padding: 0;
    background: transparent;
    border: 1px solid #444;
    border-radius: 50%;
    color: #888;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-delete:hover {
    background-color: #fee2e2;
    border-color: #dc2626;
    color: #dc2626;
  }

  .icon-delete {
    width: 1.25rem;
    height: 1.25rem;
  }

  /* トラックリスト */
  .track-list {
    flex: 1;
    overflow-y: auto;
  }

  .track-header {
    display: grid;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
    gap: 1rem;
    padding: 0.75rem 1rem;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
    border-bottom: 1px solid #333;
  }

  .track-row {
    display: grid;
    grid-template-columns: 3rem 2fr 1.5fr 1.5fr 4rem 3rem;
    gap: 1rem;
    padding: 0.75rem 1rem;
    align-items: center;
    cursor: pointer;
    border-radius: 0.25rem;
    transition: background-color 0.2s;
  }

  .track-row:hover {
    background-color: rgba(255, 255, 255, 0.05);
  }

  .track-row:active {
    background-color: rgba(255, 255, 255, 0.1);
  }

  .col-number {
    text-align: center;
    color: #888;
    font-size: 0.875rem;
  }

  .col-title {
    overflow: hidden;
  }

  .track-name {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #fff;
  }

  .col-artist,
  .col-album {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #aaa;
    font-size: 0.875rem;
  }

  .col-duration {
    text-align: right;
    color: #888;
    font-size: 0.875rem;
  }

  .col-actions {
    display: flex;
    justify-content: center;
  }

  .btn-remove-track {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    padding: 0;
    background: transparent;
    border: none;
    color: #666;
    cursor: pointer;
    border-radius: 0.25rem;
    opacity: 0;
    transition: all 0.2s;
  }

  .track-row:hover .btn-remove-track {
    opacity: 1;
  }

  .btn-remove-track:hover {
    background-color: #fee2e2;
    color: #dc2626;
  }

  .icon-remove {
    width: 1rem;
    height: 1rem;
  }

  /* 空のプレイリスト */
  .empty-playlist {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 4rem 2rem;
    text-align: center;
    color: #888;
  }

  .icon-empty {
    width: 4rem;
    height: 4rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }

  .empty-playlist p {
    margin: 0.25rem 0;
  }

  .empty-playlist .hint {
    font-size: 0.875rem;
    color: #666;
  }

  /* ダークモード */
  @media (prefers-color-scheme: dark) {
    .track-header {
      border-bottom-color: #333;
    }
  }
</style>
