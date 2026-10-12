<!--
  @component PlaylistSubmenu
  プレイリストに追加するためのサブメニューコンポーネント。
  ContextMenuとGroupContextMenuで共通のプレイリスト追加サブメニューを独立化。
-->
<script lang="ts">
  import type { Track, Playlist } from '#lib/types/models.js';
  import { usePlaylistsQuery, useAddTracksToPlaylistMutation } from '#lib/queries/playlists.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import MenuItem from './MenuItem.svelte';
  import MenuSub from './MenuSub.svelte';

  // Props
  interface Props {
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const addTracksMutation = useAddTracksToPlaylistMutation();

  // 曲を追加できるプレイリスト（自動プレイリストの曲は条件で決まるため、含めない）
  const playlists = $derived(
    (playlistsQuery.data ?? []).filter((playlist: Playlist) => playlist.rules === null)
  );

  /**
   * プレイリストに追加
   */
  function handleAddToPlaylist(playlist: Playlist) {
    // メニューを閉じるとこのコンポーネントが破棄されるため、結果は待たない（失敗はトーストで通知される）
    addTracksMutation.mutate({
      playlistId: playlist.id,
      trackIds: tracks.map((track) => track.id)
    });
    onClose();
  }
</script>

<MenuSub label={m.contextMenu.addToPlaylist}>
  {#snippet icon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
    </svg>
  {/snippet}
  {#if playlistsQuery.isLoading}
    <div class="menu-message">{m.common.loading}</div>
  {:else if playlists.length > 0}
    {#each playlists as playlist (playlist.id)}
      <MenuItem onSelect={() => handleAddToPlaylist(playlist)}>
        {#snippet icon()}
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
        {/snippet}
        {playlist.name}
      </MenuItem>
    {/each}
  {:else}
    <div class="menu-message">{m.contextMenu.noPlaylists}</div>
  {/if}
</MenuSub>
