<script lang="ts">
  import { page } from '$app/stores';
  import { useFilterQuery } from '$lib/queries/tracks';
  import TrackList from '$lib/components/library/TrackList.svelte';

  // URLからジャンル名を取得
  const genreName = $derived(decodeURIComponent($page.params.genre));

  // ジャンルでフィルタリングされたトラックを取得
  const genreQuery = $derived(useFilterQuery({ genre: genreName }));
  const tracks = $derived(genreQuery.data ?? null);
  const isLoading = $derived(genreQuery.isLoading);
  const isError = $derived(genreQuery.isError);
  const error = $derived(genreQuery.error);
</script>

<div class="genre-detail-page">
  <!-- ジャンルヘッダー -->
  <div class="genre-header">
    <a href="/library/genres" class="back-link">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="back-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
      </svg>
      ジャンル一覧
    </a>
    <h1 class="genre-title">{genreName}</h1>
    {#if tracks}
      <span class="track-count">{tracks.length}曲</span>
    {/if}
  </div>

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      emptyMessage="このジャンルに曲がありません"
      emptyHint="他のジャンルを選択してください"
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .genre-detail-page {
    @apply flex flex-col h-full;
  }

  .genre-header {
    @apply flex items-center gap-4 p-4 border-b border-border;
  }

  .back-link {
    @apply flex items-center gap-1 text-sm text-text-muted hover:text-text-primary transition-colors;
  }

  .back-icon {
    @apply w-4 h-4;
  }

  .genre-title {
    @apply text-xl font-bold text-text-primary m-0;
  }

  .track-count {
    @apply text-sm text-text-muted;
  }

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
