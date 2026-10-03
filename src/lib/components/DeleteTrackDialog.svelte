<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    useDeleteTracksMutation,
    useDeleteTracksWithFilesMutation
  } from '#lib/queries/tracks.js';
  import { Button, Modal } from '#lib/components/ui/index.js';

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
   * ファイルも削除（データベース + ファイルシステム）
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
  title="トラックの削除"
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
      「<span class="font-semibold">{trackName}</span>」を削除しますか？
    {:else}
      <span class="font-semibold">{trackCount}曲</span>を削除しますか？
    {/if}
  </p>

  <div class="space-y-3">
    <!-- ライブラリから削除 -->
    <button
      class="btn btn-outline w-full justify-start gap-3"
      onclick={handleDeleteFromLibrary}
      disabled={isDeleting}
    >
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
        <div class="font-medium">ライブラリから削除</div>
        <div class="text-xs opacity-60">ファイルはそのまま残ります</div>
      </div>
    </button>

    <!-- ファイルも削除 -->
    <button
      class="btn btn-error btn-outline w-full justify-start gap-3"
      onclick={handleDeleteWithFiles}
      disabled={isDeleting}
    >
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
        <div class="font-medium">ファイルも削除</div>
        <div class="text-xs opacity-60">この操作は取り消せません</div>
      </div>
    </button>
  </div>

  {#if isDeleting}
    <div class="flex items-center justify-center gap-2 mt-4 text-sm opacity-60">
      <span class="loading loading-spinner loading-sm"></span>
      <span>削除中...</span>
    </div>
  {/if}

  {#snippet footer()}
    <Button variant="ghost" onclick={handleClose} disabled={isDeleting}>キャンセル</Button>
  {/snippet}
</Modal>
