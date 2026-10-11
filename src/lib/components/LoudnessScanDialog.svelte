<!--
  @component LoudnessScanDialog
  選んだ曲の音量の解析。曲の音量（ラウドネス）を解析し直して、ゲインとピークをファイルのタグ
  （ReplayGain）へ書き込む。アルバムのゲインは、アルバムの全曲から求めて、全曲に書き込む。

  解析はバックグラウンドで進むため、実行中に閉じても続く（終わると通知が出る）。
  ファイルが見つからない曲は、対象にしない。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { loudnessScan } from '#lib/stores/loudnessScan.svelte.js';
  import LoudnessScanProgress from '#lib/components/LoudnessScanProgress.svelte';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 解析する曲 */
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const targets = tracks.filter((track) => !track.isMissing);
  // svelte-ignore state_referenced_locally
  const missingCount = tracks.length - targets.length;

  /** この画面から始めた解析の、書き込めなかった理由（あれば、閉じずに表示する） */
  let errors = $state.raw<string[]>([]);
  /** この画面から解析を始めたか（ほかで始まった解析の完了では、閉じない） */
  let isStarted = $state(false);

  async function handleStart() {
    isStarted = true;
    const result = await loudnessScan.startForTracks(targets);
    if (result && result.errors.length > 0) {
      errors = result.errors;
    } else if (result) {
      onClose();
    }
  }
</script>

<Modal open {onClose} title={m.loudnessScan.dialogTitle(targets.length)} class="max-w-lg">
  <p class="description">{m.loudnessScan.dialogDescription}</p>

  {#if loudnessScan.isRunning}
    {#if !isStarted}
      <p class="notice" role="status">{m.loudnessScan.alreadyRunning}</p>
    {/if}
    <LoudnessScanProgress />
  {/if}

  {#if missingCount > 0}
    <p class="hint">{m.albumArtDialog.missingExcluded(missingCount)}</p>
  {/if}

  {#if errors.length > 0}
    <ul class="error-list">
      {#each errors as error (error)}
        <li>{error}</li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    {#if loudnessScan.isRunning}
      <button
        class="btn-secondary"
        onclick={() => loudnessScan.cancel()}
        disabled={loudnessScan.isCancelling}
      >
        {m.loudnessScan.cancel}
      </button>
      <button class="btn-primary" onclick={onClose}>{m.loudnessScan.continueInBackground}</button>
    {:else if errors.length > 0}
      <button class="btn-primary" onclick={onClose}>{m.common.close}</button>
    {:else}
      <button class="btn-secondary" onclick={onClose}>{m.common.cancel}</button>
      <button class="btn-primary" onclick={handleStart} disabled={targets.length === 0}>
        {m.loudnessScan.start}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .description {
    @apply m-0 text-sm text-text-secondary;
  }

  .notice {
    @apply mt-3 mb-0 text-sm text-warning;
  }

  .hint {
    @apply mt-3 mb-0 text-xs text-text-muted;
  }

  .error-list {
    @apply mt-3 mb-0 pl-4 text-xs text-error-light max-h-40 overflow-y-auto;
  }
</style>
