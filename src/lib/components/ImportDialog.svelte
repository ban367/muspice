<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { ImportResult } from '$lib/types/models';
  import { DuplicateAction } from '$lib/types/models';
  import { validateFilePath } from '$lib/utils/validation';

  interface Props {
    isOpen?: boolean;
    onClose?: () => void;
    onImportComplete?: (result: ImportResult) => void;
  }

  let { isOpen = $bindable(false), onClose, onImportComplete }: Props = $props();

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
    isOpen = false;
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

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="dialog-overlay" onclick={closeDialog}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dialog-content" onclick={(e) => e.stopPropagation()}>
      <div class="dialog-header">
        <h3>音楽フォルダをインポート</h3>
        <button class="close-button" onclick={closeDialog} disabled={isImporting}> ✕ </button>
      </div>

      <div class="dialog-body">
        {#if !importResult}
          <!-- フォルダ選択 -->
          <div class="folder-selection">
            <label for="folder-path">選択されたフォルダ:</label>
            <div class="folder-input-group">
              <input
                id="folder-path"
                type="text"
                readonly
                value={selectedFolder || 'フォルダが選択されていません'}
                class="folder-input"
              />
              <button onclick={selectFolder} disabled={isImporting} class="select-button">
                フォルダを選択
              </button>
            </div>
          </div>

          <!-- 重複ファイル処理の選択 -->
          <div class="duplicate-action-selection">
            <label>重複ファイルの処理:</label>
            <div class="radio-group">
              <label class="radio-option">
                <input
                  type="radio"
                  bind:group={duplicateAction}
                  value={DuplicateAction.Skip}
                  disabled={isImporting}
                />
                スキップ（既存ファイルを保持）
              </label>
              <label class="radio-option">
                <input
                  type="radio"
                  bind:group={duplicateAction}
                  value={DuplicateAction.Replace}
                  disabled={isImporting}
                />
                置き換え（新しいファイルで上書き）
              </label>
            </div>
          </div>

          <!-- 進行状況バー -->
          {#if isImporting}
            <div class="progress-section">
              <p>インポート中...</p>
              <div class="progress-bar">
                <div class="progress-fill" style="width: {progress}%"></div>
              </div>
              <p class="progress-text">{progress}%</p>
            </div>
          {/if}

          <!-- エラーメッセージ -->
          {#if errorMessage}
            <div class="error-message">
              {errorMessage}
            </div>
          {/if}
        {:else}
          <!-- インポート結果 -->
          <div class="import-result">
            <h4>インポート完了</h4>
            <div class="result-stats">
              <div class="stat-item">
                <span class="stat-label">インポート成功:</span>
                <span class="stat-value success">{importResult.importedCount}件</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">スキップ:</span>
                <span class="stat-value">{importResult.skippedCount}件</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">エラー:</span>
                <span class="stat-value error">{importResult.errorCount}件</span>
              </div>
            </div>
            {#if importResult.errors.length > 0}
              <div class="error-list">
                <p>エラー詳細:</p>
                <ul>
                  {#each importResult.errors as error}
                    <li>{error}</li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <div class="dialog-footer">
        {#if !importResult}
          <button onclick={closeDialog} disabled={isImporting} class="cancel-button">
            キャンセル
          </button>
          <button
            onclick={startImport}
            disabled={!selectedFolder || isImporting}
            class="import-button"
          >
            {isImporting ? 'インポート中...' : 'インポート開始'}
          </button>
        {:else}
          <button onclick={closeDialog} class="close-result-button">閉じる</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog-content {
    background-color: white;
    border-radius: 8px;
    width: 90%;
    max-width: 600px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .dialog-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem;
    border-bottom: 1px solid #e0e0e0;
  }

  .dialog-header h3 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
  }

  .close-button {
    background: none;
    border: none;
    font-size: 1.5rem;
    cursor: pointer;
    color: #666;
    padding: 0;
    width: 2rem;
    height: 2rem;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-button:hover:not(:disabled) {
    color: #000;
  }

  .close-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dialog-body {
    padding: 1.5rem;
    overflow-y: auto;
    flex: 1;
  }

  .folder-selection {
    margin-bottom: 1.5rem;
  }

  .duplicate-action-selection {
    margin-bottom: 1.5rem;
  }

  .duplicate-action-selection label {
    display: block;
    margin-bottom: 0.5rem;
    font-weight: 500;
  }

  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .radio-option {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: normal;
    margin-bottom: 0;
  }

  .radio-option input[type='radio'] {
    margin: 0;
  }

  .folder-selection label {
    display: block;
    margin-bottom: 0.5rem;
    font-weight: 500;
  }

  .folder-input-group {
    display: flex;
    gap: 0.5rem;
  }

  .folder-input {
    flex: 1;
    padding: 0.5rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background-color: #f8f9fa;
  }

  .select-button {
    padding: 0.5rem 1rem;
    background-color: #6c757d;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
  }

  .select-button:hover:not(:disabled) {
    background-color: #5a6268;
  }

  .select-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .progress-section {
    margin-top: 1.5rem;
  }

  .progress-section p {
    margin: 0.5rem 0;
    text-align: center;
  }

  .progress-bar {
    width: 100%;
    height: 24px;
    background-color: #e9ecef;
    border-radius: 12px;
    overflow: hidden;
    margin: 1rem 0;
  }

  .progress-fill {
    height: 100%;
    background-color: #007bff;
    transition: width 0.3s ease;
  }

  .progress-text {
    font-weight: 600;
    color: #007bff;
  }

  .error-message {
    margin-top: 1rem;
    padding: 0.75rem;
    background-color: #f8d7da;
    color: #721c24;
    border: 1px solid #f5c6cb;
    border-radius: 4px;
  }

  .import-result {
    text-align: center;
  }

  .import-result h4 {
    margin: 0 0 1.5rem 0;
    color: #28a745;
    font-size: 1.25rem;
  }

  .result-stats {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .stat-item {
    display: flex;
    justify-content: space-between;
    padding: 0.75rem;
    background-color: #f8f9fa;
    border-radius: 4px;
  }

  .stat-label {
    font-weight: 500;
  }

  .stat-value {
    font-weight: 600;
  }

  .stat-value.success {
    color: #28a745;
  }

  .stat-value.error {
    color: #dc3545;
  }

  .error-list {
    text-align: left;
    margin-top: 1rem;
    padding: 1rem;
    background-color: #f8d7da;
    border-radius: 4px;
  }

  .error-list p {
    margin: 0 0 0.5rem 0;
    font-weight: 600;
    color: #721c24;
  }

  .error-list ul {
    margin: 0;
    padding-left: 1.5rem;
    color: #721c24;
  }

  .error-list li {
    margin: 0.25rem 0;
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 1.5rem;
    border-top: 1px solid #e0e0e0;
  }

  .cancel-button,
  .import-button,
  .close-result-button {
    padding: 0.5rem 1.5rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 1rem;
  }

  .cancel-button {
    background-color: #6c757d;
    color: white;
  }

  .cancel-button:hover:not(:disabled) {
    background-color: #5a6268;
  }

  .import-button {
    background-color: #007bff;
    color: white;
  }

  .import-button:hover:not(:disabled) {
    background-color: #0056b3;
  }

  .import-button:disabled,
  .cancel-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .close-result-button {
    background-color: #28a745;
    color: white;
  }

  .close-result-button:hover {
    background-color: #218838;
  }

  @media (prefers-color-scheme: dark) {
    .dialog-content {
      background-color: #2a2a2a;
      color: #f0f0f0;
    }

    .dialog-header {
      border-bottom-color: #444;
    }

    .close-button {
      color: #aaa;
    }

    .close-button:hover:not(:disabled) {
      color: #fff;
    }

    .folder-input {
      background-color: #1a1a1a;
      border-color: #444;
      color: #f0f0f0;
    }

    .progress-bar {
      background-color: #444;
    }

    .stat-item {
      background-color: #1a1a1a;
    }

    .dialog-footer {
      border-top-color: #444;
    }
  }
</style>
