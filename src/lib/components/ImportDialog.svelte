<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { ImportResult } from '$lib/types/models';
  import { DuplicateAction } from '$lib/types/models';
  import { validateFilePath } from '$lib/utils/validation';
  import { isImportDialogOpen } from '$lib/stores/ui';

  interface Props {
    onClose?: () => void;
    onImportComplete?: (result: ImportResult) => void;
  }

  let { onClose, onImportComplete }: Props = $props();

  let selectedFolder = $state<string>('');
  let duplicateAction = $state<DuplicateAction>(DuplicateAction.Skip);
  let isImporting = $state(false);
  let progress = $state(0);
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
        title: 'インポートするフォルダを選択'
      });

      if (selected && typeof selected === 'string') {
        selectedFolder = selected;
        errorMessage = '';
      }
    } catch (error) {
      console.error('フォルダ選択エラー:', error);
      errorMessage = 'フォルダの選択に失敗しました';
    }
  }

  /**
   * インポートを開始
   */
  async function startImport() {
    if (!selectedFolder) {
      errorMessage = 'フォルダを選択してください';
      return;
    }

    // ファイルパスをバリデーション
    if (!validateFilePath(selectedFolder)) {
      errorMessage = '不正なファイルパスです';
      return;
    }

    isImporting = true;
    progress = 0;
    errorMessage = '';
    importResult = null;

    try {
      // プログレスバーのアニメーション（実際の進行状況はバックエンドから取得する必要がある）
      const progressInterval = setInterval(() => {
        if (progress < 90) {
          progress += 10;
        }
      }, 200);

      const result = await invoke<ImportResult>('import_folder', {
        folderPath: selectedFolder,
        duplicateAction: duplicateAction
      });

      clearInterval(progressInterval);
      progress = 100;
      importResult = result;

      if (onImportComplete) {
        onImportComplete(result);
      }

      // 成功後、少し待ってからダイアログを閉じる
      setTimeout(() => {
        closeDialog();
      }, 2000);
    } catch (error) {
      console.error('インポートエラー:', error);
      errorMessage = `インポートに失敗しました: ${error}`;
      progress = 0;
    } finally {
      isImporting = false;
    }
  }

  /**
   * ダイアログを閉じる
   */
  function closeDialog() {
    isImportDialogOpen.set(false);
    selectedFolder = '';
    duplicateAction = DuplicateAction.Skip;
    progress = 0;
    importResult = null;
    errorMessage = '';
    if (onClose) {
      onClose();
    }
  }
</script>

{#if $isImportDialogOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-backdrop" onclick={closeDialog}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal-content max-w-xl" onclick={(e) => e.stopPropagation()}>
      <div class="flex justify-between items-center p-6 border-b border-border">
        <h3 class="m-0 text-xl font-semibold text-text-primary">音楽フォルダをインポート</h3>
        <button class="btn-icon w-8 h-8" onclick={closeDialog} disabled={isImporting}>
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="p-6 overflow-y-auto flex-1">
        {#if !importResult}
          <!-- フォルダ選択 -->
          <div class="form-group">
            <label for="folder-path" class="form-label">選択されたフォルダ</label>
            <div class="flex gap-2">
              <input
                id="folder-path"
                type="text"
                readonly
                value={selectedFolder || 'フォルダが選択されていません'}
                class="form-input flex-1"
              />
              <button onclick={selectFolder} disabled={isImporting} class="btn-secondary whitespace-nowrap">
                フォルダを選択
              </button>
            </div>
          </div>

          <!-- 重複ファイル処理の選択 -->
          <div class="form-group">
            <label class="form-label">重複ファイルの処理</label>
            <div class="flex flex-col gap-2">
              <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
                <input
                  type="radio"
                  bind:group={duplicateAction}
                  value={DuplicateAction.Skip}
                  disabled={isImporting}
                  class="w-4 h-4"
                />
                スキップ（既存ファイルを保持）
              </label>
              <label class="flex items-center gap-2 cursor-pointer text-text-secondary">
                <input
                  type="radio"
                  bind:group={duplicateAction}
                  value={DuplicateAction.Replace}
                  disabled={isImporting}
                  class="w-4 h-4"
                />
                置き換え（新しいファイルで上書き）
              </label>
            </div>
          </div>

          <!-- 進行状況バー -->
          {#if isImporting}
            <div class="mt-6">
              <p class="text-center text-text-secondary mb-2">インポート中...</p>
              <div class="progress-bar-container">
                <div class="progress-bar-fill" style="width: {progress}%"></div>
              </div>
              <p class="text-center text-primary font-semibold mt-2">{progress}%</p>
            </div>
          {/if}

          <!-- エラーメッセージ -->
          {#if errorMessage}
            <div class="message-error mt-4">{errorMessage}</div>
          {/if}
        {:else}
          <!-- インポート結果 -->
          <div class="text-center">
            <h4 class="text-xl font-semibold text-secondary m-0 mb-6">インポート完了</h4>
            <div class="flex flex-col gap-3 mb-6">
              <div class="result-stat">
                <span class="text-text-secondary">インポート成功:</span>
                <span class="font-semibold text-secondary">{importResult.importedCount}件</span>
              </div>
              <div class="result-stat">
                <span class="text-text-secondary">スキップ:</span>
                <span class="font-semibold text-text-primary">{importResult.skippedCount}件</span>
              </div>
              <div class="result-stat">
                <span class="text-text-secondary">エラー:</span>
                <span class="font-semibold text-error-light">{importResult.errorCount}件</span>
              </div>
            </div>
            {#if importResult.errors.length > 0}
              <div class="message-error text-left">
                <p class="font-semibold m-0 mb-2">エラー詳細:</p>
                <ul class="m-0 pl-6">
                  {#each importResult.errors as error}
                    <li class="my-1">{error}</li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <div class="flex justify-end gap-3 p-6 border-t border-border">
        {#if !importResult}
          <button onclick={closeDialog} disabled={isImporting} class="btn-secondary">
            キャンセル
          </button>
          <button
            onclick={startImport}
            disabled={!selectedFolder || isImporting}
            class="btn-primary"
          >
            {isImporting ? 'インポート中...' : 'インポート開始'}
          </button>
        {:else}
          <button onclick={closeDialog} class="btn-success">閉じる</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

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
