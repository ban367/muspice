<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    useCanMoveToTrashQuery,
    useDeleteTracksMutation,
    useDeleteTracksWithFilesMutation
  } from '#lib/queries/tracks.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    open?: boolean;
    tracks: Track[];
    onClose: () => void;
  }

  let { open = $bindable(false), tracks, onClose }: Props = $props();

  // ミューテーション
  const deleteTracksMutation = useDeleteTracksMutation();
  const deleteTracksWithFilesMutation = useDeleteTracksWithFilesMutation();

  // ファイルも削除する時に、ゴミ箱へ移すか（macOSだけ。ほかのOSでは完全に削除する）
  const canMoveToTrashQuery = useCanMoveToTrashQuery();
  const movesToTrash = $derived(canMoveToTrashQuery.data === true);

  // 削除中フラグ
  const isDeleting = $derived(
    deleteTracksMutation.isPending || deleteTracksWithFilesMutation.isPending
  );

  // トラック数
  const trackCount = $derived(tracks.length);

  // トラック名（単一の場合のみ表示）
  const trackName = $derived(trackCount === 1 ? tracks[0].title || tracks[0].fileName : null);

  /**
   * ライブラリから削除（データベースのみ）
   */
  async function handleDeleteFromLibrary() {
    const trackIds = tracks.map((t) => t.id);
    try {
      await deleteTracksMutation.mutateAsync(trackIds);
      // 削除成功後、ダイアログを閉じる
      open = false;
      onClose();
    } catch {
      // エラーはミューテーション内で処理済み
    }
  }

  /**
   * ファイルも削除（データベース + ファイルシステム。ゴミ箱へ移せる環境では、ゴミ箱へ移す）
   */
  async function handleDeleteWithFiles() {
    const trackIds = tracks.map((t) => t.id);
    try {
      await deleteTracksWithFilesMutation.mutateAsync(trackIds);
      // 削除成功後、ダイアログを閉じる
      open = false;
      onClose();
    } catch {
      // エラーはミューテーション内で処理済み
    }
  }

  /**
   * ダイアログを閉じる
   */
  function handleClose() {
    if (!isDeleting) {
      open = false;
      onClose();
    }
  }
</script>

<Modal
  {open}
  onClose={handleClose}
  title={m.deleteDialog.title}
  dismissible={!isDeleting}
  class="max-w-md"
>
  {#snippet titleIcon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="w-5 h-5 text-error"
    >
      <path d="M3 6h18" />
      <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
      <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
      <line x1="10" y1="11" x2="10" y2="17" />
      <line x1="14" y1="11" x2="14" y2="17" />
    </svg>
  {/snippet}

  <p class="mb-4">
    {#if trackName}
      {m.deleteDialog.confirm.before}<span class="font-semibold">{trackName}</span>{m.deleteDialog
        .confirm.after}
    {:else}
      {m.deleteDialog.confirmMany.before}<span class="font-semibold"
        >{m.common.trackCount(trackCount)}</span
      >{m.deleteDialog.confirmMany.after}
    {/if}
  </p>

  <div class="space-y-3">
    <!-- ライブラリから削除 -->
    <button class="option" onclick={handleDeleteFromLibrary} disabled={isDeleting}>
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="w-5 h-5"
      >
        <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" />
      </svg>
      <div class="text-left">
        <div class="font-medium">{m.deleteDialog.fromLibrary}</div>
        <div class="text-xs opacity-60">{m.deleteDialog.fromLibraryHint}</div>
      </div>
    </button>

    <!-- ファイルも削除（ゴミ箱へ移すか、完全に削除する） -->
    <button class="option option-danger" onclick={handleDeleteWithFiles} disabled={isDeleting}>
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="w-5 h-5"
      >
        <path d="M3 6h18" />
        <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
        <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
      </svg>
      <div class="text-left">
        <div class="font-medium">
          {movesToTrash ? m.deleteDialog.moveFilesToTrash : m.deleteDialog.withFiles}
        </div>
        <div class="text-xs opacity-60">
          {movesToTrash ? m.deleteDialog.moveFilesToTrashHint : m.deleteDialog.withFilesHint}
        </div>
      </div>
    </button>
  </div>

  {#if isDeleting}
    <div class="flex items-center justify-center gap-2 mt-4 text-sm opacity-60">
      <span class="deleting-spinner" aria-hidden="true"></span>
      <span>{m.common.deleting}</span>
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" class="btn-secondary" onclick={handleClose} disabled={isDeleting}>
      {m.common.cancel}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  /* 削除の方法を選ぶボタン（枠線だけ。ポインターを乗せると塗りつぶす） */
  .option {
    @apply flex items-center gap-3 w-full min-h-10 px-4 bg-transparent border border-current rounded text-sm font-semibold text-text-primary cursor-pointer transition-colors duration-200;
  }

  .option:hover:not(:disabled) {
    @apply bg-text-primary border-text-primary text-base-100;
  }

  .option:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .option-danger {
    @apply text-error;
  }

  .option-danger:hover:not(:disabled) {
    @apply bg-error border-error text-white;
  }

  .deleting-spinner {
    @apply w-5 h-5 rounded-full border-2 border-current border-t-transparent animate-spin;
  }
</style>
