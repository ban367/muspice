<script lang="ts">
  import { resolve } from '$app/paths';
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import PlaylistIcon from '#lib/components/PlaylistIcon.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const playlistsQuery = usePlaylistsQuery();
</script>

<div class="playlists-page">
  <div class="page-header">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="header-icon"
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
    <h1 class="page-title">{m.playlists.title}</h1>
  </div>

  <div class="playlists-content">
    {#if playlistsQuery.isLoading}
      <div class="loading">{m.common.loading}</div>
    {:else if playlistsQuery.data && playlistsQuery.data.length > 0}
      <div class="playlists-grid">
        {#each playlistsQuery.data as playlist (playlist.id)}
          <a href={resolve(`playlists/${playlist.id}`)} class="playlist-card">
            <div class="playlist-icon" class:smart={playlist.rules !== null}>
              <PlaylistIcon smart={playlist.rules !== null} class="w-7 h-7 text-white" />
            </div>
            <div class="playlist-info">
              <h3 class="playlist-name">{playlist.name}</h3>
              <p class="playlist-meta">
                {playlist.rules === null
                  ? m.common.trackCount(playlist.tracks.length)
                  : m.smartPlaylist.badge}
              </p>
            </div>
          </a>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="empty-icon"
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
        <p>{m.playlists.empty}</p>
        <p class="hint">{m.playlists.emptyHint}</p>
      </div>
    {/if}
  </div>
</div>

<style>
  @reference "../../../app.css";
  .playlists-page {
    @apply flex flex-col h-full;
  }

  .page-header {
    @apply flex items-center gap-3 p-4 border-b border-border;
  }

  .header-icon {
    @apply w-6 h-6 text-primary;
  }

  .page-title {
    @apply text-xl font-bold text-text-primary m-0;
  }

  .playlists-content {
    @apply flex-1 overflow-auto p-4;
  }

  .loading {
    @apply flex items-center justify-center h-full text-text-muted;
  }

  .playlists-grid {
    @apply grid gap-4;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  }

  .playlist-card {
    @apply flex items-center gap-4 p-4 bg-base-300 rounded-lg transition-all cursor-pointer;
  }

  .playlist-card:hover {
    @apply bg-surface-hover -translate-y-0.5 shadow-lg;
  }

  .playlist-icon {
    @apply w-14 h-14 flex items-center justify-center rounded-md;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  }

  /* 自動プレイリストは、色を変えて見分ける */
  .playlist-icon.smart {
    background: linear-gradient(135deg, #f6a13c 0%, #e0567a 100%);
  }

  .playlist-info {
    @apply flex flex-col gap-1 min-w-0;
  }

  .playlist-name {
    @apply m-0 text-base font-semibold text-text-primary truncate;
  }

  .playlist-meta {
    @apply m-0 text-sm text-text-muted;
  }

  .empty-state {
    @apply flex flex-col items-center justify-center h-full text-center text-text-muted;
  }

  .empty-icon {
    @apply w-16 h-16 mb-4 opacity-50;
  }

  .empty-state p {
    @apply m-1;
  }

  .empty-state .hint {
    @apply text-sm text-text-dimmed;
  }
</style>
