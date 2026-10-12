<!--
  @component ConvertDialogHost
  形式の変換の画面を、開く要求（`convertDialog`）に合わせて表示する。
  変換先にできる形式を取得してから表示する（変換できない環境では、表示しない）。
-->
<script lang="ts">
  import { useConvertFormatsQuery } from '#lib/queries/convert.js';
  import { convertDialog } from '#lib/stores/convert.svelte.js';
  import ConvertDialog from './ConvertDialog.svelte';

  const formatsQuery = useConvertFormatsQuery();
  const formats = $derived(formatsQuery.data ?? []);
  const tracks = $derived(convertDialog.tracks);
</script>

{#if tracks && formats.length > 0}
  <!-- 開くたびに作り直す（開いている間に一覧が更新されても、入力は保つ） -->
  {#key tracks}
    <ConvertDialog {tracks} {formats} onClose={() => convertDialog.close()} />
  {/key}
{/if}
