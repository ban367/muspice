<script lang="ts">
  import { onMount } from 'svelte';
  import type { Track, Playlist, AlbumGroup, ArtistGroup, GenreGroup } from '$lib/types/models';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation } from '$lib/queries/playlists';
  import { playTrackFromQueue, playQueue, currentTrackIndex } from '$lib/stores/player';
  import { get } from 'svelte/store';

  // グループタイプ
  type GroupType = 'album' | 'artist' | 'genre';
  type Group = AlbumGroup | ArtistGroup | GenreGroup;

  // Props
  interface Props {
    x: number;
    y: number;
    group: Group;
    type: GroupType;
    onClose: () => void;
  }

  let { x, y, group, type, onClose }: Props = $props();

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

  // グループ内のすべてのトラック
  const allTracks = $derived.by((): Track[] => {
    if (type === 'artist') {
      return (group as ArtistGroup).albums.flatMap((album) => album.tracks);
    } else {
      return (group as AlbumGroup | GenreGroup).tracks;
    }
  });

  // タイプに応じたラベル
  const typeLabel = $derived.by(() => {
    switch (type) {
      case 'album':
        return 'アルバム';
      case 'artist':
        return 'アーティスト';
      case 'genre':
        return 'ジャンル';
    }
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
   * すべて再生
   */
  function handlePlayAll() {
    if (allTracks.length > 0) {
      playTrackFromQueue(allTracks, 0);
    }
    onClose();
  }

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    if (allTracks.length > 0) {
      const shuffled = [...allTracks].sort(() => Math.random() - 0.5);
      playTrackFromQueue(shuffled, 0);
    }
    onClose();
  }

  /**
   * 次に再生（キューの先頭に追加）
   */
  function handlePlayNext() {
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);

    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...allTracks);
    playQueue.set(newQueue);
    onClose();
  }

  /**
   * キューに追加（キューの最後に追加）
   */
  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...allTracks]);
    onClose();
  }

  /**
   * プレイリストに追加
   */
  async function handleAddToPlaylist(playlist: Playlist) {
    for (const track of allTracks) {
      await addTrackMutation.mutateAsync({
        playlistId: playlist.id,
        trackId: track.id
      });
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
  <div class="menu-header">{group.name}</div>
  <div class="menu-subheader">{allTracks.length}曲</div>
  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handlePlayAll} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>{typeLabel}を再生</span>
  </button>

  <button class="menu-item" onclick={handleShufflePlay} role="menuitem">
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
        d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
      />
    </svg>
    <span>シャッフル再生</span>
  </button>

  <div class="menu-divider"></div>

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

  .menu-item:hover {
    @apply bg-surface-active;
  }

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
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
</style>
