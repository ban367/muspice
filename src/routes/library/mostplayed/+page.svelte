<script lang="ts">
  import { useMostPlayedTracksQuery } from '$lib/queries/tracks';
  import TrackList from '$lib/components/library/TrackList.svelte';

  // よく再生する曲を取得
  const mostPlayedQuery = useMostPlayedTracksQuery(50);
  const tracks = $derived(mostPlayedQuery.data ?? null);
  const isLoading = $derived(mostPlayedQuery.isLoading);
  const isError = $derived(mostPlayedQuery.isError);
  const error = $derived(mostPlayedQuery.error);
</script>

<div class="mostplayed-page">
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
        d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6"
      />
    </svg>
    <h1 class="page-title">よく再生する曲</h1>
  </div>

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      emptyMessage="よく再生する曲がありません"
      emptyHint="曲を繰り返し再生すると、ここに表示されます"
    />
  </div>
</div>

<style>
  @reference "../../../app.css";
  .mostplayed-page {
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
