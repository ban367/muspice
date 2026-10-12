<script lang="ts">
  import { events } from '#lib/bindings.js';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { DuplicateAction, ImportResult } from '#lib/types/models.js';
  import { validateFilePath } from '#lib/utils/validation.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { useImportFolderMutation } from '#lib/queries/tracks.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  // インポート（成功時にトラック一覧のキャッシュを無効化する）
  const importMutation = useImportFolderMutation();

  let selectedFolder = $state<string>('');
  let duplicateAction = $state<DuplicateAction>('Skip');
  let isImporting = $state(false);
  let progress = $state(0);
  let currentFile = $state('');
  let totalFiles = $state(0);
  let processedFiles = $state(0);
  let importResult = $state<ImportResult | null>(null);
  let errorMessage = $state<string>('');

  /**
   * フォルダ選択ダイアログを開く
   */
  async function selectFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: m.importDialog.selectFolderTitle
      });

      if (selected && typeof selected === 'string') {
        selectedFolder = selected;
        errorMessage = '';
      }
    } catch (error) {
      console.error('フォルダ選択エラー:', error);
      errorMessage = m.importDialog.selectFolderFailed;
    }
  }

  /**
   * インポートを開始
   */
  async function startImport() {
    if (!selectedFolder) {
      errorMessage = m.importDialog.folderRequired;
      return;
    }

    // ファイルパスをバリデーション
    if (!validateFilePath(selectedFolder)) {
      errorMessage = m.importDialog.invalidPath;
      return;
    }

    isImporting = true;
    progress = 0;
    currentFile = '';
    totalFiles = 0;
    processedFiles = 0;
    errorMessage = '';
    importResult = null;

    let unlisten: (() => void) | null = null;

    try {
      // インポート進捗イベントをリッスン
      unlisten = await events.importProgress.listen((event) => {
        const { current, total, currentFile: file } = event.payload;
        processedFiles = current;
        totalFiles = total;
        currentFile = file;
        progress = total > 0 ? Math.round((current / total) * 100) : 0;
      });

      const result = await importMutation.mutateAsync({
        folderPath: selectedFolder,
        duplicateAction
      });

      progress = 100;
      importResult = result;

      // 成功後、少し待ってからダイアログを閉じる
      setTimeout(() => {
        closeDialog();
      }, 2000);
    } catch (error) {
      console.error('インポートエラー:', error);
      errorMessage = m.importDialog.importFailed(toErrorMessage(error));
      progress = 0;
    } finally {
      isImporting = false;
      // イベントリスナーを解除
      unlisten?.();
    }
  }

  /**
   * ダイアログを閉じる
   */
  function closeDialog() {
    ui.isImportDialogOpen = false;
    selectedFolder = '';
    duplicateAction = 'Skip';
    progress = 0;
    currentFile = '';
    totalFiles = 0;
    processedFiles = 0;
    importResult = null;
    errorMessage = '';
    if (onClose) {
      onClose();
    }
  }
</script>

<Modal
  open={ui.isImportDialogOpen}
  onClose={closeDialog}
  title={m.importDialog.title}
  dismissible={!isImporting}
  class="max-w-xl"
>
  {#if !importResult}
    <!-- フォルダ選択 -->
    <div class="form-group">
      <label for="folder-path" class="form-label">{m.importDialog.selectedFolder}</label>
      <div class="flex gap-2">
        <input
          id="folder-path"
          type="text"
          readonly
          value={selectedFolder || m.importDialog.noFolder}
          class="form-input flex-1"
        />
        <button
          onclick={selectFolder}
          disabled={isImporting}
          class="btn-secondary whitespace-nowrap"
        >
          {m.importDialog.selectFolder}
        </button>
      </div>
    </div>

    <!-- 重複ファイル処理の選択 -->
    <fieldset class="form-group border-none p-0 m-0">
      <legend class="form-label">{m.importDialog.duplicates}</legend>
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
          <input
            type="radio"
            bind:group={duplicateAction}
            value="Skip"
            disabled={isImporting}
            class="w-4 h-4"
          />
          {m.importDialog.skip}
        </label>
        <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
          <input
            type="radio"
            bind:group={duplicateAction}
            value="Replace"
            disabled={isImporting}
            class="w-4 h-4"
          />
          {m.importDialog.replace}
        </label>
      </div>
    </fieldset>

    <!-- 進行状況バー -->
    {#if isImporting}
      <div class="mt-6">
        <p class="text-center text-text-secondary mb-2">
          {#if totalFiles > 0}
            {m.importDialog.importing(processedFiles, totalFiles)}
          {:else}
            {m.importDialog.scanning}
          {/if}
        </p>
        <div class="progress-bar-container">
          <div class="progress-bar-fill" style="width: {progress}%"></div>
        </div>
        <div class="mt-2 flex flex-col items-center gap-1">
          <p class="text-center text-primary font-semibold m-0">{progress}%</p>
          {#if currentFile}
            <p class="text-center text-text-dimmed text-xs m-0 max-w-full truncate">
              {currentFile}
            </p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- エラーメッセージ -->
    {#if errorMessage}
      <div class="message-error mt-4">{errorMessage}</div>
    {/if}
  {:else}
    <!-- インポート結果 -->
    <div class="text-center">
      <h4 class="text-xl font-semibold text-secondary m-0 mb-6">{m.importDialog.completed}</h4>
      <div class="flex flex-col gap-3 mb-6">
        <div class="result-stat">
          <span class="text-text-secondary">{m.importDialog.imported}</span>
          <span class="font-semibold text-secondary"
            >{m.common.itemCount(importResult.importedCount)}</span
          >
        </div>
        <div class="result-stat">
          <span class="text-text-secondary">{m.importDialog.skipped}</span>
          <span class="font-semibold text-text-primary"
            >{m.common.itemCount(importResult.skippedCount)}</span
          >
        </div>
        {#if importResult.relinkedCount > 0}
          <!-- 見つからない曲を、移動・改名された先のファイルに結び付けた（新しい曲としては登録していない） -->
          <div class="result-stat">
            <span class="text-text-secondary">{m.importDialog.relinked}</span>
            <span class="font-semibold text-text-primary"
              >{m.common.itemCount(importResult.relinkedCount)}</span
            >
          </div>
        {/if}
        <div class="result-stat">
          <span class="text-text-secondary">{m.importDialog.errors}</span>
          <span class="font-semibold text-error-light"
            >{m.common.itemCount(importResult.errorCount)}</span
          >
        </div>
      </div>
      {#if importResult.errors.length > 0}
        <div class="message-error text-left">
          <p class="font-semibold m-0 mb-2">{m.importDialog.errorDetails}</p>
          <ul class="m-0 pl-6">
            {#each importResult.errors as error, i (i)}
              <li class="my-1">{error}</li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if !importResult}
      <button onclick={closeDialog} disabled={isImporting} class="btn-secondary">
        {m.common.cancel}
      </button>
      <button onclick={startImport} disabled={!selectedFolder || isImporting} class="btn-primary">
        {isImporting ? m.importDialog.importingShort : m.importDialog.start}
      </button>
    {:else}
      <button onclick={closeDialog} class="btn-success">{m.common.close}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";
  .progress-bar-container {
    @apply w-full h-6 bg-base-400 rounded-full overflow-hidden;
  }

  .progress-bar-fill {
    @apply h-full bg-primary transition-[width] duration-300;
  }

  .result-stat {
    @apply flex justify-between py-3 px-4 bg-base-400 rounded-md;
  }
</style>
