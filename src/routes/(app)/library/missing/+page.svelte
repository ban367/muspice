<!--
  見つからない曲の画面（ADR-117）。再スキャンでファイルが見つからなくなった曲を並べる。
  曲は、取得済みの全曲の一覧から選ぶ（`Track.isMissing`）。曲の右クリックの「ファイルの場所を指定...」で
  1曲ずつ、「フォルダから探す...」で選んだフォルダの中から、結び付け直す（Rust側のダイアログで選ぶ）。
  結び付け直せない曲は、まとめてライブラリから外せる。
-->
<script lang="ts">
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import {
    useFindMissingTracksInFolderMutation,
    useRemoveMissingTracksMutation
  } from '#lib/queries/libraryFolders.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const tracksQuery = useTracksQuery();
  const tracks = $derived(tracksQuery.data?.filter((track) => track.isMissing) ?? null);

  const findMutation = useFindMissingTracksInFolderMutation();
  const removeMutation = useRemoveMissingTracksMutation();
  const isBusy = $derived(findMutation.isPending || removeMutation.isPending);

  function findInFolder() {
    // 失敗はミューテーション内でトースト通知する
    findMutation.mutate();
  }

  async function removeAll() {
    const count = tracks?.length ?? 0;
    if (count === 0) return;
    if (
      !(await confirmDestructive(m.library.removeMissingConfirm(count), m.library.removeMissingOk))
    ) {
      return;
    }
    try {
      const removed = await removeMutation.mutateAsync();
      showSuccess(m.libraryFolders.missingRemoved(removed));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<div class="missing-page">
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
        d="M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"
      />
    </svg>
    <div class="min-w-0 flex-1">
      <h1 class="page-title">{m.library.missing}</h1>
      <p class="page-description">
        {#if tracks && tracks.length > 0}
          {m.library.missingCount(tracks.length)} ·
        {/if}
        {m.library.missingDescription}
      </p>
    </div>
    {#if tracks && tracks.length > 0}
      <div class="actions">
        <button type="button" class="btn-secondary" onclick={findInFolder} disabled={isBusy}>
          {findMutation.isPending ? m.library.findingInFolder : m.library.findInFolder}
        </button>
        <button type="button" class="btn-secondary" onclick={removeAll} disabled={isBusy}>
          {m.library.removeMissing}
        </button>
      </div>
    {/if}
  </div>

  <!-- トラックリスト（元の場所の列を出す） -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      isLoading={tracksQuery.isLoading}
      isError={tracksQuery.isError}
      error={tracksQuery.error}
      emptyMessage={m.library.noMissing}
      emptyHint={m.library.noMissingHint}
      viewId="missing"
      defaultColumns={['title', 'artist', 'album', 'duration', 'filePath']}
      defaultSort={null}
      label={m.library.missing}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .missing-page {
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

  .actions {
    @apply flex items-center gap-2 shrink-0;
  }

  .track-list-container {
    @apply flex-1 overflow-hidden px-4;
  }
</style>
