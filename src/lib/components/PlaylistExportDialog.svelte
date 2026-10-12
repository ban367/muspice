<!--
  @component PlaylistExportDialog
  プレイリストをM3U8へ書き出す前に、曲の場所の書き方（絶対パス・相対パス）を選ぶダイアログ。
  「保存先を選ぶ」を押すと、Rust側が保存先のダイアログを開いて書き出す。
-->
<script lang="ts">
  import type { Playlist } from '#lib/types/models.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { useExportPlaylistM3uMutation } from '#lib/queries/playlists.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 書き出すプレイリスト（nullなら表示しない） */
    playlist: Playlist | null;
    onClose: () => void;
  }

  let { playlist, onClose }: Props = $props();

  const exportMutation = useExportPlaylistM3uMutation();

  // 前回の選択を初期値にする
  let pathStyle = $state<'absolute' | 'relative'>(
    ui.playlistExportRelative ? 'relative' : 'absolute'
  );

  function startExport() {
    if (!playlist) return;
    const relativePaths = pathStyle === 'relative';
    ui.playlistExportRelative = relativePaths;
    // 保存先のダイアログは、このダイアログを閉じてから開く（結果は通知で知らせる）
    exportMutation.mutate({ playlistId: playlist.id, relativePaths });
    onClose();
  }
</script>

<Modal open={playlist !== null} {onClose} title={m.playlists.exportTitle} class="max-w-lg">
  {#if playlist}
    <p class="mt-0 mb-4 text-text-secondary">{m.playlists.exportDescription(playlist.name)}</p>

    <fieldset class="m-0 p-0 border-none">
      <legend class="mb-2 text-sm font-semibold text-text-primary">
        {m.playlists.exportPathStyle}
      </legend>
      <div class="flex flex-col gap-3">
        <label class="path-option">
          <input type="radio" bind:group={pathStyle} value="absolute" class="w-4 h-4 mt-1" />
          <span>
            <span class="block text-text-primary">{m.playlists.exportAbsolute}</span>
            <span class="block text-sm text-text-muted">{m.playlists.exportAbsoluteHint}</span>
          </span>
        </label>
        <label class="path-option">
          <input type="radio" bind:group={pathStyle} value="relative" class="w-4 h-4 mt-1" />
          <span>
            <span class="block text-text-primary">{m.playlists.exportRelative}</span>
            <span class="block text-sm text-text-muted">{m.playlists.exportRelativeHint}</span>
          </span>
        </label>
      </div>
    </fieldset>
  {/if}

  {#snippet footer()}
    <button onclick={onClose} class="btn-secondary">{m.common.cancel}</button>
    <button onclick={startExport} class="btn-primary" data-autofocus>
      {m.playlists.exportConfirm}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .path-option {
    @apply flex items-start gap-3 cursor-pointer;
  }
</style>
