<!--
  転送先デバイスのページ: 接続の状態・同期する内容の選択・同期の実行・配置（転送先のフォルダと書式）。
  同期する内容の変更はすぐに保存する。名前の変更・取り出し・登録の解除は、サイドバーの右クリックの
  メニュー（`DeviceContextMenu`）に置き、見出しには同期のボタンだけを置く（ADR-137）。
-->
<script lang="ts">
  import { page } from '$app/state';
  import {
    useRelinkSyncDeviceMutation,
    useSyncDevicesQuery,
    useSyncSelectionSummaryQuery,
    useUpdateSyncDeviceMutation
  } from '#lib/queries/devices.js';
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { useSmartPlaylistsQuery } from '#lib/queries/smartPlaylists.js';
  import { playlistSidebar } from '#lib/stores/playlistSidebar.svelte.js';
  import { sortPlaylistItems } from '#lib/utils/playlistTree.js';
  import { useConvertFormatsQuery } from '#lib/queries/convert.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import type {
    PlaylistFormat,
    SyncDevice,
    SyncDeviceConfig,
    TranscodeMode
  } from '#lib/types/models.js';
  import DeviceSyncDialog from '#lib/components/DeviceSyncDialog.svelte';
  import MusicFolderTemplateSetting from '#lib/components/MusicFolderTemplateSetting.svelte';
  import { AAC_BITRATES_KBPS, DEFAULT_DEVICE_TEMPLATE, DEVICE_MUSIC_DIR } from '#lib/bindings.js';
  import { deviceConfigOf } from '#lib/utils/devices.js';
  import { formatDateTime, formatFileSize } from '#lib/utils/format.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // URLからデバイスIDを取得
  const deviceId = $derived(page.params.id);

  // デバイスの抜き差しを反映するため、開いている間は一定の間隔で読み直す
  const devicesQuery = useSyncDevicesQuery({ poll: true });
  const playlistsQuery = usePlaylistsQuery();
  const smartPlaylistsQuery = useSmartPlaylistsQuery();
  const updateMutation = useUpdateSyncDeviceMutation();
  const relinkMutation = useRelinkSyncDeviceMutation();
  // 同期する内容の曲数とサイズ（ライブラリの記録から求める見通し。ADR-113）
  const selectionQuery = useSyncSelectionSummaryQuery(() => deviceId ?? '');

  const device = $derived(devicesQuery.data?.find((d: SyncDevice) => d.id === deviceId) ?? null);
  // 転送する時の変換は、変換できる環境（macOS）でだけ出す（ADR-118）
  const convertFormatsQuery = useConvertFormatsQuery();
  const canTranscode = $derived((convertFormatsQuery.data?.length ?? 0) > 0);
  const transcodeModes: TranscodeMode[] = ['off', 'lossless', 'aboveBitrate'];
  const playlistFormats: PlaylistFormat[] = ['m3u8', 'm3u'];
  const playlists = $derived(playlistsQuery.data ?? []);
  // オートプレイリストは、サイドバーと同じ並び順で、プレイリストの前に出す（ADR-127）
  const smartPlaylists = $derived(
    sortPlaylistItems(smartPlaylistsQuery.data ?? [], playlistSidebar.smartSort)
  );
  const hasSource = $derived(
    device !== null &&
      (device.syncAll || device.playlistIds.length > 0 || device.smartPlaylistIds.length > 0)
  );

  let isSyncDialogOpen = $state(false);

  /** 設定の一部を変えて保存する */
  function updateConfig(current: SyncDevice, patch: Partial<SyncDeviceConfig>) {
    updateMutation.mutate({ deviceId: current.id, config: deviceConfigOf(current, patch) });
  }

  // 配置の書式（入力中の値。「書式を適用」で保存する。別のデバイスを開いたら、そのデバイスの書式に戻す）
  let draftTemplate = $state('');
  let draftDeviceId: string | null = null;
  $effect(() => {
    if (device && device.id !== draftDeviceId) {
      draftDeviceId = device.id;
      draftTemplate = device.pathTemplate;
    }
  });
  const isTemplateChanged = $derived(
    device !== null && draftTemplate.trim() !== device.pathTemplate
  );

  function applyTemplate(current: SyncDevice) {
    updateMutation.mutate(
      {
        deviceId: current.id,
        config: deviceConfigOf(current, { pathTemplate: draftTemplate.trim() })
      },
      { onSuccess: () => showSuccess(m.devices.layoutApplied) }
    );
  }

  function togglePlaylist(current: SyncDevice, playlistId: string, selected: boolean) {
    const others = current.playlistIds.filter((id) => id !== playlistId);
    updateConfig(current, { playlistIds: selected ? [...others, playlistId] : others });
  }

  function toggleSmartPlaylist(current: SyncDevice, smartPlaylistId: string, selected: boolean) {
    const others = current.smartPlaylistIds.filter((id) => id !== smartPlaylistId);
    updateConfig(current, { smartPlaylistIds: selected ? [...others, smartPlaylistId] : others });
  }

  /** 転送先のフォルダを選び直す（マウント先が変わった場合など。ダイアログはRust側が開く） */
  async function handleRelink(current: SyncDevice) {
    try {
      const relinked = await relinkMutation.mutateAsync(current.id);
      // 選ばずに閉じた場合は、何もしない
      if (relinked) showSuccess(m.devices.relinked);
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
      </div>
    </div>

    {#if !device.connected}
      <p class="message-info disconnected-hint">{m.devices.disconnectedHint}</p>
    {/if}

    <section class="source-section">
      <h2 id="device-source-title" class="source-title">{m.devices.sourceTitle}</h2>
      <!-- 選んだ内容の曲数とサイズ（コピー済みの曲・歌詞と画像を含めない、ライブラリでの合計） -->
      <p class="hint source-hint">
        {#if selectionQuery.data && hasSource}
          {m.devices.selectionSummary(
            selectionQuery.data.trackCount,
            formatFileSize(selectionQuery.data.totalBytes)
          )}
          {#if device.totalBytes !== null && selectionQuery.data.totalBytes > device.totalBytes}
            <span class="text-warning">
              · {m.devices.selectionTooLarge(formatFileSize(device.totalBytes))}
            </span>
          {/if}
        {:else}
          &nbsp;
        {/if}
      </p>

      <!-- 全曲か、選んだプレイリストかのどちらか一方で同期する（ADR-135） -->
      <div class="source-choice" role="radiogroup" aria-labelledby="device-source-title">
        <label class="option source-option">
          <input
            type="radio"
            name="device-source"
            class="option-checkbox mt-0.5"
            checked={device.syncAll}
            onchange={() => updateConfig(device, { syncAll: true })}
          />
          <span>
            <span class="block">{m.devices.syncAll}</span>
            <span class="hint block font-normal">{m.devices.syncAllHint}</span>
          </span>
        </label>
        <label class="option source-option">
          <input
            type="radio"
            name="device-source"
            class="option-checkbox mt-0.5"
            checked={!device.syncAll}
            onchange={() => updateConfig(device, { syncAll: false })}
          />
          {m.devices.syncPlaylists}
        </label>
      </div>

      <!-- 選ぶプレイリストは、「選んだプレイリスト」の時だけ出す -->
      {#if device.syncAll}
        <!-- 全曲の時は、すべてのプレイリストを書き出すため、選ぶものはない -->
      {:else if playlistsQuery.isPending || smartPlaylistsQuery.isPending}
        <p class="hint playlist-choice">{m.common.loading}</p>
      {:else if playlists.length === 0 && smartPlaylists.length === 0}
        <p class="hint playlist-choice">{m.devices.noPlaylists}</p>
      {:else}
        <ul class="playlist-list playlist-choice" aria-label={m.devices.syncPlaylists}>
          {#each smartPlaylists as smartPlaylist (smartPlaylist.id)}
            <li>
              <label class="option playlist-option">
                <input
                  type="checkbox"
                  class="option-checkbox"
                  checked={device.smartPlaylistIds.includes(smartPlaylist.id)}
                  onchange={(e) =>
                    toggleSmartPlaylist(device, smartPlaylist.id, e.currentTarget.checked)}
                />
                <span class="flex-1 truncate">{smartPlaylist.name}</span>
                <span class="hint shrink-0">{m.smartPlaylist.badge}</span>
              </label>
            </li>
          {/each}
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
                <span class="hint shrink-0">{m.common.trackCount(playlist.tracks.length)}</span>
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
      <!-- 利用者が置いたファイルを消さないことは、項目名から分からないため添える -->
      <p class="hint option-hint">{m.devices.removeUnselectedHint}</p>

      <label class="option remove-option">
        <input
          type="checkbox"
          class="option-checkbox"
          checked={device.copyExtras}
          onchange={(e) => updateConfig(device, { copyExtras: e.currentTarget.checked })}
        />
        {m.devices.copyExtras}
      </label>

      <!-- 書き出すプレイリストの形式（中身は同じで、拡張子だけが違う。ADR-138） -->
      <div class="option select-option">
        <label for="device-playlist-format">{m.devices.playlistFormat}</label>
        <select
          id="device-playlist-format"
          class="form-input option-select"
          value={device.playlistFormat}
          onchange={(e) =>
            updateConfig(device, {
              playlistFormat: e.currentTarget.value as PlaylistFormat
            })}
        >
          {#each playlistFormats as format (format)}
            <option value={format}>{m.devices.playlistFormats[format]}</option>
          {/each}
        </select>
      </div>

      <!-- 転送する時の変換（変換できる環境だけ。ADR-118） -->
      {#if canTranscode}
        <div class="option select-option">
          <label for="device-transcode">{m.devices.transcode}</label>
          <select
            id="device-transcode"
            class="form-input option-select"
            value={device.transcode.mode}
            onchange={(e) =>
              updateConfig(device, {
                transcode: {
                  ...device.transcode,
                  mode: e.currentTarget.value as TranscodeMode
                }
              })}
          >
            {#each transcodeModes as mode (mode)}
              <option value={mode}>{m.devices.transcodeModes[mode]}</option>
            {/each}
          </select>
          {#if device.transcode.mode !== 'off'}
            <select
              class="form-input option-select"
              aria-label={m.devices.transcodeBitrate}
              value={String(device.transcode.bitrateKbps)}
              onchange={(e) =>
                updateConfig(device, {
                  transcode: { ...device.transcode, bitrateKbps: Number(e.currentTarget.value) }
                })}
            >
              {#each AAC_BITRATES_KBPS as bitrate (bitrate)}
                <option value={String(bitrate)}>{bitrate} kbps</option>
              {/each}
            </select>
          {/if}
        </div>
      {/if}

      <!-- デバイス上の配置の書式（例は、最近追加したアルバムの曲を、デバイスのフォルダからの場所で出す。
           曲は必ずMusicフォルダの中に置く。ADR-111・ADR-135） -->
      <h3 class="group-title layout-title">{m.devices.layoutTitle}</h3>
      <!-- 転送先のフォルダ（マウント先が変わった時・カードを初期化した時に選び直す。ダイアログはRust側が開く） -->
      <div class="destination">
        <span class="destination-label">{m.devices.destination}</span>
        <p class="destination-hint">{m.devices.destinationHint}</p>
        <div class="destination-row">
          <span class="destination-path" title={device.path}>{device.path}</span>
          <button
            type="button"
            class="btn-secondary text-sm shrink-0"
            onclick={() => handleRelink(device)}
            disabled={relinkMutation.isPending}
          >
            {m.devices.relink}
          </button>
        </div>
      </div>
      <MusicFolderTemplateSetting
        bind:template={draftTemplate}
        id="device-template"
        label={m.devices.layoutTemplate}
        hint={m.devices.layoutTemplateHint}
        defaultTemplate={DEFAULT_DEVICE_TEMPLATE}
        examplePrefix={`${DEVICE_MUSIC_DIR}/`}
      />
      <button
        type="button"
        class="btn-primary text-sm"
        onclick={() => applyTemplate(device)}
        disabled={!isTemplateChanged || updateMutation.isPending}
      >
        {m.devices.applyLayout}
      </button>
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

  .device-meta {
    @apply m-0 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-text-muted;
  }

  /* 接続の状態 */
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

  /* 見出しと同じく、ページの幅いっぱいに使う（ほかのメインウィンドウの画面と同じ。ADR-137）。
     右は、重ねて出るスクロールバーが中身（一覧の枠・ボタン）にかからないよう、その幅の分も空ける（ADR-138） */
  .source-section {
    @apply flex-1 overflow-y-auto pl-2 pr-5 pb-4;
    scrollbar-gutter: stable;
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

  /* 同期する内容（全曲か、選んだプレイリスト）の選択肢 */
  .source-choice {
    @apply flex flex-col gap-3;
  }

  .source-option {
    @apply items-start;
  }

  .remove-option {
    @apply mt-6;
  }

  /* 転送先のフォルダ（配置の書式の上。書式の項目と同じ見た目の名前と説明） */
  .destination {
    @apply mb-6;
  }

  .destination-label {
    @apply block text-sm font-medium text-text-primary mb-1;
  }

  .destination-hint {
    @apply text-xs text-text-muted m-0;
  }

  .destination-row {
    @apply flex items-center gap-3 mt-3;
  }

  .destination-path {
    @apply flex-1 min-w-0 truncate text-sm font-mono text-text-secondary;
  }

  .layout-title {
    @apply mt-8 mb-3;
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

  /* 選ぶ項目（プレイリストの形式・転送する時の変換。項目名と、選ぶ欄を並べる） */
  .select-option {
    @apply mt-6 flex-wrap cursor-default;
  }

  .option-select {
    @apply w-auto;
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

  /* 選ぶプレイリストは、「選んだプレイリスト」の選択肢の下に字下げして出す（`.playlist-list`の余白より後に置く） */
  .playlist-choice {
    @apply mt-2 ml-6;
  }
</style>
