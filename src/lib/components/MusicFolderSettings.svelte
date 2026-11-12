<!--
  @component MusicFolderSettings
  設定の「ライブラリ」の、音楽フォルダ（新しい曲を置く宛先。ライブラリフォルダの1つ）の表示・変更・解除。

  操作はすぐに反映する（設定の「適用」は不要）。フォルダは、既定のフォルダ（OSの音楽フォルダの中の
  `Muspice`。なければ作る）か、Rust側のダイアログ（設定ウィンドウの上に出す）で選ぶ。変えると、ライブラリを
  新しいフォルダの中身に置き換える（ADR-086）。外す曲がある時は、確認（`MusicFolderReplaceDialog`）を出す。
  新しく登録するフォルダの曲を読み込む間は、進み具合を出す。ファイルは動かさない。
  クエリの読み直し（`LibraryChanged`・ウィンドウに戻った時）は、`LibraryFolderSettings`が行う。
-->
<script lang="ts">
  import { events } from '#lib/bindings.js';
  import MusicFolderReplaceDialog from '#lib/components/MusicFolderReplaceDialog.svelte';
  import {
    cancelMusicFolderChange,
    useClearMusicFolderMutation,
    useConfirmMusicFolderChangeMutation,
    useMusicFolderQuery,
    useSetMusicFolderMutation
  } from '#lib/queries/musicFolder.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import type { MusicFolderChange, MusicFolderSource } from '#lib/types/models.js';
  import { musicFolderMessage, type MusicFolderReplacement } from '#lib/utils/musicFolder.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** ライブラリフォルダの再スキャン中か（その間は、変えられない） */
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  const musicFolderQuery = useMusicFolderQuery();
  const setMutation = useSetMusicFolderMutation();
  const confirmMutation = useConfirmMusicFolderChangeMutation();
  const clearMutation = useClearMusicFolderMutation();

  const info = $derived(musicFolderQuery.data);
  const folder = $derived(info?.folder ?? null);
  /** 既定のフォルダ（すでに音楽フォルダにしてあれば、出さない） */
  const defaultFolder = $derived(
    info?.defaultFolder && info.defaultFolder !== folder?.path ? info.defaultFolder : null
  );
  /** 確かめている置き換え（確認を出していなければnull） */
  let replacement = $state<MusicFolderReplacement | null>(null);
  const isBusy = $derived(
    disabled ||
      setMutation.isPending ||
      confirmMutation.isPending ||
      clearMutation.isPending ||
      replacement !== null
  );

  /** 新しく登録するフォルダの曲の読み込みの進み具合（届くまでは出さない。ダイアログを開いている間も） */
  let progress = $state<{ current: number; total: number; currentFile: string } | null>(null);
  const progressStatus = $derived(
    progress
      ? m.libraryFolders.progress(progress.current, progress.total, progress.currentFile)
      : null
  );

  /** 音楽フォルダの変更（読み込みを含む）の間、進み具合を受け取る */
  async function withProgress<T>(run: () => Promise<T>): Promise<T> {
    progress = null;
    const unlisten = await events.importProgress.listen((event) => {
      progress = event.payload;
    });
    try {
      return await run();
    } finally {
      unlisten();
      progress = null;
    }
  }

  /** 音楽フォルダにした結果を知らせる */
  function notifySet(change: MusicFolderChange) {
    if (change.kind !== 'set') return;
    showSuccess(musicFolderMessage(change));
    if (change.replaced?.backupPath) {
      showSuccess(m.musicFolder.replacedBackup(change.replaced.backupPath));
    }
    const errorCount = change.imported?.errorCount ?? 0;
    if (errorCount > 0) showWarning(m.libraryFolders.readErrors(errorCount));
  }

  async function choose(source: MusicFolderSource) {
    try {
      const change = await withProgress(() => setMutation.mutateAsync(source));
      if (!change) return;
      if (change.kind === 'confirmReplace') {
        replacement = change;
        return;
      }
      notifySet(change);
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  async function confirmReplace(backup: boolean) {
    try {
      const change = await withProgress(() => confirmMutation.mutateAsync(backup));
      notifySet(change);
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    } finally {
      replacement = null;
    }
  }

  function cancelReplace() {
    replacement = null;
    cancelMusicFolderChange();
  }

  async function clear() {
    try {
      await clearMutation.mutateAsync();
      showSuccess(m.musicFolder.cleared);
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }
</script>

<div class="music-folder">
  <p class="setting-label">{m.musicFolder.title}</p>
  <p class="setting-description">{m.musicFolder.hint}</p>

  <div class="music-folder-row">
    <div class="music-folder-info">
      {#if folder}
        <p class="folder-path" title={folder.path}>{folder.path}</p>
        {#if !folder.exists}
          <p class="folder-missing">{m.musicFolder.missing}</p>
        {/if}
      {:else if info}
        <p class="not-set">{m.musicFolder.notSet}</p>
      {/if}
    </div>
    <div class="music-folder-actions">
      {#if defaultFolder}
        <button
          type="button"
          class="btn-secondary text-xs"
          onclick={() => choose('default')}
          disabled={isBusy}
          title={defaultFolder}
        >
          {m.musicFolder.useDefault}
        </button>
      {/if}
      <button
        type="button"
        class="btn-secondary text-xs"
        onclick={() => choose('pick')}
        disabled={isBusy}
      >
        {m.musicFolder.pick}
      </button>
      {#if folder}
        <button type="button" class="btn-secondary text-xs" onclick={clear} disabled={isBusy}>
          {m.musicFolder.clear}
        </button>
      {/if}
    </div>
  </div>

  {#if progressStatus && replacement === null}
    <p class="scan-progress" role="status">{progressStatus}</p>
  {/if}
</div>

<MusicFolderReplaceDialog
  {replacement}
  isReplacing={confirmMutation.isPending}
  {progressStatus}
  onCancel={cancelReplace}
  onConfirm={confirmReplace}
/>

<style>
  @reference "../../app.css";

  .music-folder {
    @apply mb-6;
  }

  .setting-label {
    @apply block text-sm font-medium text-text-primary mb-1 m-0;
  }

  .setting-description {
    @apply text-xs text-text-muted m-0;
  }

  /* 場所は長いことが多いため、操作は場所の下に並べる */
  .music-folder-row {
    @apply flex flex-col gap-3 mt-3 p-3 border border-border rounded-md;
  }

  .music-folder-info {
    @apply min-w-0;
  }

  .folder-path {
    @apply text-sm text-text-primary truncate m-0 font-mono;
  }

  .folder-missing {
    @apply text-xs text-error-light mt-1 m-0;
  }

  .not-set {
    @apply text-sm text-text-muted m-0;
  }

  .music-folder-actions {
    @apply flex flex-wrap gap-2;
  }

  .scan-progress {
    @apply text-xs text-text-secondary mt-2 m-0 truncate;
  }
</style>
