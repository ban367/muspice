<!--
  @component LibraryFolderSettings
  設定ウィンドウの「ライブラリ」: インポートしたフォルダ（ライブラリフォルダ）の一覧・再スキャン・削除・
  読み込まないフォルダ（`LibraryFolderExclusionsDialog`）と、見つからない曲（ファイルが見つからなくなった
  トラック）の削除。
  操作はすぐに反映する（設定の「適用」は不要）。ライブラリが変わると、Rustが送る
  `LibraryChanged`イベントでメインウィンドウの一覧が更新される。自動の再スキャン
  （起動時・定期・フォルダの監視）でライブラリが変わった場合も、同じイベントで曲数などを読み直す。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { events } from '#lib/bindings.js';
  import { queryKeys } from '#lib/queries/keys.js';
  import {
    useLibraryFoldersQuery,
    useRemoveLibraryFolderExclusionMutation,
    useRemoveLibraryFolderMutation,
    useRemoveMissingTracksMutation,
    useRescanLibraryFolderMutation
  } from '#lib/queries/libraryFolders.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import type { FolderExclusion, LibraryFolder, RescanResult } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import LibraryFolderExclusionsDialog from './LibraryFolderExclusionsDialog.svelte';
  import { formatDateTime } from '#lib/utils/format.js';
  import { describeRescan, sumRescanResults } from '#lib/utils/rescan.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const foldersQuery = useLibraryFoldersQuery();
  const rescanMutation = useRescanLibraryFolderMutation();
  const removeMutation = useRemoveLibraryFolderMutation();
  const removeMissingMutation = useRemoveMissingTracksMutation();
  const removeExclusionMutation = useRemoveLibraryFolderExclusionMutation();

  /** 再スキャン中のフォルダ（複数の場合は順番に処理する） */
  let scanningFolderId = $state<string | null>(null);
  let scanProgress = $state<{ current: number; total: number; currentFile: string } | null>(null);
  const isScanning = $derived(scanningFolderId !== null);
  /** 再スキャンの進み具合の文（読み込まないフォルダの一覧でも出す） */
  const scanStatus = $derived(
    !isScanning
      ? null
      : scanProgress
        ? m.libraryFolders.progress(
            scanProgress.current,
            scanProgress.total,
            scanProgress.currentFile
          )
        : m.libraryFolders.checking
  );

  /** 読み込まないフォルダの一覧を開いているフォルダ（一覧の内容は、クエリの最新の値を使う） */
  let exclusionsFolderId = $state<string | null>(null);

  /** 削除の確認中のフォルダ */
  let removingFolder = $state<LibraryFolder | null>(null);
  let removeTracks = $state(false);

  /** 見つからない曲の削除を確認中か */
  let isConfirmingRemoveMissing = $state(false);
  const missingTrackCount = $derived(foldersQuery.data?.missingTrackCount ?? 0);

  const folders = $derived(foldersQuery.data?.folders ?? []);
  const scannableFolders = $derived(folders.filter((folder) => folder.exists));
  const exclusionsFolder = $derived(
    folders.find((folder) => folder.id === exclusionsFolderId) ?? null
  );

  const queryClient = useQueryClient();

  $effect(() => {
    const unlistenProgress = events.libraryScanProgress.listen((event) => {
      scanProgress = event.payload;
    });
    // インポート・自動の再スキャンでライブラリが変わったら、曲数・最終スキャンの日時を読み直す
    const unlistenChanged = events.libraryChanged.listen(() => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders });
    });
    // ウィンドウに戻ったら読み直す（外付けドライブの接続などを反映する。TanStack Queryの
    // 再取得はページの表示状態だけを見ており、ウィンドウを切り替えても動かないため）
    const unlistenFocus = getCurrentWebviewWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders });
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenChanged.then((fn) => fn());
      unlistenFocus.then((fn) => fn());
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
    notifyRescanResults(results);
  }

  /** 再スキャンの結果をまとめて通知する */
  function notifyRescanResults(results: { path: string; result: RescanResult }[]) {
    if (results.length === 0) return;

    const totals = sumRescanResults(results);
    showSuccess(describeRescan(totals));
    if (totals.errorCount > 0) {
      showWarning(m.libraryFolders.readErrors(totals.errorCount));
    }
    for (const path of totals.missingSkippedPaths) {
      showWarning(m.libraryFolders.missingSkipped(path));
    }
  }

  /** 読み込まないフォルダを一覧から外し、フォルダを再スキャンする（Rust側で続けて行う） */
  async function removeExclusion(folder: LibraryFolder, exclusion: FolderExclusion) {
    scanningFolderId = folder.id;
    scanProgress = null;
    try {
      const result = await removeExclusionMutation.mutateAsync(exclusion.id);
      if (result) {
        notifyRescanResults([{ path: folder.path, result }]);
      } else {
        showSuccess(m.libraryFolders.exclusionRemoved(exclusion.path));
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    } finally {
      scanningFolderId = null;
      scanProgress = null;
    }
  }

  function closeRemoveMissingDialog() {
    if (removeMissingMutation.isPending) return;
    isConfirmingRemoveMissing = false;
  }

  async function confirmRemoveMissing() {
    try {
      const removed = await removeMissingMutation.mutateAsync();
      showSuccess(m.libraryFolders.missingRemoved(removed));
      isConfirmingRemoveMissing = false;
    } catch {
      // 失敗はミューテーション内でトースト通知済み
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
    // 待っている間にチェックを変えられても、送った内容で通知する
    const withTracks = removeTracks;
    try {
      const removed = await removeMutation.mutateAsync({ folderId, removeTracks: withTracks });
      showSuccess(
        withTracks ? m.libraryFolders.removedWithTracks(removed) : m.libraryFolders.removed
      );
      removingFolder = null;
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<section class="settings-section">
  <h3 class="section-title">{m.libraryFolders.title}</h3>

  <div class="folders-header">
    <div>
      <p class="setting-label">{m.libraryFolders.folders}</p>
      <p class="setting-description">
        {m.libraryFolders.foldersHint}
      </p>
    </div>
    <button
      type="button"
      class="btn-secondary text-sm shrink-0"
      onclick={() => rescan(scannableFolders)}
      disabled={isScanning || scannableFolders.length === 0}
    >
      {m.libraryFolders.rescanAll}
    </button>
  </div>

  {#if scanStatus}
    <p class="scan-progress" role="status">{scanStatus}</p>
  {/if}

  {#if foldersQuery.isPending}
    <p class="setting-description">{m.common.loading}</p>
  {:else if foldersQuery.isError}
    <p class="setting-description">{m.libraryFolders.loadFailed}</p>
  {:else if folders.length === 0}
    <p class="empty-message">
      {m.libraryFolders.empty}
    </p>
  {:else}
    <ul class="folder-list">
      {#each folders as folder (folder.id)}
        <li class="folder-item">
          <div class="folder-info">
            <p class="folder-path" title={folder.path}>{folder.path}</p>
            <p class="folder-meta">
              {#if folder.exists}
                {m.libraryFolders.folderMeta(
                  folder.trackCount,
                  formatDateTime(folder.lastScannedAt)
                )}
              {:else}
                <span class="folder-missing">{m.libraryFolders.missing}</span>
                {m.libraryFolders.missingHint(folder.trackCount)}
              {/if}
            </p>
            <p class="folder-exclusions">
              <span class="truncate">
                {m.libraryFolders.exclusionsSummary(
                  folder.exclusions.map((exclusion) => exclusion.path)
                )}
              </span>
              <button
                type="button"
                class="link-button"
                onclick={() => (exclusionsFolderId = folder.id)}
                disabled={isScanning}
              >
                {m.libraryFolders.editExclusions}
              </button>
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
              {scanningFolderId === folder.id
                ? m.libraryFolders.rescanning
                : m.libraryFolders.rescan}
            </button>
            <button
              type="button"
              class="btn-secondary text-xs"
              onclick={() => openRemoveDialog(folder)}
              disabled={isScanning}
            >
              {m.common.delete}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if foldersQuery.data && foldersQuery.data.unregisteredTrackCount > 0}
    <p class="setting-description mt-3">
      {m.libraryFolders.unregistered(foldersQuery.data.unregisteredTrackCount)}
    </p>
  {/if}

  {#if missingTrackCount > 0}
    <div class="missing-tracks">
      <p class="setting-description">
        {m.libraryFolders.missingTracks(missingTrackCount)}
      </p>
      <button
        type="button"
        class="btn-secondary text-xs shrink-0"
        onclick={() => (isConfirmingRemoveMissing = true)}
        disabled={isScanning}
      >
        {m.libraryFolders.removeMissing}
      </button>
    </div>
  {/if}
</section>

<LibraryFolderExclusionsDialog
  folder={exclusionsFolder}
  {isScanning}
  {scanStatus}
  onClose={() => (exclusionsFolderId = null)}
  onRemove={removeExclusion}
/>

<Modal
  open={isConfirmingRemoveMissing}
  onClose={closeRemoveMissingDialog}
  title={m.libraryFolders.removeMissingTitle}
  dismissible={!removeMissingMutation.isPending}
  class="max-w-md"
>
  <p class="mb-2">{m.libraryFolders.removeMissingConfirm(missingTrackCount)}</p>
  <p class="text-xs text-text-muted">{m.libraryFolders.removeMissingHint}</p>

  {#snippet footer()}
    <button
      type="button"
      class="btn-secondary"
      onclick={closeRemoveMissingDialog}
      disabled={removeMissingMutation.isPending}
      data-autofocus
    >
      {m.common.cancel}
    </button>
    <button
      type="button"
      class="btn-danger"
      onclick={confirmRemoveMissing}
      disabled={removeMissingMutation.isPending}
    >
      {m.common.delete}
    </button>
  {/snippet}
</Modal>

<Modal
  open={removingFolder !== null}
  onClose={closeRemoveDialog}
  title={m.libraryFolders.removeTitle}
  dismissible={!removeMutation.isPending}
  class="max-w-md"
>
  {#if removingFolder}
    <p class="mb-4 break-all">{m.libraryFolders.removeConfirm(removingFolder.path)}</p>
    <label class="flex items-start gap-2 text-sm cursor-pointer">
      <input
        type="checkbox"
        class="w-4 h-4 shrink-0 mt-0.5 accent-primary cursor-pointer"
        bind:checked={removeTracks}
        disabled={removeMutation.isPending}
      />
      <span>
        {m.libraryFolders.removeTracks(removingFolder.trackCount)}
        <span class="block text-xs text-text-muted">
          {m.libraryFolders.removeTracksHint}
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
      {m.common.cancel}
    </button>
    <button
      type="button"
      class="btn-danger"
      onclick={confirmRemove}
      disabled={removeMutation.isPending}
    >
      {m.common.delete}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .settings-section {
    @apply max-w-xl;
  }

  .missing-tracks {
    @apply flex items-start justify-between gap-4 mt-3;
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

  .folder-exclusions {
    @apply flex items-baseline gap-2 text-xs text-text-muted mt-1 m-0 min-w-0;
  }

  .link-button {
    @apply shrink-0 text-xs text-primary hover:underline disabled:opacity-50 disabled:no-underline cursor-pointer disabled:cursor-default bg-transparent border-0 p-0;
  }

  .folder-actions {
    @apply flex gap-2 shrink-0;
  }
</style>
