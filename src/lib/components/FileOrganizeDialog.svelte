<!--
  @component FileOrganizeDialog
  ファイルの整理。選んだ曲のファイルを、書式（`%albumartist%/%album%/%track% %title%`など）に従って、
  その曲のあるライブラリフォルダの中でリネーム・移動する。

  書式を変えるたびに、計画（今の場所 → 整理後の場所）をバックエンドから取り直して一覧にする。
  「整理する」を押すまで、ファイルは動かさない。動かせない曲（ライブラリフォルダの外・移動先に
  同じ名前のファイルがある・選んだ曲の中で同じ移動先になる）は、理由と一緒に一覧にして、動かさない。
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { OrganizeEntry, Track } from '#lib/types/models.js';
  import { fetchFileOrganizePlan, useOrganizeFilesMutation } from '#lib/queries/tracks.js';
  import {
    FILE_ORGANIZE_PRESETS,
    loadOrganizeTemplate,
    saveOrganizeTemplate,
    summarizeOrganizePlan
  } from '#lib/utils/fileOrganize.js';
  import { createDebounce } from '#lib/utils/debounce.js';
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

  let template = $state(loadOrganizeTemplate());

  // 計画（書式を変えるたびに取り直す。取り直している間は、前の計画を出したままにする）
  let entries = $state.raw<OrganizeEntry[] | null>(null);
  let planError = $state<string | null>(null);
  let isPlanning = $state(true);
  /** 取り直しの番号（後から始めた取得の結果だけを使う） */
  let planRequest = 0;

  async function loadPlan(nextTemplate: string) {
    const request = ++planRequest;
    try {
      const plan = await fetchFileOrganizePlan(trackIds, nextTemplate);
      if (request !== planRequest) return;
      entries = plan.entries;
      planError = null;
    } catch (e) {
      if (request !== planRequest) return;
      entries = null;
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

  // 最初の計画はすぐに、書式を変えた後の計画は入力が止まってから取る
  let isFirstPlan = true;
  $effect(() => {
    const nextTemplate = template;
    isPlanning = true;
    if (isFirstPlan) {
      isFirstPlan = false;
      void loadPlan(nextTemplate);
    } else {
      planDebounce.debounced(nextTemplate);
    }
  });

  const summary = $derived(summarizeOrganizePlan(entries ?? []));
  const previewMoves = $derived(summary.moves.slice(0, PREVIEW_LIMIT));
  const previewBlocked = $derived(summary.blocked.slice(0, PREVIEW_LIMIT));

  const organizeMutation = useOrganizeFilesMutation();
  const isApplying = $derived(organizeMutation.isPending);
  let applyError = $state<string | null>(null);
  const canApply = $derived(!isApplying && !isPlanning && summary.moves.length > 0);

  async function handleApply() {
    applyError = null;
    try {
      const result = await organizeMutation.mutateAsync({ trackIds, template });
      saveOrganizeTemplate(template);
      if (result.movedCount === 0) {
        // 1曲も移せなかった場合は、画面を閉じずに理由を表示し、計画を取り直す
        applyError = m.fileOrganize.allFailed(result.errors[0] ?? '');
        isPlanning = true;
        void loadPlan(template);
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
    }
  }
</script>

<Modal
  open
  {onClose}
  title={m.fileOrganize.title(trackIds.length)}
  dismissible={!isApplying}
  class="max-w-4xl"
>
  <form class="options" onsubmit={(e) => e.preventDefault()}>
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
  </form>

  {#if planError}
    <div class="message-error mt-3" role="alert">{planError}</div>
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
    <button class="btn-secondary" onclick={onClose} disabled={isApplying}>{m.common.cancel}</button>
    <button class="btn-primary" onclick={handleApply} disabled={!canApply}>
      {isApplying ? m.fileOrganize.applying : m.fileOrganize.apply}
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
</style>
