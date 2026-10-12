<!--
  @component AlbumArtDialog
  曲のアルバムアートの画面。今の画像（1曲目のもの）と、それがどこの画像か（ファイルに
  埋め込まれた画像・フォルダの画像）を出し、画像の埋め込みと取り除きを行う。

  - 埋め込む画像はRust側のダイアログで選び、選んだ時点でファイルへ書き込む（保存の操作はない）
  - 取り除く前に、画面の中で確認する（ファイルの画像は元に戻せない）
  - ファイルが見つからない曲は、対象にしない
-->
<script lang="ts">
  import type { BulkUpdateResult, Track } from '#lib/types/models.js';
  import {
    useAlbumArtInfoQuery,
    useRemoveAlbumArtMutation,
    useSetAlbumArtMutation
  } from '#lib/queries/albumArt.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatFileSize } from '#lib/utils/format.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import AlbumArt from './AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    tracks: Track[];
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const targets = tracks.filter((track) => !track.isMissing);
  // svelte-ignore state_referenced_locally
  const missingCount = tracks.length - targets.length;
  // 画像を表示する曲（書き込める曲があれば、その1曲目）
  // svelte-ignore state_referenced_locally
  const previewTrack = targets[0] ?? tracks[0];
  const targetIds = targets.map((track) => track.id);
  const trackTitle = (track: Track) => track.title || track.fileName;

  const infoQuery = useAlbumArtInfoQuery(previewTrack.id);
  const info = $derived(infoQuery.data ?? null);
  const setMutation = useSetAlbumArtMutation();
  const removeMutation = useRemoveAlbumArtMutation();
  const isWorking = $derived(setMutation.isPending || removeMutation.isPending);

  /** 画像の種類の表示名 */
  const FORMAT_NAMES: Record<string, string> = { 'image/jpeg': 'JPEG', 'image/png': 'PNG' };

  /** 画像の詳細（大きさ・種類・サイズ） */
  const details = $derived.by(() => {
    if (!info) return null;
    return [
      info.width && info.height ? m.albumArtDialog.dimensions(info.width, info.height) : null,
      FORMAT_NAMES[info.mimeType] ?? info.mimeType,
      formatFileSize(info.size)
    ]
      .filter((part) => part !== null)
      .join(' · ');
  });

  // 取り除ける画像があるか（複数の曲では、1曲目になくてもほかの曲にあることがある）
  const canRemove = $derived(targets.length > 1 || info?.source === 'embedded');

  let isConfirmingRemove = $state(false);
  let notice = $state<string | null>(null);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);

  function clearMessages() {
    notice = null;
    warning = null;
    error = null;
  }

  /** 書き込みの結果を、画面の中に出す */
  function showResult(result: BulkUpdateResult, done: (count: number) => string, none: string) {
    if (result.updatedCount > 0) {
      notice = done(result.updatedCount);
    } else if (result.failedCount === 0) {
      notice = none;
    }
    if (result.failedCount > 0) {
      if (result.updatedCount === 0) {
        error = m.albumArtDialog.allFailed(result.errors[0] ?? '');
      } else {
        warning = m.albumArtDialog.partiallyFailed(result.failedCount);
      }
    }
  }

  async function handleChoose() {
    clearMessages();
    isConfirmingRemove = false;
    try {
      const result = await setMutation.mutateAsync(targetIds);
      // 画像を選ばなかった場合は、何も変えない
      if (result) showResult(result, m.albumArtDialog.embeddedResult, '');
    } catch (e) {
      error = toErrorMessage(e);
    }
  }

  async function handleRemove() {
    clearMessages();
    isConfirmingRemove = false;
    try {
      const result = await removeMutation.mutateAsync(targetIds);
      showResult(result, m.albumArtDialog.removedResult, m.albumArtDialog.nothingToRemove);
    } catch (e) {
      error = toErrorMessage(e);
    }
  }
</script>

<Modal open {onClose} title={m.albumArtDialog.title} dismissible={!isWorking} class="max-w-lg">
  <div class="layout">
    <div class="preview">
      <AlbumArt src={albumArtUrl(previewTrack.id)} alt={m.albumArtDialog.title} rounded="lg" />
    </div>

    <div class="min-w-0 flex-1">
      <p class="heading">
        {tracks.length === 1 ? trackTitle(previewTrack) : m.albumArtDialog.target(targets.length)}
      </p>
      {#if tracks.length > 1}
        <p class="sub">{m.albumArtDialog.previewOf(trackTitle(previewTrack))}</p>
      {/if}

      <dl class="info" aria-live="polite">
        {#if infoQuery.isPending}
          <dd class="text-text-muted">{m.common.loading}</dd>
        {:else if info}
          <dd>
            {info.source === 'embedded'
              ? m.albumArtDialog.embedded
              : m.albumArtDialog.folder(info.fileName ?? '')}
          </dd>
          <dd class="text-text-muted tabular-nums">{details}</dd>
        {:else}
          <dd class="text-text-muted">{m.albumArtDialog.none}</dd>
        {/if}
      </dl>

      {#if info?.source === 'folder'}
        <p class="hint">{m.albumArtDialog.folderHint}</p>
      {/if}
    </div>
  </div>

  {#if targets.length === 0}
    <div class="message-error mt-4">{m.albumArtDialog.allMissing}</div>
  {:else}
    {#if missingCount > 0}
      <p class="hint mt-4">{m.albumArtDialog.missingExcluded(missingCount)}</p>
    {/if}

    <div class="actions">
      <button class="btn-primary" onclick={handleChoose} disabled={isWorking} data-autofocus>
        {m.albumArtDialog.choose}
      </button>
      <button
        class="btn-secondary"
        onclick={() => {
          clearMessages();
          isConfirmingRemove = true;
        }}
        disabled={isWorking || !canRemove || isConfirmingRemove}
      >
        {m.albumArtDialog.remove}
      </button>
    </div>

    {#if isConfirmingRemove}
      <div class="confirm" role="alertdialog" aria-label={m.albumArtDialog.remove}>
        <p class="m-0 text-sm">{m.albumArtDialog.confirmRemove(targets.length)}</p>
        <div class="flex gap-2 justify-end mt-3">
          <button class="btn-secondary" onclick={() => (isConfirmingRemove = false)}>
            {m.common.cancel}
          </button>
          <button class="btn-danger" onclick={handleRemove}>
            {m.albumArtDialog.confirmRemoveAction}
          </button>
        </div>
      </div>
    {/if}

    <p class="hint mt-3">{m.albumArtDialog.writesToFile}</p>
  {/if}

  <div aria-live="polite">
    {#if isWorking}
      <p class="text-sm text-text-muted mt-3 mb-0" role="status">{m.albumArtDialog.working}</p>
    {/if}
    {#if notice}
      <div class="message-success mt-3">{notice}</div>
    {/if}
    {#if warning}
      <div class="message-info mt-3">{warning}</div>
    {/if}
    {#if error}
      <div class="message-error mt-3">{error}</div>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn-secondary" onclick={onClose} disabled={isWorking}>{m.common.close}</button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .layout {
    @apply flex gap-5 items-start;
  }

  .preview {
    @apply w-44 h-44 shrink-0 rounded-lg overflow-hidden;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  .heading {
    @apply m-0 font-semibold text-text-primary break-words;
  }

  .sub {
    @apply mt-1 mb-0 text-xs text-text-muted break-words;
  }

  .info {
    @apply mt-3 mb-0 text-sm text-text-primary;
  }

  .info dd {
    @apply m-0 break-words;
  }

  .info dd + dd {
    @apply mt-1;
  }

  .hint {
    @apply mb-0 text-xs text-text-muted;
  }

  .info + .hint {
    @apply mt-3;
  }

  .actions {
    @apply flex flex-wrap gap-2 mt-4;
  }

  .confirm {
    @apply mt-3 p-3 rounded border border-border bg-base-200;
  }
</style>
