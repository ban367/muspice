<!--
  重複した曲の画面。タイトルとアーティストが同じで、長さがほぼ同じ曲を、まとまりごとに並べる。
  まとまりは、取得済みの全曲の一覧から計算する（`#lib/utils/duplicates`）。どの曲を残すかは
  利用者が決め、曲のメニュー・Deleteキーから、ライブラリから外す・ファイルを削除する。
-->
<script lang="ts">
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import {
    duplicateGroupLookup,
    findDuplicateGroups,
    flattenDuplicateGroups
  } from '#lib/utils/duplicates.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const tracksQuery = useTracksQuery();
  const groups = $derived(tracksQuery.data ? findDuplicateGroups(tracksQuery.data) : null);
  const tracks = $derived(groups ? flattenDuplicateGroups(groups) : null);
  const groupOf = $derived(duplicateGroupLookup(groups ?? []));
</script>

<div class="duplicates-page">
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
        d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
      />
    </svg>
    <div class="min-w-0">
      <h1 class="page-title">{m.library.duplicates}</h1>
      <p class="page-description">
        {#if groups && tracks && groups.length > 0}
          {m.library.duplicatesSummary(groups.length, tracks.length)} ·
        {/if}
        {m.library.duplicatesDescription}
      </p>
    </div>
  </div>

  <!-- トラックリスト（まとまりの順のまま並べ、まとまりの間に線を引く） -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      isLoading={tracksQuery.isLoading}
      isError={tracksQuery.isError}
      error={tracksQuery.error}
      emptyMessage={m.library.noDuplicates}
      emptyHint={m.library.noDuplicatesHint}
      viewId="duplicates"
      defaultColumns={[
        'title',
        'artist',
        'album',
        'duration',
        'format',
        'bitrate',
        'fileSize',
        'filePath'
      ]}
      defaultSort={null}
      label={m.library.duplicates}
      {groupOf}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .duplicates-page {
    @apply flex flex-col h-full;
  }

  .page-header {
    @apply flex items-center gap-3 p-4 border-b border-border;
  }

  .header-icon {
    @apply w-6 h-6 text-primary shrink-0;
  }

  .page-title {
    @apply text-xl font-bold text-text-primary m-0;
  }

  .page-description {
    @apply mt-1 mb-0 text-xs text-text-muted;
  }

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
