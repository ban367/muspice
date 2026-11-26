<!--
  @component DeviceContextMenu
  サイドバーの転送先デバイスの右クリックのメニュー（ADR-137）。
  名前の変更・取り出し（取り出せる時だけ）・登録の解除を置く。デバイスのページには、同期のボタンだけを置く。
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import type { SyncDevice } from '#lib/types/models.js';
  import { BaseContextMenu, MenuItem, MenuSeparator } from '#lib/components/ui/index.js';
  import {
    useEjectSyncDeviceMutation,
    useRemoveSyncDeviceMutation,
    useUpdateSyncDeviceMutation
  } from '#lib/queries/devices.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import { deviceConfigOf, isDeviceNameTooLong } from '#lib/utils/devices.js';
  import { confirmDestructive, promptText } from '#lib/utils/dialog.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    x: number;
    y: number;
    device: SyncDevice;
    onClose: () => void;
  }

  let { x, y, device, onClose }: Props = $props();

  const updateMutation = useUpdateSyncDeviceMutation();
  const ejectMutation = useEjectSyncDeviceMutation();
  const removeMutation = useRemoveSyncDeviceMutation();

  async function handleRename() {
    // メニューを閉じるとこのコンポーネントは破棄されるため、使う値は先に取り出しておく
    const current = device;
    onClose();
    const name = await promptText({
      title: m.devices.renameTitle,
      label: m.devices.nameLabel,
      defaultValue: current.name,
      confirmLabel: m.devices.renameConfirm,
      validate: (value) => (isDeviceNameTooLong(value) ? m.devices.nameTooLong : null)
    });
    if (name === null || name === current.name) return;

    updateMutation.mutate({ deviceId: current.id, config: deviceConfigOf(current, { name }) });
  }

  async function handleEject() {
    const { id, name } = device;
    onClose();
    try {
      await ejectMutation.mutateAsync(id);
      showSuccess(m.devices.ejected(name));
    } catch {
      // トーストで知らせ済み
    }
  }

  async function handleRemove() {
    const { id, name } = device;
    // 開いているのが、そのデバイスのページか（登録を解除したら、ページを離れる）
    const isOpen = page.url.pathname === `/devices/${id}`;
    onClose();
    if (!(await confirmDestructive(m.devices.confirmRemove(name), m.devices.removeConfirm))) {
      return;
    }

    try {
      await removeMutation.mutateAsync(id);
      showSuccess(m.devices.removed);
      if (isOpen) goto(resolve('library/songs'));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-title">{device.name}</div>
  <div class="menu-subtitle">
    {device.connected ? m.devices.connected : m.devices.disconnected}
  </div>
  <MenuSeparator />

  <MenuItem onSelect={handleRename}>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
        />
      </svg>
    {/snippet}
    {m.devices.renameEllipsis}
  </MenuItem>

  <!-- 取り出せるのは、macOSで、`/Volumes`の下のドライブにあり、接続されているデバイス（ADR-113） -->
  {#if device.ejectable}
    <MenuItem onSelect={handleEject} disabled={ejectMutation.isPending}>
      {#snippet icon()}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="menu-icon"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5l7 8H5z" />
          <path stroke-linecap="round" stroke-width="2" d="M5 18h14" />
        </svg>
      {/snippet}
      {m.devices.eject}
    </MenuItem>
  {/if}

  <MenuSeparator />

  <MenuItem onSelect={handleRemove} danger>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M15 12H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
    {/snippet}
    {m.devices.removeEllipsis}
  </MenuItem>
</BaseContextMenu>
