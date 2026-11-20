<!--
  曲の情報の編集画面（別のウィンドウ）。編集する曲は、Rust側が預かったものを受け取る。
  閉じる（キャンセル・保存の後・ウィンドウの閉じるボタン）と、Rust側がメインウィンドウを操作できるように戻す。
  1曲の編集で、開いた一覧の並びがある時は、前後の曲へ移れる（移ると、編集画面を作り直す。ADR-119）。
-->
<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { useQueryClient } from '@tanstack/svelte-query';
  import MetadataEditor from '#lib/components/MetadataEditor.svelte';
  import {
    moveTrackEditor,
    useTrackEditorPositionQuery,
    useTrackEditorTracksQuery
  } from '#lib/queries/trackEditor.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const queryClient = useQueryClient();
  const tracksQuery = useTrackEditorTracksQuery();
  const positionQuery = useTrackEditorPositionQuery();
  const position = $derived(positionQuery.data ?? null);
  // 移った曲ごとに、編集画面を作り直す（入力欄を、その曲のタグから始める）
  const editorKey = $derived(tracksQuery.data?.map((track) => track.id).join(',') ?? '');

  async function close() {
    await getCurrentWebviewWindow().close();
  }

  async function move(offset: number): Promise<void> {
    try {
      await moveTrackEditor(queryClient, offset);
    } catch {
      // トーストで知らせ済み
    }
  }
</script>

{#if tracksQuery.isPending}
  <p class="message" role="status">{m.metadataEditor.loadingTracks}</p>
{:else if tracksQuery.isError || !tracksQuery.data || tracksQuery.data.length === 0}
  <div class="message">
    <p class="message-error m-0">
      {tracksQuery.error ? toErrorMessage(tracksQuery.error) : m.metadataEditor.noTracks}
    </p>
    <button class="btn-secondary mt-4" onclick={close}>{m.common.close}</button>
  </div>
{:else}
  {#key editorKey}
    <MetadataEditor
      tracks={tracksQuery.data}
      onClose={close}
      {position}
      onMove={position ? move : undefined}
    />
  {/key}
{/if}

<style>
  @reference "../../app.css";

  .message {
    @apply m-0 p-6 text-sm text-text-muted;
  }
</style>
