<!--
  @component ImportDialog
  音楽ファイルのインポート（フォルダを選ぶ・ファイルを選ぶ）と、進み具合・結果の表示。

  インポートの実行と状態は`importer`が持つ（メニュー・ウィンドウへのドロップから始めた
  インポートも、このダイアログに表示する）。
-->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { validateFilePath } from '#lib/utils/validation.js';
  import { importer } from '#lib/stores/importer.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  /** インポートが終わってから、ダイアログを自動で閉じるまでの時間（ミリ秒） */
  const CLOSE_DELAY_MS = 2000;

  let selectedFolder = $state<string>('');
  /** フォルダの選択・入力の誤り（インポートそのものの失敗は`importer.errorMessage`） */
  let selectionError = $state<string>('');

  const isImporting = $derived(importer.isImporting);
  const importResult = $derived(importer.result);
  const errorMessage = $derived(selectionError || importer.errorMessage);

  // 成功したら、少し待ってからダイアログを閉じる（その間に次のインポートが始まったら、閉じない）
  $effect(() => {
    const result = importer.result;
    if (!result) return;
    const timer = setTimeout(() => {
      if (importer.result === result) closeDialog();
    }, CLOSE_DELAY_MS);
    return () => clearTimeout(timer);
  });

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
        selectionError = '';
      }
    } catch (error) {
      console.error('フォルダ選択エラー:', error);
      selectionError = m.importDialog.selectFolderFailed;
    }
  }

  /**
   * 選んだフォルダのインポートを開始
   */
  function startImport() {
    if (!selectedFolder) {
      selectionError = m.importDialog.folderRequired;
      return;
    }

    // ファイルパスをバリデーション
    if (!validateFilePath(selectedFolder)) {
      selectionError = m.importDialog.invalidPath;
      return;
    }

    selectionError = '';
    void importer.importFolder(selectedFolder);
  }

  /**
   * ファイルを選ぶダイアログを開き、選んだファイルをインポートする
   */
  function importFiles() {
    selectionError = '';
    void importer.importFiles();
  }

  /**
   * ダイアログを閉じる
   */
  function closeDialog() {
    ui.isImportDialogOpen = false;
    selectedFolder = '';
    selectionError = '';
    importer.reset();
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
      <!-- ファイルを個別に選ぶ（選んだ曲だけを登録する） -->
      <div class="mt-3 flex items-center gap-3">
        <span class="text-sm text-text-muted">{m.importDialog.orFiles}</span>
        <button
          onclick={importFiles}
          disabled={isImporting}
          class="btn-secondary whitespace-nowrap"
        >
          {m.importDialog.selectFiles}
        </button>
      </div>
      <p class="mt-2 mb-0 text-xs text-text-muted">{m.importDialog.filesHint}</p>
    </div>

    <!-- 重複ファイル処理の選択 -->
    <fieldset class="form-group border-none p-0 m-0">
      <legend class="form-label">{m.importDialog.duplicates}</legend>
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
          <input
            type="radio"
            bind:group={importer.duplicateAction}
            value="Skip"
            disabled={isImporting}
            class="w-4 h-4"
          />
          {m.importDialog.skip}
        </label>
        <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
          <input
            type="radio"
            bind:group={importer.duplicateAction}
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
          {#if importer.totalFiles > 0}
            {m.importDialog.importing(importer.processedFiles, importer.totalFiles)}
          {:else}
            {m.importDialog.scanning}
          {/if}
        </p>
        <div class="progress-bar-container">
          <div class="progress-bar-fill" style="width: {importer.progress}%"></div>
        </div>
        <div class="mt-2 flex flex-col items-center gap-1">
          <p class="text-center text-primary font-semibold m-0">{importer.progress}%</p>
          {#if importer.currentFile}
            <p class="text-center text-text-dimmed text-xs m-0 max-w-full truncate">
              {importer.currentFile}
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
