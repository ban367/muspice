<script lang="ts">
  import { usePlaylistsQuery } from '$lib/queries/playlists';

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
    <h1 class="page-title">プレイリスト</h1>
  </div>

  <div class="playlists-content">
    {#if playlistsQuery.isLoading}
      <div class="loading">読み込み中...</div>
    {:else if playlistsQuery.data && playlistsQuery.data.length > 0}
      <div class="playlists-grid">
        {#each playlistsQuery.data as playlist (playlist.id)}
          <a href="/playlists/{playlist.id}" class="playlist-card">
            <div class="playlist-icon">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="icon"
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
            <div class="playlist-info">
              <h3 class="playlist-name">{playlist.name}</h3>
              <p class="playlist-meta">{playlist.tracks.length}曲</p>
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
        <p>プレイリストがありません</p>
        <p class="hint">サイドバーの「+」ボタンから新しいプレイリストを作成できます</p>
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

  .playlist-icon .icon {
    @apply w-7 h-7 text-white;
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
