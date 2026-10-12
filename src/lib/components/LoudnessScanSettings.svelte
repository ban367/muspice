<!--
  @component LoudnessScanSettings
  音量の解析（設定ウィンドウ用）。音量の情報（ReplayGainのタグ）がない曲の音量を解析して、
  ファイルのタグへ書き込む。解析はバックグラウンドで進み、途中で中断できる（次は、残りの曲から続く）。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { loudnessScan } from '#lib/stores/loudnessScan.svelte.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import LoudnessScanProgress from '#lib/components/LoudnessScanProgress.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  onMount(() => loudnessScan.watch());

  const pendingCount = $derived(loudnessScan.pendingCount);

  async function handleStart() {
    if (loudnessScan.isRunning || !pendingCount) return;
    if (!(await confirmDestructive(m.loudnessScan.confirmMissing(pendingCount)))) return;
    await loudnessScan.start(null);
  }
</script>

<section class="settings-section">
  <h4 class="subsection-title">{m.loudnessScan.title}</h4>
  <p class="setting-description">{m.loudnessScan.description}</p>

  {#if loudnessScan.isRunning}
    <LoudnessScanProgress />
    <button
      class="btn-secondary text-sm mt-3"
      onclick={() => loudnessScan.cancel()}
      disabled={loudnessScan.isCancelling}
    >
      {m.loudnessScan.cancel}
    </button>
  {:else}
    <p class="pending" role="status">
      {#if pendingCount === null}
        {m.common.loading}
      {:else if pendingCount === 0}
        {m.loudnessScan.noPending}
      {:else}
        {m.loudnessScan.pending(pendingCount)}
      {/if}
    </p>
    <button class="btn-secondary text-sm mt-3" onclick={handleStart} disabled={!pendingCount}>
      {m.loudnessScan.startMissing}
    </button>
  {/if}

  {#if loudnessScan.lastResult && loudnessScan.lastResult.errors.length > 0}
    <ul class="error-list">
      {#each loudnessScan.lastResult.errors as error (error)}
        <li>{error}</li>
      {/each}
    </ul>
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

  .pending {
    @apply mt-3 mb-0 text-sm text-text-secondary;
  }

  .error-list {
    @apply mt-3 mb-0 pl-4 text-xs text-error-light max-h-32 overflow-y-auto;
  }
</style>
