<!--
  @component MusicFolderSettings
  設定の「ライブラリ」の、音楽フォルダ（新しい曲を置く宛先。ライブラリフォルダの1つ）の表示と変更。

  「既定のフォルダを使う」（OSの音楽フォルダの中の`Muspice`。なければ作る）と「フォルダを変更...」（Rust側の
  ダイアログを、設定ウィンドウの上に出す）は、選ぶだけで変えない（`MusicFolderDraft`）。設定の「適用」で
  変え、ライブラリを新しいフォルダの中身に置き換える（ADR-086。#408）。外す曲がある時は、確認
  （`MusicFolderReplaceDialog`）を出し、やめた時は、選んだフォルダのまま残す（もう一度「適用」できる）。
  新しく登録するフォルダの曲を読み込む間は、進み具合を出す。ファイルは動かさない。
  クエリの読み直し（`LibraryChanged`・ウィンドウに戻った時）は、`LibraryFolderSettings`が行う。
-->
<script lang="ts">
  import { events } from '#lib/bindings.js';
  import MusicFolderReplaceDialog from '#lib/components/MusicFolderReplaceDialog.svelte';
  import {
    cancelMusicFolderChange,
    useConfirmMusicFolderChangeMutation,
    useMusicFolderQuery,
    usePickMusicFolderMutation,
    useSetMusicFolderMutation
  } from '#lib/queries/musicFolder.js';
  import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
  import type { MusicFolderDraft } from '#lib/stores/musicFolderDraft.svelte.js';
  import type { MusicFolderChange } from '#lib/types/models.js';
  import { musicFolderMessage, type MusicFolderReplacement } from '#lib/utils/musicFolder.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 選んで、設定の「適用」を待っている音楽フォルダ（設定のページが持つ） */
    draft: MusicFolderDraft;
    /** ライブラリフォルダの再スキャン中か（その間は、変えられない） */
    disabled?: boolean;
  }

  let { draft, disabled = false }: Props = $props();

  const musicFolderQuery = useMusicFolderQuery();
  const pickMutation = usePickMusicFolderMutation();
  const setMutation = useSetMusicFolderMutation();
  const confirmMutation = useConfirmMusicFolderChangeMutation();

  const info = $derived(musicFolderQuery.data);
  const folder = $derived(info?.folder ?? null);
  const choice = $derived(draft.choice);
  /** 既定のフォルダ（すでに音楽フォルダにしてある・選んであれば、出さない） */
  const defaultFolder = $derived(
    info?.defaultFolder &&
      info.defaultFolder !== folder?.path &&
      info.defaultFolder !== choice?.path
      ? info.defaultFolder
      : null
  );
  /** 確かめている置き換え（確認を出していなければnull） */
  let replacement = $state<MusicFolderReplacement | null>(null);
  /** 置き換えの確認の答え（置き換えたらtrue、やめたらfalse）を、「適用」の処理へ返す */
  let answerReplacement: ((replaced: boolean) => void) | null = null;
  const isBusy = $derived(
    disabled ||
      pickMutation.isPending ||
      setMutation.isPending ||
      confirmMutation.isPending ||
      replacement !== null
  );

  /** 新しく登録するフォルダの曲の読み込みの進み具合（届くまでは出さない。ダイアログを開いている間も） */
  let progress = $state<{ current: number; total: number; currentFile: string } | null>(null);
  const progressStatus = $derived(
    progress
      ? m.libraryFolders.progress(progress.current, progress.total, progress.currentFile)
      : null
  );

  // 設定の「適用」で、選んだ音楽フォルダに変える
  $effect(() => draft.register(applyChoice));

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

  /** 既定のフォルダを選ぶ（「適用」で変える） */
  function chooseDefault() {
    if (defaultFolder) draft.choice = { source: 'default', path: defaultFolder };
  }

  /** フォルダをダイアログで選ぶ（「適用」で変える。今の音楽フォルダを選んだら、選んでいない状態に戻す） */
  async function pick() {
    try {
      const path = await pickMutation.mutateAsync();
      if (path === null) return;
      draft.choice = path === folder?.path ? null : { source: 'picked', path };
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  /**
   * 選んだ音楽フォルダに変える（設定の「適用」）
   *
   * 変えたら、選んだものを消す。失敗した時・置き換えの確認でやめた時は、選んだまま残す。
   */
  async function applyChoice() {
    const selected = draft.choice;
    if (selected === null) return;
    let change: MusicFolderChange | null;
    try {
      change = await withProgress(() => setMutation.mutateAsync(selected.source));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
      return;
    }
    if (change === null) return;
    if (change.kind === 'confirmReplace') {
      replacement = change;
      const replaced = await new Promise<boolean>((resolve) => (answerReplacement = resolve));
      if (replaced) draft.choice = null;
      return;
    }
    notifySet(change);
    draft.choice = null;
  }

  async function confirmReplace(backup: boolean) {
    let replaced = false;
    try {
      const change = await withProgress(() => confirmMutation.mutateAsync(backup));
      notifySet(change);
      replaced = true;
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    } finally {
      replacement = null;
      answerReplacement?.(replaced);
      answerReplacement = null;
    }
  }

  function cancelReplace() {
    replacement = null;
    cancelMusicFolderChange();
    answerReplacement?.(false);
    answerReplacement = null;
  }
</script>

<div class="music-folder">
  <p class="setting-label">{m.musicFolder.title}</p>
  <p class="setting-description">{m.musicFolder.hint}</p>

  <div class="music-folder-row">
    <div class="music-folder-info">
      {#if folder}
        <p class="folder-path" class:replaced={choice !== null} title={folder.path}>
          {folder.path}
        </p>
        {#if !folder.exists}
          <p class="folder-missing">{m.musicFolder.missing}</p>
        {/if}
      {:else if info}
        <p class="not-set">{m.musicFolder.notSet}</p>
      {/if}
      <!-- 選んで、「適用」を待っているフォルダ -->
      {#if choice}
        <div class="pending" role="status">
          <p class="pending-label">{m.musicFolder.pendingLabel}</p>
          <p class="folder-path" title={choice.path}>{choice.path}</p>
          <p class="setting-description">{m.musicFolder.pendingHint}</p>
        </div>
      {/if}
    </div>
    <div class="music-folder-actions">
      {#if defaultFolder}
        <button
          type="button"
          class="btn-secondary text-xs"
          onclick={chooseDefault}
          disabled={isBusy}
          title={defaultFolder}
        >
          {m.musicFolder.useDefault}
        </button>
      {/if}
      <button type="button" class="btn-secondary text-xs" onclick={pick} disabled={isBusy}>
        {folder ? m.musicFolder.change : m.musicFolder.pick}
      </button>
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

  /* 「適用」で置き換わる、今の音楽フォルダ */
  .folder-path.replaced {
    @apply line-through text-text-muted;
  }

  /* 選んで、「適用」を待っているフォルダ */
  .pending {
    @apply mt-3 pt-3 border-t border-border;
  }

  .pending-label {
    @apply text-xs font-medium text-primary mb-1 m-0;
  }

  .music-folder-actions {
    @apply flex flex-wrap gap-2;
  }

  .scan-progress {
    @apply text-xs text-text-secondary mt-2 m-0 truncate;
  }
</style>
