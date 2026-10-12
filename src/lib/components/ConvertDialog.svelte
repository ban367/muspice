<!--
  @component ConvertDialog
  形式の変換。選んだ曲を、別の形式（AAC・ALAC・FLAC・WAV・AIFF）に変換する。

  - 書き出す: 選んだフォルダへ、書式に従った場所に書き出す（ライブラリの曲は変えない。
    書き出した曲をライブラリに加えるかは選べる）
  - 置き換える: 元のファイルを、変換したファイルに入れ替える（曲はそのまま残り、お気に入り・
    再生回数・プレイリストも変わらない）。元のファイルは、ゴミ箱へ移す

  指定を変えるたびに、計画（曲ごとの出力先）をバックエンドから取り直して一覧にする。
  「変換する」を押すまで、ファイルは作らない。変換はバックグラウンドで進み、閉じても続く。
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { AAC_BITRATES_KBPS } from '#lib/bindings.js';
  import type {
    ConvertEntry,
    ConvertFormat,
    ConvertMode,
    ConvertOptions,
    Track
  } from '#lib/types/models.js';
  import { fetchConversionPlan } from '#lib/queries/convert.js';
  import { conversion } from '#lib/stores/convert.svelte.js';
  import {
    loadConvertOptions,
    saveConvertOptions,
    summarizeConvertPlan
  } from '#lib/utils/convert.js';
  import { FILE_ORGANIZE_PRESETS } from '#lib/utils/fileOrganize.js';
  import { createDebounce } from '#lib/utils/debounce.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 変換する曲（一覧の順） */
    tracks: Track[];
    /** この環境で変換先にできる形式 */
    formats: ConvertFormat[];
    onClose: () => void;
  }

  let { tracks, formats, onClose }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const trackIds = tracks.map((track) => track.id);

  /** 一覧に出す曲数の上限（多いと描画が重い。変換は、すべての曲に行う） */
  const PREVIEW_LIMIT = 200;
  const modes: ConvertMode[] = ['export', 'replace'];

  // svelte-ignore state_referenced_locally
  const initial = loadConvertOptions(formats);
  let format = $state(initial.format);
  let aacBitrateKbps = $state(initial.aacBitrateKbps);
  let mode = $state(initial.mode);
  let template = $state(initial.template);
  let addToLibrary = $state(initial.addToLibrary);
  const options = $derived<ConvertOptions>({
    format,
    aacBitrateKbps,
    mode,
    template,
    addToLibrary
  });

  // 計画（指定を変えるたびに取り直す。取り直している間は、前の計画を出したままにする）
  let entries = $state.raw<ConvertEntry[] | null>(null);
  let planError = $state<string | null>(null);
  let isPlanning = $state(true);
  /** 取り直しの番号（後から始めた取得の結果だけを使う） */
  let planRequest = 0;

  async function loadPlan(next: ConvertOptions) {
    const request = ++planRequest;
    try {
      const plan = await fetchConversionPlan(trackIds, next);
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

  // 最初の計画はすぐに、指定を変えた後の計画は入力が止まってから取る
  // （ビットレート・ライブラリに加えるかは、出力先に関わらないため、取り直さない）
  let isFirstPlan = true;
  $effect(() => {
    const next: ConvertOptions = {
      format,
      mode,
      template,
      aacBitrateKbps: AAC_BITRATES_KBPS[0],
      addToLibrary: false
    };
    isPlanning = true;
    if (isFirstPlan) {
      isFirstPlan = false;
      void loadPlan(next);
    } else {
      planDebounce.debounced(next);
    }
  });

  const summary = $derived(summarizeConvertPlan(entries ?? [], format, mode));
  const previewConversions = $derived(summary.conversions.slice(0, PREVIEW_LIMIT));
  const previewBlocked = $derived(summary.blocked.slice(0, PREVIEW_LIMIT));
  const warnings = $derived(summary.warnings);

  /** この画面から始めた変換の、変換できなかった理由（あれば、閉じずに表示する） */
  let errors = $state.raw<string[]>([]);
  const isRunning = $derived(conversion.isRunning);
  const canStart = $derived(!isRunning && !isPlanning && summary.conversions.length > 0);

  async function handleStart() {
    errors = [];
    saveConvertOptions(options);
    const result = await conversion.start(trackIds, options);
    // 書き出すフォルダを選ばなかった場合は、画面をそのままにする
    if (result === null) return;
    if (result.errors.length > 0) {
      errors = result.errors;
      isPlanning = true;
      void loadPlan(options);
    } else {
      onClose();
    }
  }

  const percent = $derived(
    conversion.progress && conversion.progress.total > 0
      ? Math.round((conversion.progress.current / conversion.progress.total) * 100)
      : 0
  );
</script>

<Modal open {onClose} title={m.convert.title(trackIds.length)} class="max-w-3xl">
  <form class="options" onsubmit={(e) => e.preventDefault()}>
    <div class="form-row">
      <div class="form-group">
        <label for="convert-format" class="form-label">{m.convert.format}</label>
        <select id="convert-format" class="form-input" bind:value={format} disabled={isRunning}>
          {#each formats as candidate (candidate)}
            <option value={candidate}>{m.convert.formats[candidate]}</option>
          {/each}
        </select>
      </div>
      {#if format === 'aac'}
        <div class="form-group">
          <label for="convert-bitrate" class="form-label">{m.convert.bitrate}</label>
          <select
            id="convert-bitrate"
            class="form-input"
            bind:value={aacBitrateKbps}
            disabled={isRunning}
          >
            {#each AAC_BITRATES_KBPS as bitrate (bitrate)}
              <option value={bitrate}>{m.convert.bitrateValue(bitrate)}</option>
            {/each}
          </select>
        </div>
      {/if}
    </div>

    <fieldset class="modes">
      <legend class="form-label">{m.convert.destination}</legend>
      {#each modes as candidate (candidate)}
        <label class="check">
          <input
            type="radio"
            name="convert-mode"
            class="w-4 h-4"
            value={candidate}
            bind:group={mode}
            disabled={isRunning}
          />
          {m.convert.modes[candidate]}
        </label>
      {/each}
    </fieldset>

    {#if mode === 'export'}
      <div class="form-row">
        <div class="form-group grow">
          <label for="convert-template" class="form-label">{m.convert.template}</label>
          <input
            id="convert-template"
            type="text"
            class="form-input font-mono"
            bind:value={template}
            autocomplete="off"
            spellcheck="false"
            disabled={isRunning}
          />
        </div>
        <div class="form-group">
          <label for="convert-preset" class="form-label">{m.fileOrganize.preset}</label>
          <select
            id="convert-preset"
            class="form-input"
            value={(FILE_ORGANIZE_PRESETS as readonly string[]).includes(template) ? template : ''}
            onchange={(e) => {
              if (e.currentTarget.value) template = e.currentTarget.value;
            }}
            disabled={isRunning}
          >
            <option value="" hidden></option>
            {#each FILE_ORGANIZE_PRESETS as preset (preset)}
              <option value={preset}>{preset}</option>
            {/each}
          </select>
        </div>
      </div>
      <label class="check">
        <input type="checkbox" class="w-4 h-4" bind:checked={addToLibrary} disabled={isRunning} />
        {m.convert.addToLibrary}
      </label>
      <p class="hint mt-2">{m.convert.exportHint}</p>
    {:else}
      <p class="hint">{m.convert.replaceHint}</p>
    {/if}
  </form>

  {#if planError}
    <div class="message-error mt-3" role="alert">{planError}</div>
  {/if}

  {#if warnings.lossyToLossless > 0}
    <p class="warning" role="status">
      {m.convert.warnings.lossyToLossless(warnings.lossyToLossless)}
    </p>
  {/if}
  {#if warnings.lossyToLossy > 0}
    <p class="warning" role="status">{m.convert.warnings.lossyToLossy(warnings.lossyToLossy)}</p>
  {/if}
  {#if warnings.losslessReplaced > 0}
    <p class="warning" role="status">
      {m.convert.warnings.losslessReplaced(warnings.losslessReplaced)}
    </p>
  {/if}

  <section class="preview" aria-live="polite" aria-busy={isPlanning}>
    <div class="preview-header">
      <h3 class="m-0 text-sm font-semibold text-text-secondary">{m.convert.conversions}</h3>
      <span class="text-sm text-text-muted">
        {#if entries === null}
          {planError ? '' : m.common.loading}
        {:else if summary.conversions.length > 0}
          {m.convert.conversionCount(summary.conversions.length)}
        {:else}
          {m.convert.noConversions}
        {/if}
      </span>
    </div>

    {#if summary.conversions.length > 0}
      <div class="preview-table" role="table" aria-label={m.convert.conversions}>
        <div class="preview-row head" role="row">
          <span role="columnheader">{m.convert.columnSource}</span>
          <span role="columnheader">{m.convert.columnOutput}</span>
        </div>
        {#each previewConversions as entry (entry.trackId)}
          <div class="preview-row" role="row">
            <span role="cell" class="before">{entry.sourceName}</span>
            <span role="cell" class="after">{entry.outputPath}</span>
          </div>
        {/each}
      </div>
      {#if summary.conversions.length > previewConversions.length}
        <p class="hint mt-2">
          {m.fileOrganize.more(summary.conversions.length - previewConversions.length)}
        </p>
      {/if}
    {/if}
  </section>

  {#if summary.blocked.length > 0}
    <section class="preview">
      <div class="preview-header">
        <h3 class="m-0 text-sm font-semibold text-text-secondary">{m.convert.blocked}</h3>
        <span class="text-sm text-text-muted">
          {m.fileOrganize.blockedCount(summary.blocked.length)}
        </span>
      </div>
      <div class="preview-table blocked" role="table" aria-label={m.convert.blocked}>
        <div class="preview-row head" role="row">
          <span role="columnheader">{m.convert.columnSource}</span>
          <span role="columnheader">{m.fileOrganize.columnReason}</span>
        </div>
        {#each previewBlocked as entry (entry.trackId)}
          <div class="preview-row" role="row">
            <span role="cell" class="before">{entry.sourceName}</span>
            <span role="cell" class="reason">
              {m.convert.reasons[entry.status]}
              {#if entry.outputPath}
                <span class="target">{entry.outputPath}</span>
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

  {#if isRunning}
    <div class="progress" role="status">
      <div
        class="bar"
        role="progressbar"
        aria-label={m.convert.converting}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
      >
        <div class="bar-fill" style="width: {percent}%"></div>
      </div>
      <p class="hint mt-2">
        {#if conversion.isCancelling}
          {m.convert.cancelling}
        {:else if conversion.progress}
          {m.convert.progress(conversion.progress.current, conversion.progress.total)}
          <span class="text-text-dimmed ml-2">{conversion.progress.currentFile}</span>
        {:else}
          {m.convert.converting}
        {/if}
      </p>
    </div>
  {/if}

  {#if errors.length > 0}
    <ul class="error-list">
      {#each errors as error (error)}
        <li>{error}</li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    {#if isRunning}
      <button
        class="btn-secondary"
        onclick={() => conversion.cancel()}
        disabled={conversion.isCancelling}
      >
        {m.convert.cancel}
      </button>
      <button class="btn-primary" onclick={onClose}>{m.convert.continueInBackground}</button>
    {:else}
      <button class="btn-secondary" onclick={onClose}>{m.common.cancel}</button>
      <button class="btn-primary" onclick={handleStart} disabled={!canStart}>
        {mode === 'export' ? m.convert.startExport : m.convert.startReplace}
      </button>
    {/if}
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

  .modes {
    @apply flex flex-wrap gap-x-6 gap-y-2 border-none p-0 mx-0 mt-0 mb-4;
  }

  .modes legend {
    @apply w-full p-0;
  }

  .check {
    @apply flex items-center gap-2 text-sm text-text-secondary cursor-pointer;
  }

  .hint {
    @apply mb-0 text-xs text-text-muted;
  }

  .warning {
    @apply mt-3 mb-0 text-sm text-warning;
  }

  .preview {
    @apply mt-4;
  }

  .preview-header {
    @apply flex items-baseline justify-between gap-3 mb-2;
  }

  /* 曲の一覧（元のファイル → 出力先）。多い場合は、この中でスクロールする */
  .preview-table {
    @apply border border-border rounded overflow-y-auto text-sm;
    max-height: 14rem;
  }

  .preview-table.blocked {
    max-height: 8rem;
  }

  .preview-row {
    @apply grid gap-3 px-3 py-1 items-baseline;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr);
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

  /* 変換しない曲の、出力先（理由の下に、小さく出す） */
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

  .error-list {
    @apply mt-3 mb-0 pl-4 text-xs text-error-light max-h-32 overflow-y-auto;
  }
</style>
