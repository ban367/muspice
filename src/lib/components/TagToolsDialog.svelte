<!--
  @component TagToolsDialog
  タグの一括ツール。選んだ曲のタグを、規則に従ってまとめて書き換える。

  - ファイル名から: 書式（`%track% - %title%`など）に合わせて、ファイル名・フォルダ名から読み取る
  - 連番: 一覧の順に、トラック番号を振り直す
  - 検索と置換: 選んだ項目の文字列を置き換える（大文字・小文字の変換、前後の空白の削除を含む）

  入力を変えるたびに、変更の一覧（前 → 後）を作り直す。「適用」を押すまで、ファイルには書き込まない。
  変更の計算は`#lib/utils/tagTools`が行う。ファイルが見つからない曲は、対象にしない。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { useApplyMetadataChangesMutation } from '#lib/queries/tracks.js';
  import {
    FILE_NAME_PATTERN_PRESETS,
    TEXT_TOOL_FIELDS,
    countFieldChanges,
    guessFromFileName,
    renumberTracks,
    searchAndReplace,
    toMetadataChanges,
    type CaseConversion,
    type TextToolField,
    type ToolField,
    type ToolResult
  } from '#lib/utils/tagTools.js';
  import { showSuccess, showWarning, toErrorMessage } from '#lib/stores/error.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 対象の曲（一覧の順） */
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const targets = tracks.filter((track) => !track.isMissing);
  // svelte-ignore state_referenced_locally
  const missingCount = tracks.length - targets.length;

  /** 変更の一覧に出す曲数の上限（多いと描画が重い。書き込みは、すべての曲に行う） */
  const PREVIEW_LIMIT = 200;

  type Tool = 'fileName' | 'renumber' | 'replace';
  const tools: Tool[] = ['fileName', 'renumber', 'replace'];
  let activeTool = $state<Tool>('fileName');

  // ファイル名から
  let pattern = $state<string>(FILE_NAME_PATTERN_PRESETS[0]);

  // 連番
  let startNumber = $state('1');
  let setTotal = $state(true);

  // 検索と置換
  let replaceFields = $state<TextToolField[]>(['title']);
  let search = $state('');
  let replace = $state('');
  let matchCase = $state(false);
  let useRegex = $state(false);
  let caseConversion = $state<CaseConversion>('none');
  let trim = $state(false);
  const caseConversions: CaseConversion[] = ['none', 'upper', 'lower', 'title'];

  const fieldLabels: Record<ToolField, string> = {
    title: m.fields.title,
    artist: m.fields.artist,
    album: m.fields.album,
    albumArtist: m.fields.albumArtist,
    genre: m.fields.genre,
    year: m.fields.year,
    trackNumber: m.fields.trackNumber,
    trackTotal: m.fields.trackTotal,
    discNumber: m.fields.discNumber
  };

  /** 今の入力での変更（入力を変えるたびに作り直す） */
  const result = $derived.by((): ToolResult => {
    switch (activeTool) {
      case 'fileName':
        return guessFromFileName(targets, pattern);
      case 'renumber':
        return renumberTracks(targets, {
          start: /^\d+$/.test(startNumber.trim()) ? Number(startNumber.trim()) : NaN,
          setTotal
        });
      case 'replace':
        return searchAndReplace(targets, {
          fields: replaceFields,
          search,
          replace,
          matchCase,
          useRegex,
          caseConversion,
          trim
        });
    }
  });

  const inputError = $derived.by(() => {
    const { error, errorDetail = '' } = result;
    if (error === null) return null;
    const message = m.tagTools.errors[error];
    return typeof message === 'function' ? message(errorDetail) : message;
  });
  const previewChanges = $derived(result.changes.slice(0, PREVIEW_LIMIT));
  const hiddenCount = $derived(result.changes.length - previewChanges.length);

  const applyMutation = useApplyMetadataChangesMutation();
  const isApplying = $derived(applyMutation.isPending);
  let applyError = $state<string | null>(null);

  function toggleField(field: TextToolField, checked: boolean) {
    replaceFields = checked
      ? TEXT_TOOL_FIELDS.filter((f) => f === field || replaceFields.includes(f))
      : replaceFields.filter((f) => f !== field);
  }

  async function handleApply() {
    applyError = null;
    try {
      const applied = await applyMutation.mutateAsync(toMetadataChanges(result.changes));
      if (applied.updatedCount === 0) {
        // 1曲も書き込めなかった場合は、画面を閉じずに理由を表示する
        applyError = m.tagTools.allFailed(applied.errors[0] ?? '');
        return;
      }
      if (applied.failedCount > 0) {
        showWarning(m.tagTools.partiallyFailed(applied.updatedCount, applied.failedCount));
      } else {
        showSuccess(m.tagTools.applied(applied.updatedCount));
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
  title={m.tagTools.title(targets.length)}
  dismissible={!isApplying}
  class="max-w-3xl"
>
  <div class="tab-list" role="tablist">
    {#each tools as tool (tool)}
      <button
        type="button"
        class="tab"
        class:active={activeTool === tool}
        role="tab"
        aria-selected={activeTool === tool}
        onclick={() => (activeTool = tool)}
        disabled={isApplying}
      >
        {m.tagTools.tabs[tool]}
      </button>
    {/each}
  </div>

  <form class="options" onsubmit={(e) => e.preventDefault()}>
    {#if activeTool === 'fileName'}
      <div class="form-row">
        <div class="form-group grow">
          <label for="tag-tools-pattern" class="form-label">{m.tagTools.pattern}</label>
          <input
            id="tag-tools-pattern"
            type="text"
            class="form-input font-mono"
            bind:value={pattern}
            autocomplete="off"
            spellcheck="false"
            disabled={isApplying}
            data-autofocus
          />
        </div>
        <div class="form-group">
          <label for="tag-tools-preset" class="form-label">{m.tagTools.patternPreset}</label>
          <select
            id="tag-tools-preset"
            class="form-input"
            value={(FILE_NAME_PATTERN_PRESETS as readonly string[]).includes(pattern)
              ? pattern
              : ''}
            onchange={(e) => {
              if (e.currentTarget.value) pattern = e.currentTarget.value;
            }}
            disabled={isApplying}
          >
            <option value="" hidden></option>
            {#each FILE_NAME_PATTERN_PRESETS as preset (preset)}
              <option value={preset}>{preset}</option>
            {/each}
          </select>
        </div>
      </div>
      <p class="hint">{m.tagTools.patternHint}</p>
    {:else if activeTool === 'renumber'}
      <p class="hint mt-0 mb-3">{m.tagTools.renumberHint}</p>
      <div class="form-row items-end">
        <div class="form-group">
          <label for="tag-tools-start" class="form-label">{m.tagTools.startNumber}</label>
          <input
            id="tag-tools-start"
            type="text"
            inputmode="numeric"
            class="form-input w-24 text-center"
            bind:value={startNumber}
            disabled={isApplying}
          />
        </div>
        <label class="check mb-4">
          <input type="checkbox" class="w-4 h-4" bind:checked={setTotal} disabled={isApplying} />
          {m.tagTools.setTotal}
        </label>
      </div>
    {:else}
      <fieldset class="fields">
        <legend class="form-label">{m.tagTools.targetFields}</legend>
        {#each TEXT_TOOL_FIELDS as field (field)}
          <label class="check">
            <input
              type="checkbox"
              class="w-4 h-4"
              checked={replaceFields.includes(field)}
              onchange={(e) => toggleField(field, e.currentTarget.checked)}
              disabled={isApplying}
            />
            {fieldLabels[field]}
          </label>
        {/each}
      </fieldset>
      <div class="form-row">
        <div class="form-group grow">
          <label for="tag-tools-search" class="form-label">{m.tagTools.search}</label>
          <input
            id="tag-tools-search"
            type="text"
            class="form-input"
            bind:value={search}
            autocomplete="off"
            spellcheck="false"
            disabled={isApplying}
          />
        </div>
        <div class="form-group grow">
          <label for="tag-tools-replace" class="form-label">{m.tagTools.replaceWith}</label>
          <input
            id="tag-tools-replace"
            type="text"
            class="form-input"
            bind:value={replace}
            autocomplete="off"
            spellcheck="false"
            disabled={isApplying}
          />
        </div>
      </div>
      <div class="form-row items-center">
        <label class="check">
          <input type="checkbox" class="w-4 h-4" bind:checked={matchCase} disabled={isApplying} />
          {m.tagTools.matchCase}
        </label>
        <label class="check">
          <input type="checkbox" class="w-4 h-4" bind:checked={useRegex} disabled={isApplying} />
          {m.tagTools.useRegex}
        </label>
        <label class="check">
          <input type="checkbox" class="w-4 h-4" bind:checked={trim} disabled={isApplying} />
          {m.tagTools.trim}
        </label>
        <label class="check">
          {m.tagTools.caseConversion}
          <select class="form-input w-auto" bind:value={caseConversion} disabled={isApplying}>
            {#each caseConversions as conversion (conversion)}
              <option value={conversion}>{m.tagTools.caseOptions[conversion]}</option>
            {/each}
          </select>
        </label>
      </div>
    {/if}
  </form>

  {#if inputError}
    <div class="message-error mt-3" role="alert">{inputError}</div>
  {/if}

  <section class="preview" aria-live="polite">
    <div class="preview-header">
      <h3 class="m-0 text-sm font-semibold text-text-secondary">{m.tagTools.preview}</h3>
      <span class="text-sm text-text-muted">
        {result.changes.length > 0
          ? m.tagTools.summary(result.changes.length, countFieldChanges(result.changes))
          : m.tagTools.noChanges}
      </span>
    </div>

    {#if result.changes.length > 0}
      <div class="preview-table" role="table" aria-label={m.tagTools.preview}>
        <div class="preview-row head" role="row">
          <span role="columnheader">{m.tagTools.columnTrack}</span>
          <span role="columnheader">{m.tagTools.columnField}</span>
          <span role="columnheader">{m.tagTools.columnBefore}</span>
          <span role="columnheader">{m.tagTools.columnAfter}</span>
        </div>
        {#each previewChanges as change (change.track.id)}
          {#each change.fields as field, index (field.field)}
            <div class="preview-row" class:first={index === 0} role="row">
              <span role="cell" class="file" title={change.track.filePath}>
                {index === 0 ? change.track.fileName : ''}
              </span>
              <span role="cell" class="text-text-muted">{fieldLabels[field.field]}</span>
              <span role="cell" class="before">
                {#if field.before === null}
                  <span class="text-text-dimmed">
                    {field.field === 'trackTotal' ? m.tagTools.unknown : m.tagTools.empty}
                  </span>
                {:else}
                  {field.before}
                {/if}
              </span>
              <span role="cell" class="after">
                {#if field.after === ''}
                  <span class="text-text-dimmed">{m.tagTools.empty}</span>
                {:else}
                  {field.after}
                {/if}
              </span>
            </div>
          {/each}
        {/each}
      </div>
      {#if hiddenCount > 0}
        <p class="hint mt-2">{m.tagTools.moreChanges(hiddenCount)}</p>
      {/if}
    {/if}

    {#if result.unmatchedCount > 0}
      <p class="hint mt-2">{m.tagTools.unmatched(result.unmatchedCount)}</p>
    {/if}
    {#if missingCount > 0}
      <p class="hint mt-2">{m.albumArtDialog.missingExcluded(missingCount)}</p>
    {/if}
  </section>

  {#if applyError}
    <div class="message-error mt-3">{applyError}</div>
  {/if}

  {#snippet footer()}
    <button class="btn-secondary" onclick={onClose} disabled={isApplying}>{m.common.cancel}</button>
    <button
      class="btn-primary"
      onclick={handleApply}
      disabled={isApplying || result.changes.length === 0}
    >
      {isApplying ? m.tagTools.applying : m.tagTools.apply}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .tab-list {
    @apply flex gap-1 mb-4 border-b border-border;
  }

  .tab {
    @apply bg-transparent border-none px-4 py-2 text-sm text-text-muted cursor-pointer border-b-2 border-transparent transition-colors;
    margin-bottom: -1px;
  }

  .tab:hover:not(:disabled) {
    @apply text-text-primary;
  }

  .tab.active {
    @apply text-primary border-primary font-semibold;
  }

  .form-row {
    @apply flex flex-wrap gap-x-4 gap-y-2;
  }

  .grow {
    @apply flex-1 min-w-48;
  }

  .fields {
    @apply flex flex-wrap gap-x-4 gap-y-2 border-none p-0 mx-0 mt-0 mb-4;
  }

  .fields legend {
    @apply w-full p-0;
  }

  .check {
    @apply flex items-center gap-2 text-sm text-text-secondary cursor-pointer whitespace-nowrap;
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

  /* 変更の一覧（曲・項目・前・後）。多い場合は、この中でスクロールする */
  .preview-table {
    @apply border border-border rounded overflow-y-auto text-sm;
    max-height: 16rem;
  }

  .preview-row {
    @apply grid gap-3 px-3 py-1 items-baseline;
    grid-template-columns: minmax(0, 2fr) 7rem minmax(0, 2fr) minmax(0, 2fr);
  }

  .preview-row.head {
    @apply sticky top-0 bg-base-200 text-xs font-semibold text-text-muted uppercase border-b border-border;
  }

  /* 曲の最初の行の上に、区切りの線を引く */
  .preview-row.first:not(:nth-child(2)) {
    @apply border-t border-border;
  }

  .preview-row span[role='cell'] {
    @apply min-w-0 break-words;
  }

  .file {
    @apply text-text-secondary;
  }

  .before {
    @apply text-text-muted;
  }

  .after {
    @apply text-text-primary font-medium;
  }
</style>
