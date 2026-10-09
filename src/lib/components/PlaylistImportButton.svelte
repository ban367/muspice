<!--
  @component PlaylistImportButton
  M3U（M3U8）のファイルを読み込んで、プレイリストを作るボタン。
  押すと、Rust側がファイルを選ぶダイアログを開く。すべての行に対応する曲があれば通知だけを出し、
  対応が付かなかった行・読めなかったファイルがあれば、結果のダイアログで知らせる。
-->
<script lang="ts">
  import type { M3uImportResult } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { useImportM3uMutation } from '#lib/queries/playlists.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const importMutation = useImportM3uMutation();

  /** 結果のダイアログに出す内容（nullなら表示しない） */
  let results = $state.raw<M3uImportResult[] | null>(null);

  /** 知らせることのない結果か（プレイリストができ、すべての行に対応する曲があった） */
  function isClean(result: M3uImportResult): boolean {
    return (
      result.playlistId !== null &&
      result.error === null &&
      result.unmatchedCount === 0 &&
      result.duplicateCount === 0
    );
  }

  async function startImport() {
    let imported: M3uImportResult[];
    try {
      imported = await importMutation.mutateAsync();
    } catch {
      // 失敗はミューテーション内でトースト通知する
      return;
    }
    // ファイルを選ばなかった
    if (imported.length === 0) return;

    if (!imported.every(isClean)) {
      results = imported;
    } else if (imported.length === 1) {
      showSuccess(m.notices.m3uImported(imported[0].playlistName ?? '', imported[0].addedCount));
    } else {
      showSuccess(m.notices.m3uImportedMany(imported.length));
    }
  }
</script>

<button
  class="btn-icon w-6 h-6 p-0"
  title={m.playlists.importM3u}
  aria-label={m.playlists.importM3u}
  onclick={startImport}
  disabled={importMutation.isPending}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    class="w-4 h-4"
    fill="none"
    viewBox="0 0 24 24"
    stroke="currentColor"
  >
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2"
      d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
    />
  </svg>
</button>

<Modal
  open={results !== null}
  onClose={() => (results = null)}
  title={m.playlists.importResultTitle}
  class="max-w-2xl"
>
  {#if results}
    <ul class="list-none m-0 p-0 flex flex-col gap-5">
      {#each results as result (result.fileName + (result.playlistId ?? ''))}
        <li class="import-result">
          <p class="m-0 font-semibold text-text-primary break-all">{result.fileName}</p>
          {#if result.error !== null}
            <p class="result-line text-error">{m.playlists.importFailed(result.error)}</p>
          {:else if result.playlistId !== null}
            <p class="result-line text-success">
              {m.playlists.importCreated(result.playlistName ?? '', result.addedCount)}
            </p>
          {:else}
            <p class="result-line text-warning">{m.playlists.importNotCreated}</p>
          {/if}
          {#if result.duplicateCount > 0}
            <p class="result-line text-text-muted">
              {m.playlists.importDuplicates(result.duplicateCount)}
            </p>
          {/if}
          {#if result.unmatchedCount > 0}
            <p class="result-line text-text-secondary">
              {m.playlists.importUnmatched(result.unmatchedCount)}
            </p>
            <ul class="unmatched-list">
              {#each result.unmatched as line, index (index)}
                <li>{line}</li>
              {/each}
              {#if result.unmatchedCount > result.unmatched.length}
                <li class="text-text-muted">
                  {m.playlists.importUnmatchedMore(result.unmatchedCount - result.unmatched.length)}
                </li>
              {/if}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
    {#if results.some((result) => result.unmatchedCount > 0)}
      <p class="mt-5 mb-0 text-sm text-text-muted">{m.playlists.importUnmatchedHint}</p>
    {/if}
  {/if}

  {#snippet footer()}
    <button onclick={() => (results = null)} class="btn-primary" data-autofocus>
      {m.common.close}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .result-line {
    @apply mt-1 mb-0 text-sm;
  }

  /* 対応が付かなかった行（M3Uに書かれていたまま。長い一覧は、この中でスクロールする） */
  .unmatched-list {
    @apply list-none mt-2 mb-0 p-3 max-h-40 overflow-y-auto rounded bg-base-300 text-xs text-text-secondary font-mono break-all;
  }

  .unmatched-list li + li {
    @apply mt-1;
  }
</style>
