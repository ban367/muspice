<!--
  @component LibraryExportSettings
  ライブラリを、iTunes形式のライブラリのXMLかCSVで書き出す操作（設定ウィンドウ用。ADR-121）。
  ボタンを押すと、Rust側が保存のダイアログを開く。
-->
<script lang="ts">
  import { useExportLibraryMutation } from '#lib/queries/tracks.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import type { LibraryExportFormat } from '#lib/types/models.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const exportMutation = useExportLibraryMutation();
  /** 書き出している形式（ボタンの文言に使う） */
  let exporting = $state<LibraryExportFormat | null>(null);

  async function handleExport(format: LibraryExportFormat) {
    if (exportMutation.isPending) return;
    exporting = format;
    try {
      const result = await exportMutation.mutateAsync(format);
      // 保存先を選ばなかった場合は、何もしない
      if (result) showSuccess(m.libraryExport.exported(result.path));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    } finally {
      exporting = null;
    }
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.libraryExport.title}</h4>
  <p class="setting-description">{m.libraryExport.description}</p>

  <div class="flex flex-wrap gap-2 mt-3">
    <button
      class="btn-secondary text-sm"
      onclick={() => handleExport('xml')}
      disabled={exportMutation.isPending}
    >
      {exporting === 'xml' ? m.libraryExport.exporting : m.libraryExport.xml}
    </button>
    <button
      class="btn-secondary text-sm"
      onclick={() => handleExport('csv')}
      disabled={exportMutation.isPending}
    >
      {exporting === 'csv' ? m.libraryExport.exporting : m.libraryExport.csv}
    </button>
  </div>
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
</style>
