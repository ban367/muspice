<!--
  @component LibraryFolderExclusionsDialog
  設定の「ライブラリ」で開く、ライブラリフォルダの読み込まないフォルダ（除外）の一覧。
  足すフォルダはRust側のダイアログで選ぶ（WebViewからパスを渡さない）。足したフォルダの中に
  ライブラリの曲があれば、ライブラリから外すかを確かめる（ファイルは消さない）。
  一覧から外す操作は、再スキャンを伴うため、呼び出し側（`onRemove`）が進み具合と結果を扱う。
-->
<script lang="ts">
  import {
    useAddLibraryFolderExclusionMutation,
    useRemoveExcludedTracksMutation
  } from '#lib/queries/libraryFolders.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import type { FolderExclusion, LibraryFolder } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 一覧を見るライブラリフォルダ（nullなら閉じている） */
    folder: LibraryFolder | null;
    /** 再スキャン中か（足す・外す操作を止める） */
    isScanning: boolean;
    /** 再スキャンの進み具合の文（再スキャン中でなければnull） */
    scanStatus: string | null;
    onClose: () => void;
    /** 読み込まないフォルダを一覧から外す（外した後の再スキャンも、呼び出し側が行う） */
    onRemove: (folder: LibraryFolder, exclusion: FolderExclusion) => Promise<void>;
  }

  let { folder, isScanning, scanStatus, onClose, onRemove }: Props = $props();

  const addMutation = useAddLibraryFolderExclusionMutation();
  const removeTracksMutation = useRemoveExcludedTracksMutation();

  /** 足したフォルダのうち、中の曲をライブラリから外すかを確かめているもの */
  let pendingTracks = $state<FolderExclusion | null>(null);

  const isBusy = $derived(isScanning || addMutation.isPending || removeTracksMutation.isPending);

  function close() {
    if (isBusy) return;
    // 中の曲を外すかを確かめている間に閉じた場合は、曲を残したことになる
    keepTracks();
    onClose();
  }

  async function addExclusion() {
    if (!folder) return;
    try {
      const added = await addMutation.mutateAsync(folder.id);
      if (!added) return;
      if (added.trackCount > 0) {
        pendingTracks = added;
      } else {
        showSuccess(m.libraryFolders.exclusionAdded(added.path));
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  function keepTracks() {
    if (!pendingTracks) return;
    showSuccess(m.libraryFolders.exclusionAdded(pendingTracks.path));
    pendingTracks = null;
  }

  async function removeTracks() {
    if (!pendingTracks) return;
    const exclusion = pendingTracks;
    try {
      const removed = await removeTracksMutation.mutateAsync(exclusion.id);
      showSuccess(m.libraryFolders.excludedTracksRemoved(exclusion.path, removed));
      pendingTracks = null;
    } catch {
      // 失敗はミューテーション内でトースト通知済み（読み込まないフォルダには入っている）
    }
  }
</script>

<Modal
  open={folder !== null}
  onClose={close}
  title={pendingTracks ? m.libraryFolders.excludedTracksTitle : m.libraryFolders.exclusionsTitle}
  dismissible={!isBusy}
  class="max-w-lg"
>
  {#if folder && pendingTracks}
    <p class="mb-2 break-all">
      {m.libraryFolders.excludedTracksConfirm(pendingTracks.path, pendingTracks.trackCount)}
    </p>
    <p class="text-xs text-text-muted m-0">{m.libraryFolders.excludedTracksHint}</p>
  {:else if folder}
    <p class="text-sm mb-2 break-all m-0">{m.libraryFolders.exclusionsHint(folder.path)}</p>
    <p class="text-xs text-text-muted mb-4 m-0">{m.libraryFolders.alwaysSkippedHint}</p>

    {#if folder.exclusions.length === 0}
      <p class="empty-message">{m.libraryFolders.noExclusions}</p>
    {:else}
      <ul class="exclusion-list">
        {#each folder.exclusions as exclusion (exclusion.id)}
          <li class="exclusion-item">
            <div class="exclusion-info">
              <p class="exclusion-path" title={exclusion.path}>{exclusion.path}</p>
              {#if exclusion.trackCount > 0}
                <p class="exclusion-meta">
                  {m.libraryFolders.exclusionTracks(exclusion.trackCount)}
                </p>
              {/if}
            </div>
            <button
              type="button"
              class="btn-secondary text-xs shrink-0"
              onclick={() => folder && onRemove(folder, exclusion)}
              disabled={isBusy}
            >
              {m.libraryFolders.removeExclusion}
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if scanStatus}
      <p class="scan-status" role="status">{scanStatus}</p>
    {/if}
  {/if}

  {#snippet footer()}
    {#if pendingTracks}
      <button
        type="button"
        class="btn-secondary"
        onclick={keepTracks}
        disabled={removeTracksMutation.isPending}
        data-autofocus
      >
        {m.libraryFolders.keepExcludedTracks}
      </button>
      <button
        type="button"
        class="btn-danger"
        onclick={removeTracks}
        disabled={removeTracksMutation.isPending}
      >
        {m.libraryFolders.removeExcludedTracks}
      </button>
    {:else}
      <button
        type="button"
        class="btn-secondary mr-auto"
        onclick={addExclusion}
        disabled={isBusy || !folder?.exists}
      >
        {m.libraryFolders.addExclusion}
      </button>
      <button type="button" class="btn-primary" onclick={close} disabled={isBusy} data-autofocus>
        {m.common.close}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .empty-message {
    @apply text-sm text-text-muted p-4 border border-dashed border-border rounded-md m-0;
  }

  .exclusion-list {
    @apply list-none m-0 p-0 border border-border rounded-md divide-y divide-border;
  }

  .exclusion-item {
    @apply flex items-center gap-3 px-3 py-2;
  }

  .exclusion-info {
    @apply flex-1 min-w-0;
  }

  .exclusion-path {
    @apply text-sm text-text-primary truncate m-0 font-mono;
  }

  .exclusion-meta {
    @apply text-xs text-text-muted mt-0.5 m-0;
  }

  .scan-status {
    @apply text-xs text-text-secondary mt-3 m-0 truncate;
  }
</style>
