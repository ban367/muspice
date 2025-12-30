<script lang="ts">
  import { onMount } from 'svelte';
  import type { Playlist } from '$lib/types/models';
  import { useDeletePlaylistMutation, useRenamePlaylistMutation } from '$lib/queries/playlists';
  import { playTrackFromQueue, playQueue, currentTrackIndex } from '$lib/stores/player';
  import { useTracksQuery } from '$lib/queries/tracks';
  import { get } from 'svelte/store';

  // Props
  interface Props {
    x: number;
    y: number;
    playlist: Playlist;
    onClose: () => void;
  }

  let { x, y, playlist, onClose }: Props = $props();

  // ミューテーション
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const renamePlaylistMutation = useRenamePlaylistMutation();

  // トラッククエリ（プレイリスト内のトラック情報取得用）
  const tracksQuery = useTracksQuery();

  let menuElement: HTMLDivElement | null = null;

  // メニュー位置の調整
  let adjustedX = $state(x);
  let adjustedY = $state(y);

  // プレイリスト内のトラック
  const playlistTracks = $derived.by(() => {
    if (!tracksQuery.data) return [];
    const trackIds = new Set(playlist.tracks.map(t => t.trackId));
    return tracksQuery.data.filter(t => trackIds.has(t.id));
  });

  onMount(() => {
    // メニューが画面外に出ないように位置を調整
    if (menuElement) {
      const rect = menuElement.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;

      if (x + rect.width > viewportWidth) {
        adjustedX = viewportWidth - rect.width - 10;
      }
      if (y + rect.height > viewportHeight) {
        adjustedY = viewportHeight - rect.height - 10;
      }
    }

    // クリックイベントでメニューを閉じる
    const handleClick = (e: MouseEvent) => {
      if (menuElement && !menuElement.contains(e.target as Node)) {
        onClose();
      }
    };

    // Escキーでメニューを閉じる
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('click', handleClick);
    document.addEventListener('keydown', handleKeydown);

    return () => {
      document.removeEventListener('click', handleClick);
      document.removeEventListener('keydown', handleKeydown);
    };
  });

  /**
   * プレイリストを再生
   */
  function handlePlay() {
    if (playlistTracks.length > 0) {
      playTrackFromQueue(playlistTracks, 0);
    }
    onClose();
  }

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    if (playlistTracks.length > 0) {
      const shuffled = [...playlistTracks].sort(() => Math.random() - 0.5);
      playTrackFromQueue(shuffled, 0);
    }
    onClose();
  }

  /**
   * キューに追加
   */
  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...playlistTracks]);
    onClose();
  }

  /**
   * 次に再生
   */
  function handlePlayNext() {
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);

    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...playlistTracks);
    playQueue.set(newQueue);
    onClose();
  }

  /**
   * プレイリストの名前を変更
   */
  function handleRename() {
    const newName = prompt('新しいプレイリスト名を入力してください', playlist.name);
    if (newName && newName.trim() && newName !== playlist.name) {
      renamePlaylistMutation.mutate({ playlistId: playlist.id, name: newName.trim() });
    }
    onClose();
  }

  /**
   * プレイリストを削除
   */
  function handleDelete() {
    if (confirm(`プレイリスト「${playlist.name}」を削除しますか？`)) {
      deletePlaylistMutation.mutate(playlist.id);
    }
    onClose();
  }
</script>

<div
  class="context-menu"
  bind:this={menuElement}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  role="menu"
>
  <div class="menu-header">{playlist.name}</div>
  <div class="menu-subheader">{playlist.tracks.length}曲</div>
  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handlePlay} role="menuitem" disabled={playlistTracks.length === 0}>
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" viewBox="0 0 24 24" fill="currentColor">
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>プレイリストを再生</span>
  </button>

  <button class="menu-item" onclick={handleShufflePlay} role="menuitem" disabled={playlistTracks.length === 0}>
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
    </svg>
    <span>シャッフル再生</span>
  </button>

  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handlePlayNext} role="menuitem" disabled={playlistTracks.length === 0}>
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
    </svg>
    <span>次に再生</span>
  </button>

  <button class="menu-item" onclick={handleAddToQueue} role="menuitem" disabled={playlistTracks.length === 0}>
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h7" />
    </svg>
    <span>キューに追加</span>
  </button>

  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handleRename} role="menuitem">
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
    </svg>
    <span>名前を変更</span>
  </button>

  <button class="menu-item menu-item-danger" onclick={handleDelete} role="menuitem">
    <svg xmlns="http://www.w3.org/2000/svg" class="menu-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
    </svg>
    <span>削除</span>
  </button>
</div>

<style>
@reference "../../app.css";
  .context-menu {
    @apply fixed z-[10000] min-w-[200px] bg-base-300 border border-border rounded-lg py-2;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
    animation: fadeIn 0.1s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .menu-header {
    @apply py-1 px-4 text-sm font-semibold text-text-primary;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-subheader {
    @apply pb-2 px-4 text-xs text-text-muted;
  }

  .menu-item {
    @apply flex items-center gap-3 w-full py-2 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover:not(:disabled) {
    @apply bg-surface-active;
  }

  .menu-item:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .menu-item-danger {
    @apply text-error-light;
  }

  .menu-item-danger:hover:not(:disabled) {
    @apply bg-error-light/10;
  }

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }
</style>
