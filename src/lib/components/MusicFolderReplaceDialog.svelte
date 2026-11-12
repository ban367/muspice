<!--
  @component MusicFolderReplaceDialog
  設定の「ライブラリ」で開く、音楽フォルダを変えた時のライブラリの置き換えの確認（ADR-086）。
  新しいフォルダはRust側が預かっている（WebViewからパスを渡さない）。外す曲・残る曲・消すプレイリストの
  数と、消える情報を見せ、置き換えるかを確かめる。「置き換える前にバックアップする」（既定はオン）を
  選べる。置き換えとやめる操作は、呼び出し側（`onConfirm`・`onCancel`）が行う。
-->
<script lang="ts">
  import type { MusicFolderReplacement } from '#lib/utils/musicFolder.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 置き換えの見通し（nullなら閉じている） */
    replacement: MusicFolderReplacement | null;
    /** 置き換え中か（新しいフォルダの読み込みを含む。閉じる・置き換える操作を止める） */
    isReplacing: boolean;
    /** 読み込みの進み具合の文（読み込み中でなければnull） */
    progressStatus: string | null;
    onCancel: () => void;
    onConfirm: (backup: boolean) => void;
  }

  let { replacement, isReplacing, progressStatus, onCancel, onConfirm }: Props = $props();

  /** 置き換える前にバックアップするか（開くたびにオンに戻す） */
  let backup = $state(true);

  $effect(() => {
    if (replacement) backup = true;
  });

  function close() {
    if (isReplacing) return;
    onCancel();
  }
</script>

<Modal
  open={replacement !== null}
  onClose={close}
  title={m.musicFolder.replaceTitle}
  dismissible={!isReplacing}
  class="max-w-lg"
>
  {#if replacement}
    <p class="text-sm mb-3 m-0 break-all">{m.musicFolder.replaceIntro(replacement.path)}</p>
    <ul class="replace-summary">
      <li class="text-warning">
        {m.musicFolder.replaceRemovedTracks(replacement.removedTrackCount)}
      </li>
      {#if replacement.keptTrackCount > 0}
        <li>{m.musicFolder.replaceKeptTracks(replacement.keptTrackCount)}</li>
      {/if}
      {#if replacement.playlistCount > 0}
        <li class="text-warning">{m.musicFolder.replacePlaylists(replacement.playlistCount)}</li>
      {/if}
    </ul>
    <p class="text-xs text-text-muted mb-4 m-0">{m.musicFolder.replaceLostInfo}</p>

    <label class="flex items-start gap-2 text-sm cursor-pointer">
      <input
        type="checkbox"
        class="w-4 h-4 shrink-0 mt-0.5 accent-primary cursor-pointer"
        bind:checked={backup}
        disabled={isReplacing}
      />
      <span>
        {m.musicFolder.replaceBackup}
        <span class="block text-xs text-text-muted">{m.musicFolder.replaceBackupHint}</span>
      </span>
    </label>

    {#if isReplacing}
      <p class="replace-status" role="status">{progressStatus ?? m.musicFolder.replacing}</p>
    {/if}
  {/if}

  {#snippet footer()}
    <button
      type="button"
      class="btn-secondary"
      onclick={close}
      disabled={isReplacing}
      data-autofocus
    >
      {m.common.cancel}
    </button>
    <button
      type="button"
      class="btn-danger"
      onclick={() => onConfirm(backup)}
      disabled={isReplacing}
    >
      {m.musicFolder.replaceConfirm}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .replace-summary {
    @apply text-sm mb-3 m-0 pl-5 list-disc flex flex-col gap-1;
  }

  .replace-status {
    @apply text-xs text-text-secondary mt-3 m-0 truncate;
  }
</style>
