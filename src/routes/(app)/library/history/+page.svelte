<script lang="ts">
  import { usePlayHistoryQuery, useTracksQuery } from '#lib/queries/tracks.js';
  import { combineQueryStates } from '#lib/queries/shared.js';
  import PlayHistoryList from '#lib/components/library/PlayHistoryList.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 再生履歴（再生した日時とトラックID）と、曲の情報を引くための全曲の一覧
  const historyQuery = usePlayHistoryQuery();
  const tracksQuery = useTracksQuery();
  const status = $derived(combineQueryStates(historyQuery, tracksQuery));
</script>

<div class="history-page">
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
    <h1 class="page-title">{m.library.playHistory}</h1>
  </div>

  <!-- 再生履歴（日付ごと） -->
  <div class="track-list-container">
    <PlayHistoryList
      entries={historyQuery.data ?? null}
      tracks={tracksQuery.data ?? null}
      isLoading={status.isLoading}
      isError={status.isError}
      error={status.error}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .history-page {
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
