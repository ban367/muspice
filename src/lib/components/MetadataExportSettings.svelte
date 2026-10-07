<!--
  @component MetadataExportSettings
  アプリ内（データベース）だけにある編集内容・評価を、ファイルのタグへ書き出す操作（設定ウィンドウ用）。
  メタデータの編集がファイルへ書き込まれなかった頃の内容を、ファイルへ移すために使う。
-->
<script lang="ts">
  import { useWriteLibraryMetadataToFilesMutation } from '#lib/queries/tracks.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import type { WriteMetadataResult } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const writeMutation = useWriteLibraryMetadataToFilesMutation();

  // 直前の結果（書き込めなかったファイルの理由を表示する）
  let lastResult = $state.raw<WriteMetadataResult | null>(null);

  async function handleWrite() {
    if (writeMutation.isPending) return;
    if (!(await confirmDestructive(m.metadataExport.confirm))) return;

    try {
      const result = await writeMutation.mutateAsync();
      lastResult = result;
      const summary = m.metadataExport.result(
        result.writtenCount,
        result.unchangedCount,
        result.skippedCount,
        result.errorCount
      );
      if (result.errorCount > 0) {
        showWarning(summary);
      } else {
        showSuccess(summary);
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.metadataExport.title}</h4>
  <p class="setting-description">{m.metadataExport.description}</p>
  <button
    class="btn-secondary text-sm mt-3"
    onclick={handleWrite}
    disabled={writeMutation.isPending}
  >
    {writeMutation.isPending ? m.metadataExport.writing : m.metadataExport.write}
  </button>

  {#if lastResult && lastResult.errors.length > 0}
    <ul class="error-list">
      {#each lastResult.errors as error (error)}
        <li>{error}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  @reference "../../app.css";

  /* 設定画面（settings/+page.svelte）の項目と同じ見た目にする */
  .settings-section {
    @apply max-w-xl;
  }

  .subsection-title {
    @apply text-base font-semibold mt-8 mb-4;
  }

  .setting-description {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .error-list {
    @apply mt-3 mb-0 pl-4 text-xs text-error-light max-h-32 overflow-y-auto;
  }
</style>
