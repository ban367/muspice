<!--
  @component MusicFolderTemplateSetting
  設定の「ライブラリ」の、音楽フォルダの書式（音楽フォルダに曲を置く時の、フォルダとファイルの名前の付け方）。

  書式は設定（`Settings.musicFolderTemplate`）の1項目で、設定の「適用」で保存する。書式の候補は、
  ファイルの整理の画面と同じ。入力が止まったら、最近追加したアルバムの曲を書式に当てはめた例を、
  バックエンドから取り直して出す（置き方の決まりは、ファイルの整理と同じ。`file_organize.rs`）。
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { DEFAULT_MUSIC_FOLDER_TEMPLATE } from '#lib/bindings.js';
  import { fetchMusicFolderTemplatePreview } from '#lib/queries/musicFolder.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { FILE_ORGANIZE_PRESETS } from '#lib/utils/fileOrganize.js';
  import { createDebounce } from '#lib/utils/debounce.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 編集中の書式（適用前） */
    template: string;
  }

  let { template = $bindable() }: Props = $props();

  // 書式の例（書式を変えるたびに取り直す。取り直している間は、前の例を出したままにする）
  let examples = $state.raw<string[] | null>(null);
  let previewError = $state<string | null>(null);
  /** 取り直しの番号（後から始めた取得の結果だけを使う） */
  let previewRequest = 0;

  async function loadExamples(nextTemplate: string) {
    const request = ++previewRequest;
    try {
      const result = await fetchMusicFolderTemplatePreview(nextTemplate);
      if (request !== previewRequest) return;
      examples = result;
      previewError = null;
    } catch (e) {
      if (request !== previewRequest) return;
      examples = null;
      previewError = toErrorMessage(e);
    }
  }

  const previewDebounce = createDebounce(loadExamples, 300);
  onDestroy(() => {
    previewDebounce.cancel();
    // 閉じた後に届いた結果は使わない
    previewRequest++;
  });

  // 最初の例はすぐに、書式を変えた後の例は入力が止まってから取る
  let isFirstPreview = true;
  $effect(() => {
    const nextTemplate = template;
    if (isFirstPreview) {
      isFirstPreview = false;
      void loadExamples(nextTemplate);
    } else {
      previewDebounce.debounced(nextTemplate);
    }
  });
</script>

<div class="template-setting">
  <label class="setting-label" for="music-folder-template">{m.musicFolder.template}</label>
  <p class="setting-description">{m.musicFolder.templateHint}</p>

  <input
    id="music-folder-template"
    type="text"
    class="form-input font-mono mt-3"
    bind:value={template}
    autocomplete="off"
    spellcheck="false"
  />
  <div class="template-options">
    <!-- 書式の候補（選ぶと、入力欄に入れる。候補にない書式の間は「書式の例」と出す） -->
    <select
      class="form-input preset"
      aria-label={m.musicFolder.preset}
      value={(FILE_ORGANIZE_PRESETS as readonly string[]).includes(template) ? template : ''}
      onchange={(e) => {
        if (e.currentTarget.value) template = e.currentTarget.value;
      }}
    >
      <option value="" disabled>{m.musicFolder.preset}</option>
      {#each FILE_ORGANIZE_PRESETS as preset (preset)}
        <option value={preset}>{preset}</option>
      {/each}
    </select>
    <button
      type="button"
      class="setting-reset"
      onclick={() => (template = DEFAULT_MUSIC_FOLDER_TEMPLATE)}
      disabled={template === DEFAULT_MUSIC_FOLDER_TEMPLATE}
    >
      {m.settings.resetToDefault}
    </button>
  </div>
  <p class="setting-description mt-2">{m.musicFolder.templateSyntax}</p>

  {#if previewError}
    <p class="message-error mt-3" role="alert">{previewError}</p>
  {:else if examples}
    <section class="examples" aria-live="polite">
      <p class="examples-title">{m.musicFolder.examples}</p>
      {#if examples.length === 0}
        <p class="setting-description">{m.musicFolder.noExamples}</p>
      {:else}
        <ul class="example-list">
          {#each examples as example, index (index)}
            <li class="example">{example}</li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  @reference "../../app.css";

  .template-setting {
    @apply mb-6;
  }

  .setting-label {
    @apply block text-sm font-medium text-text-primary mb-1;
  }

  .setting-description {
    @apply text-xs text-text-muted m-0;
  }

  .template-options {
    @apply flex items-center gap-4 mt-2;
  }

  .preset {
    @apply flex-1 min-w-0 font-mono cursor-pointer;
  }

  .setting-reset {
    @apply shrink-0 text-xs text-text-secondary bg-transparent border-none cursor-pointer underline p-0;
  }

  .setting-reset:disabled {
    @apply opacity-50 cursor-default no-underline;
  }

  .examples {
    @apply mt-3;
  }

  .examples-title {
    @apply text-xs font-medium text-text-secondary mb-1 m-0;
  }

  .example-list {
    @apply list-none m-0 p-0 border border-border rounded-md divide-y divide-border;
  }

  .example {
    @apply px-3 py-1.5 text-xs text-text-primary font-mono break-all;
  }
</style>
