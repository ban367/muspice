<!--
  転送先デバイスのページ: 接続の状態・同期する内容の選択・同期の実行。
  同期する内容の変更はすぐに保存する。
-->
<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    useRelinkSyncDeviceMutation,
    useRemoveSyncDeviceMutation,
    useSyncDevicesQuery,
    useUpdateSyncDeviceMutation
  } from '#lib/queries/devices.js';
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { handleError, showSuccess } from '#lib/stores/error.svelte.js';
  import type { SyncDevice, SyncDeviceConfig } from '#lib/types/models.js';
  import DeviceSyncDialog from '#lib/components/DeviceSyncDialog.svelte';
  import { isDeviceNameTooLong } from '#lib/utils/devices.js';
  import { confirmDestructive, promptText } from '#lib/utils/dialog.svelte.js';
  import { formatDateTime, formatFileSize } from '#lib/utils/format.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // URLからデバイスIDを取得
  const deviceId = $derived(page.params.id);

  // デバイスの抜き差しを反映するため、開いている間は一定の間隔で読み直す
  const devicesQuery = useSyncDevicesQuery({ poll: true });
  const playlistsQuery = usePlaylistsQuery();
  const updateMutation = useUpdateSyncDeviceMutation();
  const relinkMutation = useRelinkSyncDeviceMutation();
  const removeMutation = useRemoveSyncDeviceMutation();

  const device = $derived(devicesQuery.data?.find((d: SyncDevice) => d.id === deviceId) ?? null);
  const playlists = $derived(playlistsQuery.data ?? []);
  const hasSource = $derived(device !== null && (device.syncAll || device.playlistIds.length > 0));

  let isSyncDialogOpen = $state(false);

  /** 設定の一部を変えて保存する */
  function updateConfig(current: SyncDevice, patch: Partial<SyncDeviceConfig>) {
    updateMutation.mutate({
      deviceId: current.id,
      config: {
        name: current.name,
        syncAll: current.syncAll,
        playlistIds: current.playlistIds,
        removeUnselected: current.removeUnselected,
        ...patch
      }
    });
  }

  function togglePlaylist(current: SyncDevice, playlistId: string, selected: boolean) {
    const others = current.playlistIds.filter((id) => id !== playlistId);
    updateConfig(current, { playlistIds: selected ? [...others, playlistId] : others });
  }

  async function handleRename(current: SyncDevice) {
    const name = await promptText({
      title: m.devices.renameTitle,
      label: m.devices.nameLabel,
      defaultValue: current.name,
      confirmLabel: m.devices.renameConfirm,
      validate: (value) => (isDeviceNameTooLong(value) ? m.devices.nameTooLong : null)
    });
    if (name === null || name === current.name) return;

    updateConfig(current, { name });
  }

  /** 転送先のフォルダを選び直す（マウント先が変わった場合など） */
  async function handleRelink(current: SyncDevice) {
    let selected: string | string[] | null;
    try {
      selected = await open({
        directory: true,
        multiple: false,
        title: m.devices.selectFolderTitle
      });
    } catch (error) {
      handleError(error, m.devices.selectFolderFailed);
      return;
    }
    if (typeof selected !== 'string') return;

    try {
      await relinkMutation.mutateAsync({ deviceId: current.id, folderPath: selected });
      showSuccess(m.devices.relinked);
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  async function handleRemove(current: SyncDevice) {
    if (!(await confirmDestructive(m.devices.confirmRemove(current.name)))) return;

    try {
      await removeMutation.mutateAsync(current.id);
      showSuccess(m.devices.removed);
      goto(resolve('library/songs'));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<div class="device-page">
  {#if devicesQuery.isPending}
    <div class="centered">{m.common.loading}</div>
  {:else if !device}
    <div class="centered">
      <h2>{m.devices.notFound}</h2>
      <p>{m.devices.notFoundHint}</p>
    </div>
  {:else}
    <div class="device-header">
      <div class="device-info">
        <div class="device-icon" class:connected={device.connected}>
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="w-10 h-10"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M7 3h7l4 4v13a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1z"
            />
            <path stroke-linecap="round" d="M9.5 6.5v3M12 6.5v3M14.5 8.5v1" />
          </svg>
        </div>
        <div class="device-details">
          <h1 class="device-name">{device.name}</h1>
          <p class="device-path" title={device.path}>{device.path}</p>
          <p class="device-meta">
            <span class="connection" class:connected={device.connected}>
              {device.connected ? m.devices.connected : m.devices.disconnected}
            </span>
            {#if device.freeBytes !== null && device.totalBytes !== null}
              <span>
                {m.devices.freeSpace(
                  formatFileSize(device.freeBytes),
                  formatFileSize(device.totalBytes)
                )}
              </span>
            {/if}
            <span>
              {device.lastSyncedAt
                ? m.devices.lastSynced(formatDateTime(device.lastSyncedAt))
                : m.devices.neverSynced}
            </span>
          </p>
        </div>
      </div>
      <div class="device-actions">
        <button
          type="button"
          class="btn-sync"
          onclick={() => (isSyncDialogOpen = true)}
          disabled={!device.connected || !hasSource}
          title={device.connected && !hasSource ? m.devices.noSource : undefined}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="w-5 h-5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M4 4v5h5M20 20v-5h-5M5.5 15a7 7 0 0011.9 2.5L20 15M4 9l2.6-2.5A7 7 0 0118.5 9"
            />
          </svg>
          {m.devices.sync}
        </button>
        <button type="button" class="btn-secondary text-sm" onclick={() => handleRename(device)}>
          {m.devices.rename}
        </button>
        <button
          type="button"
          class="btn-secondary text-sm"
          onclick={() => handleRelink(device)}
          disabled={relinkMutation.isPending}
        >
          {m.devices.relink}
        </button>
        <button
          type="button"
          class="btn-secondary text-sm"
          onclick={() => handleRemove(device)}
          disabled={removeMutation.isPending}
        >
          {m.devices.remove}
        </button>
      </div>
    </div>

    {#if !device.connected}
      <p class="message-info disconnected-hint">{m.devices.disconnectedHint}</p>
    {/if}

    <section class="source-section">
      <h2 class="source-title">{m.devices.sourceTitle}</h2>
      <p class="hint source-hint">{m.devices.layoutHint}</p>

      <label class="option">
        <input
          type="checkbox"
          class="option-checkbox"
          checked={device.syncAll}
          onchange={(e) => updateConfig(device, { syncAll: e.currentTarget.checked })}
        />
        {m.devices.syncAll}
      </label>

      <h3 class="group-title">{m.devices.playlists}</h3>
      <p class="hint group-hint">{m.devices.playlistsHint}</p>
      {#if playlistsQuery.isPending}
        <p class="hint">{m.common.loading}</p>
      {:else if playlists.length === 0}
        <p class="hint">{m.devices.noPlaylists}</p>
      {:else}
        <ul class="playlist-list">
          {#each playlists as playlist (playlist.id)}
            <li>
              <label class="option playlist-option">
                <input
                  type="checkbox"
                  class="option-checkbox"
                  checked={device.playlistIds.includes(playlist.id)}
                  onchange={(e) => togglePlaylist(device, playlist.id, e.currentTarget.checked)}
                />
                <span class="flex-1 truncate">{playlist.name}</span>
                <span class="hint shrink-0">
                  {playlist.rules === null
                    ? m.common.trackCount(playlist.tracks.length)
                    : m.smartPlaylist.badge}
                </span>
              </label>
            </li>
          {/each}
        </ul>
      {/if}

      <label class="option remove-option">
        <input
          type="checkbox"
          class="option-checkbox"
          checked={device.removeUnselected}
          onchange={(e) => updateConfig(device, { removeUnselected: e.currentTarget.checked })}
        />
        {m.devices.removeUnselected}
      </label>
      <p class="hint option-hint">{m.devices.removeUnselectedHint}</p>
    </section>

    <DeviceSyncDialog open={isSyncDialogOpen} {device} onClose={() => (isSyncDialogOpen = false)} />
  {/if}
</div>

<style>
  @reference "../../../../app.css";

  .device-page {
    @apply h-full flex flex-col;
  }

  .centered {
    @apply flex-1 flex flex-col items-center justify-center text-text-muted text-center p-8;
  }

  .centered h2 {
    @apply m-0 mb-2 text-2xl text-text-secondary;
  }

  .centered p {
    @apply m-0 text-text-dimmed;
  }

  .device-header {
    @apply flex flex-wrap items-center justify-between gap-4 p-6 rounded-lg mb-4;
    background: linear-gradient(135deg, var(--color-base-200) 0%, var(--color-base-300) 100%);
  }

  .device-info {
    @apply flex items-center gap-4 min-w-0;
  }

  .device-icon {
    @apply w-20 h-20 shrink-0 flex items-center justify-center rounded-lg bg-base-400 text-text-muted;
  }

  .device-icon.connected {
    @apply bg-primary/20 text-primary;
  }

  .device-details {
    @apply flex flex-col gap-1 min-w-0;
  }

  .device-name {
    @apply m-0 text-2xl font-bold text-text-primary truncate;
  }

  .device-path {
    @apply m-0 text-xs text-text-muted font-mono truncate;
  }

  .device-meta {
    @apply m-0 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-text-muted;
  }

  /* 接続の状態（`status`はDaisyUIのコンポーネントと名前が重なるため使わない） */
  .connection {
    @apply inline-flex items-center gap-1.5;
  }

  .connection::before {
    content: '';
    @apply w-2 h-2 rounded-full bg-text-dimmed;
  }

  .connection.connected {
    @apply text-success;
  }

  .connection.connected::before {
    @apply bg-success;
  }

  .device-actions {
    @apply flex flex-wrap items-center gap-2;
  }

  .btn-sync {
    @apply flex items-center gap-2 px-6 py-3 bg-success text-white border-none rounded-full text-sm font-semibold cursor-pointer transition-all;
  }

  .btn-sync:hover:not(:disabled) {
    @apply bg-success/80;
  }

  .btn-sync:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .source-section {
    @apply flex-1 overflow-y-auto px-2 pb-4 max-w-2xl;
  }

  .source-title {
    @apply text-lg font-semibold text-text-primary m-0 mb-1;
  }

  .group-title {
    @apply text-sm font-semibold text-text-secondary m-0 mt-6 mb-1;
  }

  .disconnected-hint {
    @apply m-0 mb-4;
  }

  .hint {
    @apply text-xs text-text-muted m-0;
  }

  .source-hint {
    @apply mb-4;
  }

  .group-hint {
    @apply mb-2;
  }

  .remove-option {
    @apply mt-6;
  }

  .option {
    @apply flex items-center gap-2 text-sm font-medium text-text-primary cursor-pointer;
  }

  .option-checkbox {
    @apply w-4 h-4 shrink-0 accent-primary cursor-pointer;
  }

  .option-hint {
    @apply mt-1 ml-6;
  }

  .playlist-list {
    @apply list-none m-0 p-0 border border-border rounded-md divide-y divide-border;
  }

  .playlist-option {
    @apply px-3 py-2 font-normal;
  }

  .playlist-option:hover {
    @apply bg-surface-hover;
  }
</style>
