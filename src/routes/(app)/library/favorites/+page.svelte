<script lang="ts">
  import { useFavoriteTracksQuery } from '#lib/queries/tracks.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // お気に入りの曲を取得（最近お気に入りにした順）
  const favoritesQuery = useFavoriteTracksQuery();
  const tracks = $derived(favoritesQuery.data ?? null);
  const isLoading = $derived(favoritesQuery.isLoading);
  const isError = $derived(favoritesQuery.isError);
  const error = $derived(favoritesQuery.error);
</script>

<div class="favorites-page">
  <!-- ヘッダー -->
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
        d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
      />
    </svg>
    <h1 class="page-title">{m.library.favorites}</h1>
  </div>

  <!-- トラックリスト（取得した順（最近お気に入りにした順）のまま並べる） -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      emptyMessage={m.library.noFavorites}
      emptyHint={m.library.noFavoritesHint}
      viewId="favorites"
      defaultSort={null}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .favorites-page {
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

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
