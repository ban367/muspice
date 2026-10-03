<script lang="ts">
  import { useRecentlyPlayedTracksQuery } from '#lib/queries/tracks.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 最近再生した曲を取得
  const recentQuery = useRecentlyPlayedTracksQuery(50);
  const tracks = $derived(recentQuery.data ?? null);
  const isLoading = $derived(recentQuery.isLoading);
  const isError = $derived(recentQuery.isError);
  const error = $derived(recentQuery.error);
</script>

<div class="recent-page">
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
        d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
    <h1 class="page-title">{m.library.recentlyPlayed}</h1>
  </div>

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      emptyMessage={m.library.noRecentlyPlayed}
      emptyHint={m.library.noRecentlyPlayedHint}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .recent-page {
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
