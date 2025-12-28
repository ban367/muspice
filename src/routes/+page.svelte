<script lang="ts">
  import Library from '$lib/components/Library.svelte';
  import ImportDialog from '$lib/components/ImportDialog.svelte';
  import type { ImportResult } from '$lib/types/models';
  import { useQueryClient } from '@tanstack/svelte-query';

  let isImportDialogOpen = $state(false);
  const queryClient = useQueryClient();

  function openImportDialog() {
    isImportDialogOpen = true;
  }

  function handleImportComplete(result: ImportResult) {
    console.log('インポート完了:', result);
    // トラック一覧を再取得
    queryClient.invalidateQueries({ queryKey: ['tracks'] });
  }
</script>

<div class="page-container">
  <header class="page-header">
    <h1>音楽ライブラリ</h1>
    <button onclick={openImportDialog} class="import-button"> フォルダをインポート </button>
  </header>

  <div class="page-content">
    <Library />
  </div>

  <ImportDialog bind:isOpen={isImportDialogOpen} onImportComplete={handleImportComplete} />
</div>

<style>
  .page-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 0;
    margin-bottom: 1rem;
    border-bottom: 1px solid #e5e7eb;
  }

  .page-header h1 {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 600;
  }

  .import-button {
    padding: 0.5rem 1.5rem;
    background-color: #3b82f6;
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 1rem;
    font-weight: 500;
    transition: background-color 0.2s;
  }

  .import-button:hover {
    background-color: #2563eb;
  }

  .page-content {
    flex: 1;
    overflow: hidden;
  }

  @media (prefers-color-scheme: dark) {
    .page-header {
      border-bottom-color: #374151;
    }

    .import-button {
      background-color: #2563eb;
    }

    .import-button:hover {
      background-color: #1d4ed8;
    }
  }
</style>
