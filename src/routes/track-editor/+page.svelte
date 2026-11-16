<!--
  曲の情報の編集画面（別のウィンドウ）。編集する曲は、Rust側が預かったものを受け取る。
  閉じる（キャンセル・保存の後・ウィンドウの閉じるボタン）と、Rust側がメインウィンドウを操作できるように戻す。
-->
<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import MetadataEditor from '#lib/components/MetadataEditor.svelte';
  import { useTrackEditorTracksQuery } from '#lib/queries/trackEditor.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const tracksQuery = useTrackEditorTracksQuery();

  async function close() {
    await getCurrentWebviewWindow().close();
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
  <MetadataEditor tracks={tracksQuery.data} onClose={close} />
{/if}

<style>
  @reference "../../app.css";

  .message {
    @apply m-0 p-6 text-sm text-text-muted;
  }
</style>
