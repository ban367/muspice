<!--
  @component LibraryFolderRelocationDialog
  設定の「ライブラリ」で開く、ライブラリフォルダの場所の変更（付け替え）の確認。
  新しい場所はRust側のダイアログで選び、Rust側が預かっている（WebViewからパスを渡さない）。
  新しい場所でファイルが見つかった曲の数を見せ、付け替えるかを確かめる。付け替え（続けて行う
  再スキャンを含む）とやめる操作は、呼び出し側（`onConfirm`・`onCancel`）が行う。
-->
<script lang="ts">
  import type { LibraryFolder } from '#lib/types/models.js';
  import type { ReadyRelocation } from '#lib/utils/relocation.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 付け替えるフォルダと、選んだ新しい場所（nullなら閉じている） */
    target: { folder: LibraryFolder; relocation: ReadyRelocation } | null;
    /** 付け替え中か（再スキャンを含む。閉じる・付け替える操作を止める） */
    isRelocating: boolean;
    /** 再スキャンの進み具合の文（再スキャン中でなければnull） */
    scanStatus: string | null;
    onCancel: () => void;
    onConfirm: () => void;
  }

  let { target, isRelocating, scanStatus, onCancel, onConfirm }: Props = $props();

  const notFoundCount = $derived(
    target ? target.relocation.trackCount - target.relocation.foundCount : 0
  );

  function close() {
    if (isRelocating) return;
    onCancel();
  }
</script>

<Modal
  open={target !== null}
  onClose={close}
  title={m.libraryFolders.relocateTitle}
  dismissible={!isRelocating}
  class="max-w-lg"
>
  {#if target}
    <dl class="relocation-paths">
      <dt>{m.libraryFolders.relocateFrom}</dt>
      <dd>{target.folder.path}</dd>
      <dt>{m.libraryFolders.relocateTo}</dt>
      <dd>{target.relocation.path}</dd>
    </dl>

    {#if target.relocation.trackCount > 0}
      {#if target.relocation.foundCount === 0}
        <p class="text-sm text-warning mb-2 m-0">{m.libraryFolders.relocateNoneFound}</p>
      {:else}
        <p class="text-sm mb-2 m-0">
          {m.libraryFolders.relocateFound(
            target.relocation.foundCount,
            target.relocation.trackCount
          )}
        </p>
      {/if}
      {#if notFoundCount > 0}
        <p class="text-xs text-text-muted mb-2 m-0">
          {m.libraryFolders.relocateNotFound(notFoundCount)}
        </p>
      {/if}
    {/if}
    <p class="text-xs text-text-muted m-0">{m.libraryFolders.relocateHint}</p>

    {#if isRelocating}
      <p class="relocation-status" role="status">{scanStatus ?? m.libraryFolders.relocating}</p>
    {/if}
  {/if}

  {#snippet footer()}
    <button
      type="button"
      class="btn-secondary"
      onclick={close}
      disabled={isRelocating}
      data-autofocus
    >
      {m.common.cancel}
    </button>
    <button type="button" class="btn-primary" onclick={onConfirm} disabled={isRelocating}>
      {m.libraryFolders.relocateConfirm}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .relocation-paths {
    @apply grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm mb-4 m-0;
  }

  .relocation-paths dt {
    @apply text-text-muted;
  }

  .relocation-paths dd {
    @apply m-0 font-mono break-all text-text-primary;
  }

  .relocation-status {
    @apply text-xs text-text-secondary mt-3 m-0 truncate;
  }
</style>
