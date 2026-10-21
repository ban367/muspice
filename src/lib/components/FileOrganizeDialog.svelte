<!--
  @component FileOrganizeDialog
  ファイルの整理。選んだ曲のファイルを、書式（`%albumartist%/%album%/%track% %title%`など）に従って、
  その曲のあるライブラリフォルダの中でリネーム・移動するか、音楽フォルダへ集める（ADR-070）。

  書式・移動先を変えるたびに、計画（今の場所 → 整理後の場所）をバックエンドから取り直して一覧にする。
  「整理する」を押すまで、ファイルは動かさない。動かせない曲（ライブラリフォルダの外・移動先に
  同じ名前のファイルがある・選んだ曲の中で同じ移動先になる）は、理由と一緒に一覧にして、動かさない。
  音楽フォルダへ集める時は、設定の音楽フォルダの書式を使い、別のドライブの曲はコピーしてから元を
  ゴミ箱へ移す。整理している間は、進み具合を出し、中止できる。
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { events } from '#lib/bindings.js';
  import type {
    OrganizeDestination,
    OrganizeEntry,
    OrganizePlan,
    OrganizeProgress,
    Track
  } from '#lib/types/models.js';
  import {
    cancelFileOrganize,
    fetchFileOrganizePlan,
    useCanMoveToTrashQuery,
    useOrganizeFilesMutation
  } from '#lib/queries/tracks.js';
  import { useMusicFolderQuery } from '#lib/queries/musicFolder.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import {
    FILE_ORGANIZE_PRESETS,
    loadOrganizeTemplate,
    saveOrganizeTemplate,
    summarizeOrganizePlan
  } from '#lib/utils/fileOrganize.js';
  import { createDebounce } from '#lib/utils/debounce.js';
  import { formatFileSize } from '#lib/utils/format.js';
  import { showSuccess, showWarning, toErrorMessage } from '#lib/stores/error.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 整理する曲（一覧の順） */
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const trackIds = tracks.map((track) => track.id);

  /** 一覧に出す曲数の上限（多いと描画が重い。整理は、すべての曲に行う） */
  const PREVIEW_LIMIT = 200;

  const musicFolderQuery = useMusicFolderQuery();
  const settingsQuery = useSettingsQuery();
  const canMoveToTrashQuery = useCanMoveToTrashQuery();
  /** 音楽フォルダ（設定していなければnull。集める移動先は、設定している時だけ出す） */
  const musicFolder = $derived(musicFolderQuery.data?.folder ?? null);

  let template = $state(loadOrganizeTemplate());
  let destination = $state<OrganizeDestination>('libraryFolder');
  const isCollecting = $derived(destination === 'musicFolder');

  // 計画（書式・移動先を変えるたびに取り直す。取り直している間は、前の計画を出したままにする）
  let plan = $state.raw<OrganizePlan | null>(null);
  const entries = $derived<OrganizeEntry[] | null>(plan?.entries ?? null);
  let planError = $state<string | null>(null);
  let isPlanning = $state(true);
  /** 取り直しの番号（後から始めた取得の結果だけを使う） */
  let planRequest = 0;

  async function loadPlan(nextTemplate: string, nextDestination: OrganizeDestination) {
    const request = ++planRequest;
    try {
      const nextPlan = await fetchFileOrganizePlan(trackIds, nextTemplate, nextDestination);
      if (request !== planRequest) return;
      plan = nextPlan;
      planError = null;
    } catch (e) {
      if (request !== planRequest) return;
      plan = null;
      planError = toErrorMessage(e);
    } finally {
      if (request === planRequest) isPlanning = false;
    }
  }

  const planDebounce = createDebounce(loadPlan, 300);
  onDestroy(() => {
    planDebounce.cancel();
    // 閉じた後に届いた結果は使わない
    planRequest++;
  });

  // 最初の計画と、移動先を変えた後の計画はすぐに、書式を変えた後の計画は入力が止まってから取る
  // （音楽フォルダへ集める時は、この画面の書式を使わないため、書式を変えても取り直さない）
  let lastDestination: OrganizeDestination | null = null;
  $effect(() => {
    const nextTemplate = template;
    const nextDestination = destination;
    if (lastDestination !== nextDestination) {
      lastDestination = nextDestination;
      isPlanning = true;
      planDebounce.cancel();
      void loadPlan(nextTemplate, nextDestination);
    } else if (nextDestination === 'libraryFolder') {
      isPlanning = true;
      planDebounce.debounced(nextTemplate, nextDestination);
    }
  });

  const summary = $derived(summarizeOrganizePlan(entries ?? []));
  const previewMoves = $derived(summary.moves.slice(0, PREVIEW_LIMIT));
  const previewBlocked = $derived(summary.blocked.slice(0, PREVIEW_LIMIT));

  /** 音楽フォルダへ集められない理由（集められる、または集めない時はnull） */
  const collectBlocker = $derived.by(() => {
    if (!isCollecting) return null;
    if (musicFolder && !musicFolder.exists) {
      return m.fileOrganize.musicFolderMissing(musicFolder.path);
    }
    if (plan && plan.freeBytes !== null && plan.copyBytes > plan.freeBytes) {
      return m.fileOrganize.notEnoughSpace(
        formatFileSize(plan.copyBytes),
        formatFileSize(plan.freeBytes)
      );
    }
    return null;
  });

  const organizeMutation = useOrganizeFilesMutation();
  const isApplying = $derived(organizeMutation.isPending);
  let applyError = $state<string | null>(null);
  /** 整理の進み具合（届くまではnull） */
  let progress = $state.raw<OrganizeProgress | null>(null);
  let isCancelling = $state(false);
  const canApply = $derived(
    !isApplying && !isPlanning && summary.moves.length > 0 && collectBlocker === null
  );
  const percent = $derived(
    progress && progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0
  );

  async function handleApply() {
    applyError = null;
    progress = null;
    isCancelling = false;
    const unlisten = await events.organizeProgress.listen((event) => {
      progress = event.payload;
    });
    try {
      const result = await organizeMutation.mutateAsync({ trackIds, template, destination });
      if (!isCollecting) saveOrganizeTemplate(template);
      if (result.cancelled) {
        showWarning(m.fileOrganize.cancelled(result.movedCount));
        onClose();
        return;
      }
      if (result.movedCount === 0) {
        // 1曲も移せなかった場合は、画面を閉じずに理由を表示し、計画を取り直す
        applyError = m.fileOrganize.allFailed(result.errors[0] ?? '');
        isPlanning = true;
        void loadPlan(template, destination);
        return;
      }
      if (result.failedCount > 0) {
        showWarning(m.fileOrganize.partiallyFailed(result.movedCount, result.failedCount));
      } else {
        showSuccess(m.fileOrganize.moved(result.movedCount));
      }
      onClose();
    } catch (e) {
      applyError = toErrorMessage(e);
    } finally {
      unlisten();
      progress = null;
    }
  }

  /** 整理を中止する（移している曲を移し終えてから止まる） */
  async function cancel() {
    isCancelling = true;
    try {
      await cancelFileOrganize();
    } catch (e) {
      isCancelling = false;
      applyError = toErrorMessage(e);
    }
  }
</script>

<Modal
  open
  {onClose}
  title={m.fileOrganize.title(trackIds.length)}
  dismissible={!isApplying}
  class="max-w-4xl"
  bodyClass="p-6 overflow-y-auto flex-1 [overflow-anchor:none]"
>
  <!-- 移動先は、音楽フォルダを読んでから上に出るため、本文のスクロールの位置を合わせ直させない
       （書式の入力欄へのフォーカスで、本文が下へずれないように） -->
  <form class="options" onsubmit={(e) => e.preventDefault()}>
    {#if musicFolder}
      <!-- 移動先（音楽フォルダを設定している時だけ選べる） -->
      <fieldset class="destination">
        <legend class="form-label">{m.fileOrganize.destination}</legend>
        <label class="destination-option">
          <input
            type="radio"
            bind:group={destination}
            value="libraryFolder"
            disabled={isApplying}
            class="w-4 h-4"
          />
          {m.fileOrganize.destinationLibrary}
        </label>
        <label class="destination-option">
          <input
            type="radio"
            bind:group={destination}
            value="musicFolder"
            disabled={isApplying}
            class="w-4 h-4"
          />
          {m.fileOrganize.destinationMusicFolder}
          <span class="destination-path" title={musicFolder.path}>{musicFolder.path}</span>
        </label>
      </fieldset>
    {/if}

    {#if isCollecting}
      <!-- 音楽フォルダへ集める時は、設定の音楽フォルダの書式を使う（この画面では変えない） -->
      <div class="form-group">
        <span class="form-label">{m.fileOrganize.musicFolderTemplate}</span>
        <p class="music-template">{settingsQuery.data?.musicFolderTemplate ?? ''}</p>
      </div>
      <p class="hint">{m.fileOrganize.musicFolderTemplateHint}</p>
      {#if musicFolder}
        <p class="hint mt-1">{m.fileOrganize.collectHint(musicFolder.path)}</p>
      {/if}
    {:else}
      <div class="form-row">
        <div class="form-group grow">
          <label for="file-organize-template" class="form-label">{m.fileOrganize.template}</label>
          <input
            id="file-organize-template"
            type="text"
            class="form-input font-mono"
            bind:value={template}
            autocomplete="off"
            spellcheck="false"
            disabled={isApplying}
            data-autofocus
          />
        </div>
        <div class="form-group">
          <label for="file-organize-preset" class="form-label">{m.fileOrganize.preset}</label>
          <select
            id="file-organize-preset"
            class="form-input"
            value={(FILE_ORGANIZE_PRESETS as readonly string[]).includes(template) ? template : ''}
            onchange={(e) => {
              if (e.currentTarget.value) template = e.currentTarget.value;
            }}
            disabled={isApplying}
          >
            <option value="" hidden></option>
            {#each FILE_ORGANIZE_PRESETS as preset (preset)}
              <option value={preset}>{preset}</option>
            {/each}
          </select>
        </div>
      </div>
      <p class="hint">{m.fileOrganize.templateHint}</p>
      <p class="hint mt-1">{m.fileOrganize.scopeHint}</p>
    {/if}
  </form>

  {#if planError}
    <div class="message-error mt-3" role="alert">{planError}</div>
  {/if}

  {#if isCollecting && plan && plan.copyCount > 0}
    <!-- 別のドライブの曲は、コピーしてから元をゴミ箱へ移す -->
    <p class="hint mt-3">
      {m.fileOrganize.copyAcrossDrives(
        plan.copyCount,
        formatFileSize(plan.copyBytes),
        canMoveToTrashQuery.data !== false
      )}
      {#if plan.freeBytes !== null}
        {m.fileOrganize.freeSpace(formatFileSize(plan.freeBytes))}
      {/if}
    </p>
  {/if}
  {#if collectBlocker}
    <div class="message-error mt-3" role="alert">{collectBlocker}</div>
  {/if}

  <!-- 進み具合（一覧より上に出し、整理している間も見えるようにする） -->
  {#if isApplying}
    <div class="progress" role="status">
      <div
        class="bar"
        role="progressbar"
        aria-label={m.fileOrganize.applying}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
      >
        <div class="bar-fill" style="width: {percent}%"></div>
      </div>
      <p class="hint mt-2">
        {#if isCancelling}
          {m.fileOrganize.cancelling}
        {:else if progress}
          {m.fileOrganize.progress(progress.current, progress.total)}
          <span class="text-text-dimmed ml-2">{progress.currentFile}</span>
        {:else}
          {m.fileOrganize.applying}
        {/if}
      </p>
    </div>
  {/if}

  <section class="preview" aria-live="polite" aria-busy={isPlanning}>
    <div class="preview-header">
      <h3 class="m-0 text-sm font-semibold text-text-secondary">{m.fileOrganize.moves}</h3>
      <span class="text-sm text-text-muted">
        {#if entries === null}
          {planError ? '' : m.common.loading}
        {:else if summary.moves.length > 0}
          {m.fileOrganize.moveCount(summary.moves.length)}
        {:else}
          {m.fileOrganize.noMoves}
        {/if}
      </span>
    </div>

    {#if summary.moves.length > 0}
      <div class="preview-table" role="table" aria-label={m.fileOrganize.moves}>
        <div class="preview-row head" role="row">
          <span role="columnheader">{m.fileOrganize.columnCurrent}</span>
          <span role="columnheader">{m.fileOrganize.columnNew}</span>
        </div>
        {#each previewMoves as entry (entry.trackId)}
          <div class="preview-row" role="row">
            <span role="cell" class="before">{entry.currentPath}</span>
            <span role="cell" class="after">{entry.newPath}</span>
          </div>
        {/each}
      </div>
      {#if summary.moves.length > previewMoves.length}
        <p class="hint mt-2">
          {m.fileOrganize.more(summary.moves.length - previewMoves.length)}
        </p>
      {/if}
    {/if}

    {#if summary.unchangedCount > 0}
      <p class="hint mt-2">{m.fileOrganize.unchanged(summary.unchangedCount)}</p>
    {/if}
  </section>

  {#if summary.blocked.length > 0}
    <section class="preview">
      <div class="preview-header">
        <h3 class="m-0 text-sm font-semibold text-text-secondary">{m.fileOrganize.blocked}</h3>
        <span class="text-sm text-text-muted">
          {m.fileOrganize.blockedCount(summary.blocked.length)}
        </span>
      </div>
      <div class="preview-table blocked" role="table" aria-label={m.fileOrganize.blocked}>
        <div class="preview-row head" role="row">
          <span role="columnheader">{m.fileOrganize.columnCurrent}</span>
          <span role="columnheader">{m.fileOrganize.columnReason}</span>
        </div>
        {#each previewBlocked as entry (entry.trackId)}
          <div class="preview-row" role="row">
            <span role="cell" class="before">{entry.currentPath}</span>
            <span role="cell" class="reason">
              {m.fileOrganize.reasons[entry.status]}
              {#if entry.newPath}
                <span class="target">{entry.newPath}</span>
              {/if}
            </span>
          </div>
        {/each}
      </div>
      {#if summary.blocked.length > previewBlocked.length}
        <p class="hint mt-2">
          {m.fileOrganize.more(summary.blocked.length - previewBlocked.length)}
        </p>
      {/if}
    </section>
  {/if}

  {#if applyError}
    <div class="message-error mt-3">{applyError}</div>
  {/if}

  {#snippet footer()}
    {#if isApplying}
      <button class="btn-secondary" onclick={cancel} disabled={isCancelling}>
        {m.fileOrganize.cancel}
      </button>
    {:else}
      <button class="btn-secondary" onclick={onClose}>{m.common.cancel}</button>
    {/if}
    <button class="btn-primary" onclick={handleApply} disabled={!canApply}>
      {#if isApplying}
        {m.fileOrganize.applying}
      {:else}
        {isCollecting ? m.fileOrganize.applyCollect : m.fileOrganize.apply}
      {/if}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .form-row {
    @apply flex flex-wrap gap-x-4 gap-y-2;
  }

  .grow {
    @apply flex-1 min-w-48;
  }

  .hint {
    @apply mb-0 text-xs text-text-muted;
  }

  .destination {
    @apply flex flex-col gap-2 mb-4 p-0 border-none;
  }

  .destination-option {
    @apply flex items-center gap-2 min-w-0 text-sm text-text-secondary cursor-pointer;
  }

  .destination-path {
    @apply min-w-0 truncate text-xs text-text-muted font-mono;
  }

  /* 音楽フォルダの書式（この画面では変えない） */
  .music-template {
    @apply m-0 py-2 px-3 text-sm font-mono text-text-primary bg-base-400 border border-border rounded-md;
  }

  .preview {
    @apply mt-4;
  }

  .preview-header {
    @apply flex items-baseline justify-between gap-3 mb-2;
  }

  /* 曲の一覧（今の場所 → 整理後の場所）。多い場合は、この中でスクロールする */
  .preview-table {
    @apply border border-border rounded overflow-y-auto text-sm;
    max-height: 16rem;
  }

  .preview-table.blocked {
    max-height: 10rem;
  }

  .preview-row {
    @apply grid gap-3 px-3 py-1 items-baseline;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .preview-row.head {
    @apply sticky top-0 bg-base-200 text-xs font-semibold text-text-muted uppercase border-b border-border;
  }

  .preview-row span[role='cell'] {
    @apply min-w-0 break-words;
  }

  .before {
    @apply text-text-muted;
  }

  .after {
    @apply text-text-primary font-medium;
  }

  .reason {
    @apply text-warning;
  }

  /* 動かせなかった曲の、整理後の場所（理由の下に、小さく出す） */
  .target {
    @apply block text-xs text-text-muted;
  }

  .progress {
    @apply mt-4;
  }

  .bar {
    @apply h-1.5 rounded-full bg-surface-active overflow-hidden;
  }

  .bar-fill {
    @apply h-full bg-primary transition-all duration-200;
  }
</style>
