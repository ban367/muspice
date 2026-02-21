<!--
  @component PlaylistSubmenu
  プレイリストに追加するためのサブメニューコンポーネント。
  ContextMenuとGroupContextMenuで共通のプレイリスト追加サブメニューを独立化。
-->
<script lang="ts">
  import type { Track, Playlist } from '$lib/types/models';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation } from '$lib/queries/playlists';

  // Props
  interface Props {
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const addTrackMutation = useAddTrackToPlaylistMutation();

  // サブメニュー表示状態
  let showSubmenu = $state(false);

  /**
   * プレイリストに追加
   */
  async function handleAddToPlaylist(playlist: Playlist) {
    for (const t of tracks) {
      await addTrackMutation.mutateAsync({
        playlistId: playlist.id,
        trackId: t.id
      });
    }
    onClose();
  }
</script>

<div
  class="menu-item submenu-trigger"
  onmouseenter={() => (showSubmenu = true)}
  onmouseleave={() => (showSubmenu = false)}
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

  {#if showSubmenu}
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

<style>
  @reference "../../../app.css";
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
