<!--
  @component LibraryFolderSettings
  設定ウィンドウの「ライブラリ」: インポートしたフォルダ（ライブラリフォルダ）の一覧・再スキャン・削除。
  操作はすぐに反映する（設定の「適用」は不要）。ライブラリが変わると、Rustが送る
  `LibraryChanged`イベントでメインウィンドウの一覧が更新される。自動の再スキャン
  （起動時・定期・フォルダの監視）でライブラリが変わった場合も、同じイベントで曲数などを読み直す。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { events } from '#lib/bindings.js';
  import { queryKeys } from '#lib/queries/keys.js';
  import {
    useLibraryFoldersQuery,
    useRemoveLibraryFolderMutation,
    useRescanLibraryFolderMutation
  } from '#lib/queries/libraryFolders.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import type { LibraryFolder, RescanResult } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { formatDateTime } from '#lib/utils/format.js';
  import { describeRescan, sumRescanResults } from '#lib/utils/rescan.js';

  const foldersQuery = useLibraryFoldersQuery();
  const rescanMutation = useRescanLibraryFolderMutation();
  const removeMutation = useRemoveLibraryFolderMutation();

  /** 再スキャン中のフォルダ（複数の場合は順番に処理する） */
  let scanningFolderId = $state<string | null>(null);
  let scanProgress = $state<{ current: number; total: number; currentFile: string } | null>(null);
  const isScanning = $derived(scanningFolderId !== null);

  /** 削除の確認中のフォルダ */
  let removingFolder = $state<LibraryFolder | null>(null);
  let removeTracks = $state(false);

  const folders = $derived(foldersQuery.data?.folders ?? []);
  const scannableFolders = $derived(folders.filter((folder) => folder.exists));

  const queryClient = useQueryClient();

  $effect(() => {
    const unlistenProgress = events.libraryScanProgress.listen((event) => {
      scanProgress = event.payload;
    });
    // 自動の再スキャンでライブラリが変わったら、曲数・最終スキャンの日時を読み直す
    const unlistenChanged = events.libraryChanged.listen(() => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders });
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenChanged.then((fn) => fn());
    };
  });

  /** フォルダを順番に再スキャンし、結果をまとめて通知する */
  async function rescan(targets: LibraryFolder[]) {
    const results: { path: string; result: RescanResult }[] = [];
    for (const folder of targets) {
      scanningFolderId = folder.id;
      scanProgress = null;
      try {
        results.push({ path: folder.path, result: await rescanMutation.mutateAsync(folder.id) });
      } catch {
        // 失敗はミューテーション内でトースト通知済み。残りのフォルダは続ける
      }
    }
    scanningFolderId = null;
    scanProgress = null;
    if (results.length === 0) return;

    const totals = sumRescanResults(results);
    showSuccess(describeRescan(totals));
    if (totals.errorCount > 0) {
      showWarning(`${totals.errorCount}件のファイルを読み込めませんでした`);
    }
    for (const path of totals.removalSkippedPaths) {
      showWarning(`音楽ファイルが見つからないため、ライブラリから曲を外しませんでした: ${path}`);
    }
  }

  function openRemoveDialog(folder: LibraryFolder) {
    removingFolder = folder;
    removeTracks = false;
  }

  function closeRemoveDialog() {
    if (removeMutation.isPending) return;
    removingFolder = null;
  }

  async function confirmRemove() {
    if (!removingFolder) return;
    const folderId = removingFolder.id;
    try {
      const removed = await removeMutation.mutateAsync({ folderId, removeTracks });
      showSuccess(
        removeTracks
          ? `ライブラリフォルダを削除し、${removed}曲をライブラリから外しました`
          : 'ライブラリフォルダを削除しました'
      );
      removingFolder = null;
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<section class="settings-section">
  <h3 class="section-title">ライブラリ</h3>

  <div class="folders-header">
    <div>
      <p class="setting-label">ライブラリフォルダ</p>
      <p class="setting-description">
        インポートしたフォルダです。再スキャンすると、フォルダ内で追加・削除・変更されたファイルをライブラリに反映します。
      </p>
    </div>
    <button
      type="button"
      class="btn-secondary text-sm shrink-0"
      onclick={() => rescan(scannableFolders)}
      disabled={isScanning || scannableFolders.length === 0}
    >
      すべて再スキャン
    </button>
  </div>

  {#if isScanning}
    <p class="scan-progress" role="status">
      {#if scanProgress}
        読み込み中 {scanProgress.current} / {scanProgress.total}: {scanProgress.currentFile}
      {:else}
        フォルダを確認しています...
      {/if}
    </p>
  {/if}

  {#if foldersQuery.isPending}
    <p class="setting-description">読み込み中...</p>
  {:else if foldersQuery.isError}
    <p class="setting-description">ライブラリフォルダを読み込めませんでした</p>
  {:else if folders.length === 0}
    <p class="empty-message">
      ライブラリフォルダはまだありません。メニューの「フォルダをインポート...」でフォルダを取り込むと、ここに追加されます。
    </p>
  {:else}
    <ul class="folder-list">
      {#each folders as folder (folder.id)}
        <li class="folder-item">
          <div class="folder-info">
            <p class="folder-path" title={folder.path}>{folder.path}</p>
            <p class="folder-meta">
              {#if folder.exists}
                {folder.trackCount}曲・最終スキャン: {formatDateTime(folder.lastScannedAt)}
              {:else}
                <span class="folder-missing">フォルダが見つかりません</span>
                （{folder.trackCount}曲。外付けドライブなどが接続されているか確認してください）
              {/if}
            </p>
          </div>
          <div class="folder-actions">
            <button
              type="button"
              class="btn-secondary text-xs"
              onclick={() => rescan([folder])}
              disabled={isScanning || !folder.exists}
              aria-busy={scanningFolderId === folder.id}
            >
              {scanningFolderId === folder.id ? '再スキャン中...' : '再スキャン'}
            </button>
            <button
              type="button"
              class="btn-secondary text-xs"
              onclick={() => openRemoveDialog(folder)}
              disabled={isScanning}
            >
              削除
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if foldersQuery.data && foldersQuery.data.unregisteredTrackCount > 0}
    <p class="setting-description mt-3">
      どのライブラリフォルダにも含まれない曲が{foldersQuery.data
        .unregisteredTrackCount}曲あります（フォルダの記録を始める前にインポートした曲など）。同じフォルダをもう一度インポートすると、ライブラリフォルダとして登録されます（登録済みの曲は読み直しません）。
    </p>
  {/if}
</section>

<Modal
  open={removingFolder !== null}
  onClose={closeRemoveDialog}
  title="ライブラリフォルダの削除"
  dismissible={!removeMutation.isPending}
  class="max-w-md"
>
  {#if removingFolder}
    <p class="mb-4 break-all">「{removingFolder.path}」をライブラリフォルダから外しますか？</p>
    <label class="flex items-start gap-2 text-sm cursor-pointer">
      <input type="checkbox" class="checkbox checkbox-sm mt-0.5" bind:checked={removeTracks} />
      <span>
        フォルダ内の{removingFolder.trackCount}曲もライブラリから外す
        <span class="block text-xs text-text-muted">
          ファイルは削除しません。外した曲はプレイリストからも消えます
        </span>
      </span>
    </label>
  {/if}

  {#snippet footer()}
    <button
      type="button"
      class="btn-secondary"
      onclick={closeRemoveDialog}
      disabled={removeMutation.isPending}
      data-autofocus
    >
      キャンセル
    </button>
    <button
      type="button"
      class="btn-danger"
      onclick={confirmRemove}
      disabled={removeMutation.isPending}
    >
      削除
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .settings-section {
    @apply max-w-xl;
  }

  .section-title {
    @apply text-xl font-semibold mb-6 m-0;
  }

  .setting-label {
    @apply block text-sm font-medium text-text-primary mb-1 m-0;
  }

  .setting-description {
    @apply text-xs text-text-muted m-0;
  }

  .folders-header {
    @apply flex items-start justify-between gap-4 mb-4;
  }

  .scan-progress {
    @apply text-xs text-text-secondary mb-3 m-0 truncate;
  }

  .empty-message {
    @apply text-sm text-text-muted p-4 border border-dashed border-border rounded-md m-0;
  }

  .folder-list {
    @apply list-none m-0 p-0 border border-border rounded-md divide-y divide-border;
  }

  .folder-item {
    @apply flex items-center gap-3 p-3;
  }

  .folder-info {
    @apply flex-1 min-w-0;
  }

  .folder-path {
    @apply text-sm text-text-primary truncate m-0 font-mono;
  }

  .folder-meta {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .folder-missing {
    @apply text-error-light;
  }

  .folder-actions {
    @apply flex gap-2 shrink-0;
  }
</style>
