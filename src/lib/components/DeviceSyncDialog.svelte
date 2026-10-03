<!--
  @component DeviceSyncDialog
  デバイスへの同期のダイアログ。
  開くと差分（コピー・削除する曲数と必要な容量）を調べて表示し、確認の後に同期する。
  同期中は進捗を表示し、中止できる（閉じる操作は受け付けない）。
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { events } from '#lib/bindings.js';
  import {
    cancelDeviceSync,
    usePlanDeviceSyncMutation,
    useRunDeviceSyncMutation
  } from '#lib/queries/devices.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import type {
    DeviceSyncPlan,
    DeviceSyncProgress,
    DeviceSyncResult,
    SyncDevice
  } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { isDeviceUpToDate, missingSpaceBytes, syncProgressPercent } from '#lib/utils/devices.js';
  import { formatFileSize } from '#lib/utils/format.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 表示するか */
    open: boolean;
    /** 同期するデバイス */
    device: SyncDevice;
    /** 閉じる操作があったときに呼ばれる */
    onClose: () => void;
  }

  let { open, device, onClose }: Props = $props();

  const planMutation = usePlanDeviceSyncMutation();
  const runMutation = useRunDeviceSyncMutation();

  let plan = $state<DeviceSyncPlan | null>(null);
  let planError = $state('');
  let isSyncing = $state(false);
  let isStopping = $state(false);
  let progress = $state<DeviceSyncProgress | null>(null);
  let result = $state<DeviceSyncResult | null>(null);
  let syncError = $state('');

  const messages = $derived(m.devices.syncDialog);
  const percent = $derived(progress ? syncProgressPercent(progress) : 0);
  // コピーが済んだ後（プレイリストの書き出し・管理ファイルの保存）
  const isFinishing = $derived(progress !== null && progress.current >= progress.total);

  // 開いたときに差分を調べる（デバイスの一覧の読み直しでは調べ直さない）
  $effect(() => {
    if (open) untrack(() => void loadPlan(device.id));
  });

  async function loadPlan(deviceId: string) {
    plan = null;
    planError = '';
    progress = null;
    result = null;
    syncError = '';
    isStopping = false;
    try {
      plan = await planMutation.mutateAsync(deviceId);
    } catch (error) {
      planError = messages.planFailed(toErrorMessage(error));
    }
  }

  async function startSync() {
    const deviceId = device.id;
    isSyncing = true;
    isStopping = false;
    progress = null;
    syncError = '';

    let unlisten: (() => void) | null = null;
    try {
      unlisten = await events.deviceSyncProgress.listen((event) => {
        if (event.payload.deviceId === deviceId) progress = event.payload;
      });
      result = await runMutation.mutateAsync(deviceId);
    } catch (error) {
      syncError = messages.failed(toErrorMessage(error));
    } finally {
      isSyncing = false;
      unlisten?.();
    }
  }

  async function stopSync() {
    isStopping = true;
    try {
      await cancelDeviceSync();
    } catch {
      // 失敗はトースト通知済み。もう一度中止できるようにする
      isStopping = false;
    }
  }
</script>

<Modal {open} {onClose} title={messages.title(device.name)} dismissible={!isSyncing}>
  {#if result}
    <!-- 結果 -->
    <h4 class="result-title" class:cancelled={result.cancelled}>
      {result.cancelled ? messages.cancelled : messages.completed}
    </h4>
    {#if result.cancelled}
      <p class="hint cancelled-hint">{messages.cancelledHint}</p>
    {/if}
    <dl class="summary">
      <div class="summary-row">
        <dt>{messages.copied}</dt>
        <dd>{m.common.trackCount(result.copiedCount)}</dd>
      </div>
      <div class="summary-row">
        <dt>{messages.deleted}</dt>
        <dd>{m.common.trackCount(result.deletedCount)}</dd>
      </div>
      {#if result.renamedCount > 0}
        <div class="summary-row">
          <dt>{messages.renamed}</dt>
          <dd>{m.common.trackCount(result.renamedCount)}</dd>
        </div>
      {/if}
      <div class="summary-row">
        <dt>{messages.playlistsWritten}</dt>
        <dd>{m.common.itemCount(result.playlistCount)}</dd>
      </div>
      {#if result.errorCount > 0}
        <div class="summary-row">
          <dt>{messages.errors}</dt>
          <dd class="text-error-light">{m.common.itemCount(result.errorCount)}</dd>
        </div>
      {/if}
    </dl>
    {#if result.errors.length > 0}
      <div class="message-error notice text-left">
        <p class="font-semibold m-0 mb-2">{messages.errorDetails}</p>
        <ul class="m-0 pl-6 break-all">
          {#each result.errors as error, i (i)}
            <li class="my-1">{error}</li>
          {/each}
        </ul>
      </div>
    {/if}
  {:else if isSyncing}
    <!-- 進捗 -->
    <p class="text-center text-text-secondary mb-2" role="status">
      {#if isStopping}
        {messages.stopping}
      {:else if !progress}
        {messages.preparing}
      {:else if isFinishing}
        {messages.finishing}
      {:else}
        {messages.copying(progress.current + 1, progress.total)}
      {/if}
    </p>
    <div class="progress-bar-container">
      <div class="progress-bar-fill" style="width: {percent}%"></div>
    </div>
    <div class="mt-2 flex flex-col items-center gap-1">
      <p class="text-center text-primary font-semibold m-0">{percent}%</p>
      {#if progress && progress.bytesTotal > 0}
        <p class="hint text-center">
          {formatFileSize(progress.bytesDone)} / {formatFileSize(progress.bytesTotal)}
        </p>
      {/if}
      {#if progress?.currentFile}
        <p class="text-center text-text-dimmed text-xs m-0 max-w-full truncate">
          {progress.currentFile}
        </p>
      {/if}
    </div>
  {:else if plan}
    <!-- 同期の前の確認 -->
    <dl class="summary">
      <div class="summary-row">
        <dt>{messages.copy}</dt>
        <dd>{messages.countAndSize(plan.copyCount, formatFileSize(plan.copyBytes))}</dd>
      </div>
      <div class="summary-row">
        <dt>{messages.delete}</dt>
        <dd>{messages.countAndSize(plan.deleteCount, formatFileSize(plan.deleteBytes))}</dd>
      </div>
      {#if plan.renameCount > 0}
        <div class="summary-row">
          <dt>{messages.rename}</dt>
          <dd>{m.common.trackCount(plan.renameCount)}</dd>
        </div>
      {/if}
      <div class="summary-row">
        <dt>{messages.unchanged}</dt>
        <dd>{m.common.trackCount(plan.unchangedCount)}</dd>
      </div>
      <div class="summary-row">
        <dt>{messages.playlists}</dt>
        <dd>{m.common.itemCount(plan.playlistCount)}</dd>
      </div>
      <div class="summary-row">
        <dt>{messages.freeSpace}</dt>
        <dd>{formatFileSize(plan.freeBytes)}</dd>
      </div>
    </dl>
    {#if isDeviceUpToDate(plan)}
      <p class="hint notice">{messages.upToDate}</p>
    {/if}
    {#if plan.missingSourceCount > 0}
      <p class="message-warning notice">{messages.missingSources(plan.missingSourceCount)}</p>
    {/if}
    {#if !plan.hasEnoughSpace}
      <p class="message-error notice">
        {messages.notEnoughSpace(formatFileSize(missingSpaceBytes(plan)))}
      </p>
    {/if}
    {#if syncError}
      <p class="message-error notice">{syncError}</p>
    {/if}
  {:else if planError}
    <p class="message-error plan-error">{planError}</p>
  {:else}
    <p class="text-center text-text-secondary m-0" role="status">{messages.planning}</p>
  {/if}

  {#snippet footer()}
    {#if result}
      <button type="button" class="btn-success" onclick={onClose} data-autofocus>
        {m.common.close}
      </button>
    {:else if isSyncing}
      <button type="button" class="btn-secondary" onclick={stopSync} disabled={isStopping}>
        {isStopping ? messages.stopping : messages.stop}
      </button>
    {:else}
      <button type="button" class="btn-secondary" onclick={onClose}>{m.common.cancel}</button>
      <button
        type="button"
        class="btn-primary"
        onclick={startSync}
        disabled={!plan || !plan.hasEnoughSpace}
        data-autofocus
      >
        {messages.start}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .result-title {
    @apply text-xl font-semibold text-secondary text-center m-0 mb-4;
  }

  .result-title.cancelled {
    @apply text-text-primary;
  }

  /* 余白はここで指定する（スコープ付きのスタイルは、テンプレートのユーティリティより優先されるため） */
  .hint {
    @apply text-xs text-text-muted m-0;
  }

  .cancelled-hint {
    @apply text-center mb-4;
  }

  .notice {
    @apply m-0 mt-4;
  }

  .plan-error {
    @apply m-0;
  }

  .summary {
    @apply flex flex-col gap-2 m-0;
  }

  .summary-row {
    @apply flex justify-between py-2.5 px-4 bg-base-400 rounded-md;
  }

  .summary-row dt {
    @apply text-text-secondary;
  }

  .summary-row dd {
    @apply m-0 font-semibold text-text-primary;
  }

  .message-warning {
    @apply p-3 bg-warning/20 text-warning rounded-md text-sm;
  }

  .progress-bar-container {
    @apply w-full h-6 bg-base-400 rounded-full overflow-hidden;
  }

  .progress-bar-fill {
    @apply h-full bg-primary transition-[width] duration-300;
  }
</style>
