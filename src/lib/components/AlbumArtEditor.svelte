<!--
  @component AlbumArtEditor
  曲の情報の編集画面の「アルバムアート」のタブ。今の画像（1曲目のもの）と、それがどこの画像か（ファイルに
  埋め込まれた画像・フォルダの画像）を出し、新しい画像を選ぶ・埋め込みの画像を取り除くを選ばせる。

  - 選んだ変更は、編集画面の「保存」でファイルへ書き込む（ここでは書き込まない。「キャンセル」で何も変えない。ADR-094）
  - 新しい画像はRust側のダイアログで選び、Rust側が預かる（見本の画像と、預かった画像を指す印を受け取る）
  - ファイルが見つからない曲は、対象にしない
-->
<script module lang="ts">
  import type { PickedAlbumArt } from '#lib/types/models.js';

  /** 保存する時に行う、アルバムアートの変更 */
  export type AlbumArtChange =
    { kind: 'none' } | { kind: 'set'; picked: PickedAlbumArt } | { kind: 'remove' };
</script>

<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { useAlbumArtInfoQuery, usePickAlbumArtMutation } from '#lib/queries/albumArt.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatFileSize } from '#lib/utils/format.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import AlbumArt from './AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 編集する曲（開いた時点のもの） */
    tracks: Track[];
    /** 保存する時に行う変更 */
    change: AlbumArtChange;
    /** 保存の間など、変えられない時 */
    disabled?: boolean;
  }

  let { tracks, change = $bindable(), disabled = false }: Props = $props();

  // 扱う曲は、開いた時点のものを使う（開いている間に一覧が更新されても変えない）
  // svelte-ignore state_referenced_locally
  const targets = tracks.filter((track) => !track.isMissing);
  // svelte-ignore state_referenced_locally
  const missingCount = tracks.length - targets.length;
  // 画像を表示する曲（書き込める曲があれば、その1曲目）
  // svelte-ignore state_referenced_locally
  const previewTrack = targets[0] ?? tracks[0];
  const trackTitle = (track: Track) => track.title || track.fileName;

  const infoQuery = useAlbumArtInfoQuery(previewTrack.id);
  const info = $derived(infoQuery.data ?? null);
  const pickMutation = usePickAlbumArtMutation();
  const isPicking = $derived(pickMutation.isPending);

  /** 画像の種類の表示名 */
  const FORMAT_NAMES: Record<string, string> = { 'image/jpeg': 'JPEG', 'image/png': 'PNG' };

  /** 画像の詳細（大きさ・種類・サイズ） */
  function describe(image: {
    width: number | null;
    height: number | null;
    mimeType: string;
    size: number;
  }): string {
    return [
      image.width && image.height ? m.albumArtEditor.dimensions(image.width, image.height) : null,
      FORMAT_NAMES[image.mimeType] ?? image.mimeType,
      formatFileSize(image.size)
    ]
      .filter((part) => part !== null)
      .join(' · ');
  }

  // 取り除ける画像があるか（複数の曲では、1曲目になくてもほかの曲にあることがある）
  const canRemove = $derived(targets.length > 1 || info?.source === 'embedded');

  let error = $state<string | null>(null);

  async function handleChoose() {
    error = null;
    try {
      const picked = await pickMutation.mutateAsync();
      // 画像を選ばなかった場合は、何も変えない
      if (picked) change = { kind: 'set', picked };
    } catch (e) {
      error = toErrorMessage(e);
    }
  }

  function handleRemove() {
    error = null;
    change = { kind: 'remove' };
  }

  function handleUndo() {
    error = null;
    change = { kind: 'none' };
  }
</script>

<div class="layout">
  <div class="preview" class:pending={change.kind !== 'none'}>
    {#if change.kind === 'set'}
      <img src={change.picked.dataUrl} alt={m.albumArtEditor.picked} class="picked" />
    {:else if change.kind === 'remove'}
      <AlbumArt src={null} alt="" rounded="lg" />
    {:else}
      <AlbumArt src={albumArtUrl(previewTrack.id)} alt={m.common.albumArt} rounded="lg" />
    {/if}
  </div>

  <div class="min-w-0 flex-1">
    <p class="heading">
      {tracks.length === 1 ? trackTitle(previewTrack) : m.albumArtEditor.target(targets.length)}
    </p>
    {#if tracks.length > 1 && change.kind === 'none'}
      <p class="sub">{m.albumArtEditor.previewOf(trackTitle(previewTrack))}</p>
    {/if}

    <div class="info" aria-live="polite">
      {#if change.kind === 'set'}
        <p>{m.albumArtEditor.picked}</p>
        <p class="text-text-muted tabular-nums">{describe(change.picked)}</p>
      {:else if change.kind === 'remove'}
        <p>{m.albumArtEditor.none}</p>
      {:else if infoQuery.isPending}
        <p class="text-text-muted">{m.common.loading}</p>
      {:else if info}
        <p>
          {info.source === 'embedded'
            ? m.albumArtEditor.embedded
            : m.albumArtEditor.folder(info.fileName ?? '')}
        </p>
        <p class="text-text-muted tabular-nums">{describe(info)}</p>
      {:else}
        <p class="text-text-muted">{m.albumArtEditor.none}</p>
      {/if}
    </div>

    {#if change.kind === 'none' && info?.source === 'folder'}
      <p class="hint">{m.albumArtEditor.folderHint}</p>
    {/if}
  </div>
</div>

{#if targets.length === 0}
  <div class="message-error mt-4">{m.albumArtEditor.allMissing}</div>
{:else}
  {#if missingCount > 0}
    <p class="hint mt-4">{m.albumArtEditor.missingExcluded(missingCount)}</p>
  {/if}

  <div class="actions">
    <button
      type="button"
      class="btn-primary"
      onclick={handleChoose}
      disabled={disabled || isPicking}
    >
      {m.albumArtEditor.choose}
    </button>
    <button
      type="button"
      class="btn-secondary"
      onclick={handleRemove}
      disabled={disabled || isPicking || !canRemove || change.kind === 'remove'}
    >
      {m.albumArtEditor.remove}
    </button>
    {#if change.kind !== 'none'}
      <button type="button" class="btn-secondary" onclick={handleUndo} {disabled}>
        {m.albumArtEditor.undo}
      </button>
    {/if}
  </div>

  <div aria-live="polite">
    {#if change.kind === 'set'}
      <div class="message-info mt-3">{m.albumArtEditor.willEmbed(targets.length)}</div>
    {:else if change.kind === 'remove'}
      <div class="message-warning mt-3">{m.albumArtEditor.willRemove(targets.length)}</div>
    {/if}
    {#if error}
      <div class="message-error mt-3">{error}</div>
    {/if}
  </div>

  <p class="hint mt-3">{m.albumArtEditor.writesToFile}</p>
{/if}

<style>
  @reference "../../app.css";

  .layout {
    @apply flex gap-5 items-start;
  }

  .preview {
    @apply w-44 h-44 shrink-0 rounded-lg overflow-hidden;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  /* 保存すると変わる画像（まだ書き込んでいない）は、枠で示す */
  .preview.pending {
    @apply outline-2 outline-primary outline-offset-2;
  }

  .picked {
    @apply w-full h-full object-cover;
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

  .info p {
    @apply m-0 break-words;
  }

  .info p + p {
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
</style>
