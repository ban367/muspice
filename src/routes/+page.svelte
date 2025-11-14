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

<main class="app-container">
  <header class="app-header">
    <h1>Muspice</h1>
    <button onclick={openImportDialog} class="import-button"> フォルダをインポート </button>
  </header>

  <div class="app-content">
    <Library />
  </div>

  <ImportDialog bind:isOpen={isImportDialogOpen} onImportComplete={handleImportComplete} />
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, 'Open Sans',
      'Helvetica Neue', sans-serif;
  }

  .app-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .app-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 2rem;
    background-color: #007bff;
    color: white;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  }

  .app-header h1 {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 600;
  }

  .import-button {
    padding: 0.5rem 1.5rem;
    background-color: white;
    color: #007bff;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 1rem;
    font-weight: 500;
    transition: background-color 0.2s;
  }

  .import-button:hover {
    background-color: #f0f0f0;
  }

  .app-content {
    flex: 1;
    overflow: hidden;
  }

  @media (prefers-color-scheme: dark) {
    .app-header {
      background-color: #0056b3;
    }

    .import-button {
      background-color: #1a1a1a;
      color: #007bff;
    }

    .import-button:hover {
      background-color: #2a2a2a;
    }
  }
</style>
