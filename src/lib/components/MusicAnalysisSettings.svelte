<!--
  @component MusicAnalysisSettings
  曲の解析（設定ウィンドウの「AI機能」）。macOS 27のMusicUnderstandingで曲のテンポ・キーを解析し、
  AIシャッフルの「似た曲」に使う（ADR-104）。解析はバックグラウンドで1曲ずつ進み、止めるまでに
  終わらなければ、次の起動で続きから再開する。操作はすぐに反映する（「適用」の対象外）。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { musicAnalysis } from '#lib/stores/musicAnalysis.svelte.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  onMount(() => musicAnalysis.watch());

  const status = $derived(musicAnalysis.status);
  /** 解析を終えた曲（解析できなかった曲を含む）の割合 */
  const percent = $derived(
    status && status.total > 0
      ? Math.round(((status.analyzed + status.failed) / status.total) * 100)
      : 0
  );

  async function handleDelete() {
    if (!(await confirmDestructive(m.musicAnalysis.deleteConfirm))) return;
    await musicAnalysis.deleteAll();
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.musicAnalysis.title}</h4>
  <p class="setting-description">{m.musicAnalysis.description}</p>

  {#if status === null}
    <p class="status">{m.common.loading}</p>
  {:else}
    {#if !status.available}
      <p class="status unavailable">{m.musicAnalysis.unavailable}</p>
    {/if}

    <p class="status" role="status">
      {m.musicAnalysis.analyzedCount(status.analyzed, status.total)}
      {#if status.failed > 0}
        <span class="note">{m.musicAnalysis.failedCount(status.failed)}</span>
      {/if}
    </p>
    {#if status.running}
      <div
        class="bar"
        role="progressbar"
        aria-label={m.musicAnalysis.title}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
      >
        <div class="bar-fill" style="width: {percent}%"></div>
      </div>
      <p class="setting-description">
        {musicAnalysis.isStopping ? m.musicAnalysis.stopping : m.musicAnalysis.running}
      </p>
    {/if}

    <div class="actions">
      {#if status.running}
        <button
          class="btn-secondary text-sm"
          onclick={() => musicAnalysis.stop()}
          disabled={musicAnalysis.isStopping}
        >
          {m.musicAnalysis.stop}
        </button>
      {:else}
        <button
          class="btn-secondary text-sm"
          onclick={() => musicAnalysis.start()}
          disabled={!status.available || musicAnalysis.pendingCount === 0}
        >
          {m.musicAnalysis.start}
        </button>
      {/if}
      <button
        class="btn-secondary text-sm"
        onclick={handleDelete}
        disabled={status.running || status.analyzed + status.failed === 0}
      >
        {m.musicAnalysis.delete}
      </button>
    </div>
  {/if}
</section>

<style>
  @reference "../../app.css";

  /* 設定画面（settings/+page.svelte）の項目と同じ見た目にする */
  .settings-section {
    @apply max-w-xl;
  }

  .subsection-title {
    @apply text-base font-semibold mt-8 mb-4;
  }

  .setting-description {
    @apply text-xs text-text-muted mt-1 m-0;
  }

  .status {
    @apply mt-3 mb-0 text-sm text-text-secondary;
  }

  .status.unavailable {
    @apply text-warning;
  }

  .note {
    @apply ml-2 text-text-muted;
  }

  .bar {
    @apply mt-3 h-1.5 rounded-full bg-surface-active overflow-hidden;
  }

  .bar-fill {
    @apply h-full bg-primary transition-[width] duration-300;
  }

  .actions {
    @apply flex gap-2 mt-3;
  }
</style>
