<!--
  @component LoudnessScanProgress
  音量の解析の進み具合（読み終わった曲数の棒と、読み終わったファイルの名前）。
  設定ウィンドウの「音量の解析」と、選んだ曲の解析の画面で使う。
-->
<script lang="ts">
  import { loudnessScan } from '#lib/stores/loudnessScan.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const progress = $derived(loudnessScan.progress);
  const percent = $derived(
    progress && progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0
  );
</script>

<div class="scan-progress" role="status">
  <div
    class="bar"
    role="progressbar"
    aria-label={m.loudnessScan.title}
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={percent}
  >
    <div class="bar-fill" style="width: {percent}%"></div>
  </div>
  <p class="status">
    {#if loudnessScan.isCancelling}
      {m.loudnessScan.cancelling}
    {:else if progress}
      {m.loudnessScan.progress(progress.current, progress.total)}
      {#if progress.currentFile}
        <span class="file">{progress.currentFile}</span>
      {/if}
    {:else}
      {m.loudnessScan.preparing}
    {/if}
  </p>
</div>

<style>
  @reference "../../app.css";

  .scan-progress {
    @apply mt-3;
  }

  .bar {
    @apply h-1.5 rounded-full bg-surface-active overflow-hidden;
  }

  .bar-fill {
    @apply h-full bg-primary transition-all duration-200;
  }

  .status {
    @apply mt-2 mb-0 text-xs text-text-muted truncate;
  }

  .file {
    @apply ml-2 text-text-dimmed;
  }
</style>
