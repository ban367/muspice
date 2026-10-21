<!--
  @component LibraryBackupSettings
  設定ウィンドウの「ライブラリ」: ライブラリのデータ（DB・設定）のバックアップの保存と、バックアップからの復元。
  保存先・復元するファイルはRust側のダイアログで選ぶ（WebViewからパスを渡さない）。復元は、選んだファイルの
  中身を見せて確かめてから行い、アプリが再起動する。
  `children`は、ボタンの下に出す（設定の「適用」で保存する、自動のバックアップの項目）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    cancelLibraryRestore,
    useExportLibraryBackupMutation,
    usePlanLibraryRestoreMutation,
    useRestoreLibraryBackupMutation
  } from '#lib/queries/backup.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import type { RestorePlan } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { formatDateTime } from '#lib/utils/format.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** ボタンの下に出す内容 */
    children?: Snippet;
  }

  let { children }: Props = $props();

  const exportMutation = useExportLibraryBackupMutation();
  const planMutation = usePlanLibraryRestoreMutation();
  const restoreMutation = useRestoreLibraryBackupMutation();

  /** 復元を確かめているファイル（Rust側が預かっている） */
  let restoreTarget = $state<Extract<RestorePlan, { kind: 'ready' }> | null>(null);
  /** 復元の用意ができ、再起動を待っているか（アプリが終わるまで、操作を止める） */
  let isRestarting = $state(false);

  const isBusy = $derived(
    exportMutation.isPending || planMutation.isPending || restoreMutation.isPending || isRestarting
  );

  async function saveBackup() {
    try {
      const saved = await exportMutation.mutateAsync();
      if (saved) {
        showSuccess(m.backup.saved(saved.path, saved.trackCount, saved.playlistCount));
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  async function chooseRestore() {
    try {
      const plan = await planMutation.mutateAsync();
      if (!plan) return;
      switch (plan.kind) {
        case 'ready':
          restoreTarget = plan;
          break;
        case 'notBackup':
          showWarning(m.backup.notBackup(plan.fileName));
          break;
        case 'damaged':
          showWarning(m.backup.damaged(plan.fileName));
          break;
        case 'newerFormat':
          showWarning(m.backup.newerFormat(plan.fileName));
          break;
      }
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  function cancelRestore() {
    if (restoreMutation.isPending || isRestarting) return;
    restoreTarget = null;
    cancelLibraryRestore();
  }

  async function confirmRestore() {
    try {
      await restoreMutation.mutateAsync();
      // アプリが再起動するまで、画面を閉じずに待つ
      isRestarting = true;
    } catch {
      // 失敗はミューテーション内でトースト通知済み（預かったファイルは、Rust側が手放している）
      restoreTarget = null;
    }
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.backup.title}</h4>
  <p class="setting-description">{m.backup.description}</p>
  <div class="backup-actions">
    <button type="button" class="btn-secondary text-sm" onclick={saveBackup} disabled={isBusy}>
      {exportMutation.isPending ? m.backup.saving : m.backup.save}
    </button>
    <button type="button" class="btn-secondary text-sm" onclick={chooseRestore} disabled={isBusy}>
      {m.backup.restore}
    </button>
  </div>
  {@render children?.()}
</section>

<Modal
  open={restoreTarget !== null}
  onClose={cancelRestore}
  title={m.backup.restoreTitle}
  dismissible={!restoreMutation.isPending && !isRestarting}
  class="max-w-lg"
>
  {#if restoreTarget}
    <dl class="restore-details">
      <dt>{m.backup.restoreFile}</dt>
      <dd class="font-mono">{restoreTarget.fileName}</dd>
      <dt>{m.backup.restoreCreatedAt}</dt>
      <dd>
        {restoreTarget.createdAt
          ? formatDateTime(restoreTarget.createdAt)
          : m.backup.restoreUnknownDate}
      </dd>
      <dt>{m.backup.restoreContents}</dt>
      <dd>
        {m.backup.restoreContentsValue(
          restoreTarget.trackCount,
          restoreTarget.playlistCount,
          restoreTarget.libraryFolderCount
        )}
      </dd>
    </dl>
    <p class="text-sm mb-2 m-0">{m.backup.restoreConfirm}</p>
    <p class="text-xs text-text-muted mb-2 m-0">
      {restoreTarget.includesSettings ? m.backup.restoreSettings : m.backup.restoreNoSettings}
    </p>
    <p class="text-xs text-text-muted m-0">{m.backup.restoreFoldersHint}</p>
    {#if isRestarting}
      <p class="restore-status" role="status">{m.backup.restarting}</p>
    {/if}
  {/if}

  {#snippet footer()}
    <button
      type="button"
      class="btn-secondary"
      onclick={cancelRestore}
      disabled={restoreMutation.isPending || isRestarting}
      data-autofocus
    >
      {m.common.cancel}
    </button>
    <button
      type="button"
      class="btn-danger"
      onclick={confirmRestore}
      disabled={restoreMutation.isPending || isRestarting}
    >
      {m.backup.restoreButton}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  /* 設定画面（settings/+page.svelte）の項目と同じ見た目にする */
  .settings-section {
    @apply max-w-xl;
  }

  .subsection-title {
    @apply text-base font-semibold mt-8 mb-4;
  }

  .setting-description {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .backup-actions {
    @apply flex flex-wrap gap-2 mt-3 mb-4;
  }

  .restore-details {
    @apply grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm mb-4 m-0;
  }

  .restore-details dt {
    @apply text-text-muted;
  }

  .restore-details dd {
    @apply m-0 break-all text-text-primary;
  }

  .restore-status {
    @apply text-xs text-text-secondary mt-3 m-0;
  }
</style>
