<script lang="ts">
  import { onMount } from 'svelte';
  import type { Track, Playlist } from '$lib/types/models';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation } from '$lib/queries/playlists';
  import { playSingleTrack, playQueue, currentTrackIndex } from '$lib/stores/player';
  import { get } from 'svelte/store';

  // Props
  interface Props {
    x: number;
    y: number;
    track: Track;
    tracks?: Track[];
    selectedTrackIds?: Set<string>;
    onClose: () => void;
    onEditMetadata?: () => void;
    onAddToQueue?: () => void;
    onPlayNext?: () => void;
    onDelete?: () => void;
  }

  let {
    x,
    y,
    track,
    tracks = [],
    selectedTrackIds = new Set(),
    onClose,
    onEditMetadata,
    onAddToQueue,
    onPlayNext,
    onDelete
  }: Props = $props();

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const addTrackMutation = useAddTrackToPlaylistMutation();

  // サブメニュー表示状態
  let showPlaylistSubmenu = $state(false);
  let menuElement: HTMLDivElement | null = null;

  // メニュー位置の調整
  let adjustedX = $state(0);
  let adjustedY = $state(0);

  // propsからの初期位置を設定
  $effect(() => {
    adjustedX = x;
    adjustedY = y;
  });

  // 選択されたトラックの数
  const selectedCount = $derived(selectedTrackIds.size > 0 ? selectedTrackIds.size : 1);

  // 選択されたトラックのリスト
  const selectedTracks = $derived.by(() => {
    if (selectedTrackIds.size > 0 && tracks.length > 0) {
      return tracks.filter((t) => selectedTrackIds.has(t.id));
    }
    return [track];
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
   * トラックを再生
   */
  function handlePlay() {
    if (selectedTracks.length > 0) {
      playSingleTrack(selectedTracks[0]);
    }
    onClose();
  }

  /**
   * 次に再生（キューの先頭に追加）
   */
  function handlePlayNext() {
    if (onPlayNext) {
      onPlayNext();
    } else {
      // デフォルト動作: キューの現在位置の次に挿入
      const queue = get(playQueue);
      const currentIndex = get(currentTrackIndex);

      const newQueue = [...queue];
      newQueue.splice(currentIndex + 1, 0, ...selectedTracks);
      playQueue.set(newQueue);
    }
    onClose();
  }

  /**
   * キューに追加（キューの最後に追加）
   */
  function handleAddToQueue() {
    if (onAddToQueue) {
      onAddToQueue();
    } else {
      const queue = get(playQueue);
      playQueue.set([...queue, ...selectedTracks]);
    }
    onClose();
  }

  /**
   * プレイリストに追加
   */
  async function handleAddToPlaylist(playlist: Playlist) {
    for (const t of selectedTracks) {
      await addTrackMutation.mutateAsync({
        playlistId: playlist.id,
        trackId: t.id
      });
    }
    onClose();
  }

  /**
   * メタデータを編集
   */
  function handleEditMetadata() {
    if (onEditMetadata) {
      onEditMetadata();
    }
    onClose();
  }

  /**
   * ファイルの場所を開く（Tauriコマンドが必要）
   */
  async function handleShowInFolder() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('show_in_folder', { path: track.filePath });
    } catch (error) {
      console.error('ファイルの場所を開けませんでした:', error);
    }
    onClose();
  }

  /**
   * トラックを削除
   */
  function handleDelete() {
    if (onDelete) {
      onDelete();
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
  {#if selectedCount > 1}
    <div class="menu-header">{selectedCount}曲を選択中</div>
    <div class="menu-divider"></div>
  {/if}

  <button class="menu-item" onclick={handlePlay} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>再生</span>
  </button>

  <button class="menu-item" onclick={handlePlayNext} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M13 5l7 7-7 7M5 5l7 7-7 7"
      />
    </svg>
    <span>次に再生</span>
  </button>

  <button class="menu-item" onclick={handleAddToQueue} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4 6h16M4 10h16M4 14h16M4 18h7"
      />
    </svg>
    <span>キューに追加</span>
  </button>

  <div class="menu-divider"></div>

  <div
    class="menu-item submenu-trigger"
    onmouseenter={() => (showPlaylistSubmenu = true)}
    onmouseleave={() => (showPlaylistSubmenu = false)}
    role="menuitem"
    tabindex="0"
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
    </svg>
    <span>プレイリストに追加</span>
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-arrow"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>

    {#if showPlaylistSubmenu}
      <div class="submenu">
        {#if playlistsQuery.isLoading}
          <div class="menu-message">読み込み中...</div>
        {:else if playlistsQuery.data && playlistsQuery.data.length > 0}
          {#each playlistsQuery.data as playlist (playlist.id)}
            <button class="menu-item" onclick={() => handleAddToPlaylist(playlist)} role="menuitem">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="menu-icon"
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
              <span>{playlist.name}</span>
            </button>
          {/each}
        {:else}
          <div class="menu-message">プレイリストがありません</div>
        {/if}
      </div>
    {/if}
  </div>

  <div class="menu-divider"></div>

  {#if onEditMetadata}
    <button class="menu-item" onclick={handleEditMetadata} role="menuitem">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
        />
      </svg>
      <span>メタデータを編集</span>
      <span class="menu-shortcut">Ctrl+I</span>
    </button>
  {/if}

  <button class="menu-item" onclick={handleShowInFolder} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
      />
    </svg>
    <span>ファイルの場所を開く</span>
  </button>

  {#if onDelete}
    <div class="menu-divider"></div>

    <button class="menu-item menu-item-danger" onclick={handleDelete} role="menuitem">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
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
      <span>削除...</span>
    </button>
  {/if}
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
    @apply py-2 px-4 text-xs font-semibold text-text-muted uppercase;
  }

  .menu-item {
    @apply flex items-center gap-3 w-full py-2 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover {
    @apply bg-surface-active;
  }

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
  }

  .menu-shortcut {
    @apply text-xs text-text-dimmed shrink-0;
    flex: none !important;
  }

  .menu-arrow {
    @apply w-3 h-3 shrink-0 text-text-dimmed;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }

  .submenu-trigger {
    @apply relative;
  }

  .submenu {
    @apply absolute left-full top-0 min-w-[180px] bg-base-300 border border-border rounded-lg py-2 ml-1;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
  }

  .menu-message {
    @apply py-2 px-4 text-sm text-text-dimmed;
  }

  .menu-item-danger {
    @apply text-error;
  }

  .menu-item-danger:hover {
    @apply bg-error/10;
  }
</style>
