<!--
  プレイリストの一覧の画面。フォルダごとにまとめ、サイドバーと同じ並び順で出す（ADR-043）。
-->
<script lang="ts">
  import { resolve } from '$app/paths';
  import { usePlaylistFoldersQuery, usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { combineQueryStates } from '#lib/queries/shared.js';
  import PlaylistIcon from '#lib/components/PlaylistIcon.svelte';
  import { playlistSidebar } from '#lib/stores/playlistSidebar.svelte.js';
  import { buildPlaylistTree } from '#lib/utils/playlistTree.js';
  import type { Playlist } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const playlistsQuery = usePlaylistsQuery();
  const foldersQuery = usePlaylistFoldersQuery();
  const queryState = $derived(combineQueryStates(playlistsQuery, foldersQuery));

  const playlists = $derived(playlistsQuery.data ?? []);
  const tree = $derived(
    buildPlaylistTree(foldersQuery.data ?? [], playlists, playlistSidebar.sort)
  );
  // フォルダごとの節（プレイリストのないフォルダは出さない）と、フォルダの外の節
  const sections = $derived([
    ...tree.folders
      .filter((node) => node.playlists.length > 0)
      .map((node) => ({ id: node.folder.id, title: node.folder.name, playlists: node.playlists })),
    ...(tree.root.length > 0
      ? [{ id: 'root', title: m.playlists.outsideFolders, playlists: tree.root }]
      : [])
  ]);
  // フォルダ分けしていない場合は、見出しを出さない
  const hasFolderSections = $derived(sections.some((section) => section.id !== 'root'));
</script>

{#snippet playlistCard(playlist: Playlist)}
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
      {#if playlist.description}
        <p class="playlist-description">{playlist.description}</p>
      {/if}
    </div>
  </a>
{/snippet}

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
    {#if queryState.isLoading}
      <div class="loading">{m.common.loading}</div>
    {:else if playlists.length > 0}
      {#each sections as section (section.id)}
        <section class="folder-section">
          {#if hasFolderSections}
            <h2 class="folder-title">{section.title}</h2>
          {/if}
          <div class="playlists-grid">
            {#each section.playlists as playlist (playlist.id)}
              {@render playlistCard(playlist)}
            {/each}
          </div>
        </section>
      {/each}
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

  .playlist-description {
    @apply m-0 text-xs text-text-dimmed truncate;
  }

  .folder-section {
    @apply mb-6;
  }

  .folder-title {
    @apply m-0 mb-3 text-sm font-semibold text-text-secondary;
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
