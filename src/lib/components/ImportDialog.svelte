<!--
  @component ImportDialog
  音楽ファイルのインポート（フォルダを選ぶ・ファイルを選ぶ）と、進み具合・結果の表示。

  インポートの実行と状態は`importer`が持つ（メニュー・ウィンドウへのドロップから始めた
  インポートも、このダイアログに表示する）。取り込む曲を音楽フォルダへコピーする設定で、
  ライブラリフォルダの外のフォルダを取り込む時は、参照するかコピーするかを選ばせる（ADR-069）。
-->
<script lang="ts">
  import { importer } from '#lib/stores/importer.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { useMusicFolderQuery } from '#lib/queries/musicFolder.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import type { FolderImportMode, NotCopied } from '#lib/types/models.js';
  import { formatFileSize } from '#lib/utils/format.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  /** インポートが終わってから、ダイアログを自動で閉じるまでの時間（ミリ秒） */
  const CLOSE_DELAY_MS = 2000;

  const settingsQuery = useSettingsQuery();
  const musicFolderQuery = useMusicFolderQuery();

  const isImporting = $derived(importer.isImporting);
  const importResult = $derived(importer.result);
  const pending = $derived(importer.pending);

  /** 取り込む曲をコピーする音楽フォルダ（設定がオフか、音楽フォルダが未設定ならnull） */
  const copyTarget = $derived(
    settingsQuery.data?.copyToMusicFolder ? (musicFolderQuery.data?.folder?.path ?? null) : null
  );

  /** 預かったフォルダを、音楽フォルダへコピーできない理由（コピーできればnull） */
  const copyBlocker = $derived.by(() => {
    if (!pending) return null;
    if (!pending.musicFolderExists) return m.importDialog.musicFolderMissing(pending.musicFolder);
    if (pending.freeBytes !== null && pending.copyBytes > pending.freeBytes) {
      return m.importDialog.notEnoughSpace(
        formatFileSize(pending.copyBytes),
        formatFileSize(pending.freeBytes)
      );
    }
    return null;
  });

  // 成功したら、少し待ってからダイアログを閉じる（その間に次のインポートが始まったら、閉じない）。
  // エラー・コピーしなかった曲がある場合は、読めるよう閉じない
  $effect(() => {
    const result = importer.result;
    if (!result || result.errorCount > 0 || result.notCopied.length > 0) return;
    const timer = setTimeout(() => {
      if (importer.result === result) closeDialog();
    }, CLOSE_DELAY_MS);
    return () => clearTimeout(timer);
  });

  /**
   * フォルダを選ぶダイアログ（Rust側が開く）を開き、選んだフォルダをインポートする
   */
  function importFolder() {
    void importer.importFolder();
  }

  /**
   * ファイルを選ぶダイアログ（Rust側が開く）を開き、選んだファイルをインポートする
   */
  function importFiles() {
    void importer.importFiles();
  }

  /**
   * 預かったフォルダを、選んだ方法で取り込む
   */
  function importPending(mode: FolderImportMode) {
    void importer.importPending(mode);
  }

  /** コピーしなかった曲の、一覧に出す文（元のファイル名 → コピー先: 理由） */
  function notCopiedLabel(entry: NotCopied): string {
    const name = entry.source.split(/[\\/]/).pop() ?? entry.source;
    return `${name} → ${entry.target}: ${m.importDialog.notCopiedReasons[entry.reason]}`;
  }

  /**
   * ダイアログを閉じる（預かったフォルダがあれば、取り込みをやめる）
   */
  function closeDialog() {
    ui.isImportDialogOpen = false;
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

    {#if pending}
      <!-- 預かったフォルダの取り込み方を選ぶ（参照か、音楽フォルダへのコピーか） -->
      <section class="mt-6" aria-labelledby="import-mode-title">
        <h4 id="import-mode-title" class="pending-title">{m.importDialog.chooseMode}</h4>
        <ul class="pending-folders">
          {#each pending.folders as folder (folder)}
            <li class="truncate" title={folder}>{folder}</li>
          {/each}
        </ul>
        <p class="pending-summary">
          {m.importDialog.pendingSummary(pending.trackCount, formatFileSize(pending.totalBytes))}
        </p>
        <div class="mode-choices">
          <button
            type="button"
            class="mode-choice"
            onclick={() => importPending('reference')}
            disabled={isImporting}
          >
            <span class="mode-title">{m.importDialog.reference}</span>
            <span class="mode-hint">{m.importDialog.referenceHint}</span>
          </button>
          <button
            type="button"
            class="mode-choice"
            onclick={() => importPending('copy')}
            disabled={isImporting || copyBlocker !== null}
          >
            <span class="mode-title">{m.importDialog.copy}</span>
            <span class="mode-hint">
              {m.importDialog.copyModeHint(
                pending.musicFolder,
                pending.copyCount,
                formatFileSize(pending.copyBytes)
              )}
              {#if pending.copyCount < pending.trackCount}
                {m.importDialog.alreadyInLibrary(pending.trackCount - pending.copyCount)}
              {/if}
            </span>
            {#if pending.freeBytes !== null}
              <span class="mode-hint">
                {m.importDialog.freeSpace(formatFileSize(pending.freeBytes))}
              </span>
            {/if}
          </button>
        </div>
        {#if copyBlocker}
          <p class="message-error mt-3 mb-0">{copyBlocker}</p>
        {/if}
      </section>
    {:else}
      <!-- フォルダ・ファイルを選んでインポートする（選ぶダイアログは、Rust側が開く） -->
      <div class="mt-6">
        <div class="flex flex-wrap gap-2">
          <button
            onclick={importFolder}
            disabled={isImporting}
            class="btn-primary whitespace-nowrap"
          >
            {m.importDialog.selectFolder}
          </button>
          <button
            onclick={importFiles}
            disabled={isImporting}
            class="btn-secondary whitespace-nowrap"
          >
            {m.importDialog.selectFiles}
          </button>
        </div>
        <p class="mt-2 mb-0 text-xs text-text-muted">{m.importDialog.sourceHint}</p>
        {#if copyTarget}
          <p class="mt-2 mb-0 text-xs text-text-muted">{m.importDialog.copyHint(copyTarget)}</p>
        {/if}
      </div>
    {/if}

    <!-- 進行状況バー -->
    {#if isImporting}
      <div class="mt-6">
        <p class="text-center text-text-secondary mb-2">
          {#if importer.totalFiles === 0}
            {m.importDialog.scanning}
          {:else if importer.phase === 'copying'}
            {m.importDialog.copying(importer.processedFiles, importer.totalFiles)}
          {:else}
            {m.importDialog.importing(importer.processedFiles, importer.totalFiles)}
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
    {#if importer.errorMessage}
      <div class="message-error mt-4">{importer.errorMessage}</div>
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
        {#if importResult.copiedCount > 0}
          <!-- 音楽フォルダへコピーして取り込んだ（インポート成功・引き継ぎの内数） -->
          <div class="result-stat">
            <span class="text-text-secondary">{m.importDialog.copied}</span>
            <span class="font-semibold text-text-primary"
              >{m.common.itemCount(importResult.copiedCount)}</span
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
      {#if importResult.notCopied.length > 0}
        <div class="not-copied">
          <p class="font-semibold m-0 mb-2">
            {m.importDialog.notCopied(importResult.notCopied.length)}
          </p>
          <ul class="m-0 pl-6">
            {#each importResult.notCopied as entry, i (i)}
              <li class="my-1 break-all">{notCopiedLabel(entry)}</li>
            {/each}
          </ul>
        </div>
      {/if}
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

  .pending-title {
    @apply text-sm font-semibold text-text-primary m-0 mb-2;
  }

  .pending-folders {
    @apply list-none m-0 p-0 text-sm text-text-primary font-mono;
  }

  .pending-summary {
    @apply text-xs text-text-muted mt-1 mb-0;
  }

  .mode-choices {
    @apply grid gap-2 mt-3 sm:grid-cols-2;
  }

  /* 選ぶとすぐに取り込む（題と説明を並べた、大きめのボタン） */
  .mode-choice {
    @apply flex flex-col items-start gap-1 p-3 text-left bg-base-400 border border-border rounded-md
           cursor-pointer transition-colors;
  }

  .mode-choice:hover:not(:disabled) {
    @apply border-primary;
  }

  .mode-choice:disabled {
    @apply opacity-50 cursor-default;
  }

  .mode-title {
    @apply text-sm font-semibold text-text-primary;
  }

  .mode-hint {
    @apply text-xs text-text-muted;
  }

  /* コピーしなかった曲（取り込んでいない）。エラーではないため、注意の色で出す */
  .not-copied {
    @apply p-3 mb-4 bg-warning/20 text-warning rounded-md text-sm text-left;
  }
</style>
