<!--
  @component PlaylistInfoDialogHost
  プレイリストの情報の編集画面を、開く要求（`playlistInfoDialog`）に合わせて表示する。
  編集するプレイリストは、IDからプレイリストの一覧で探す（見つからなければ閉じる）。
-->
<script lang="ts">
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { playlistInfoDialog } from '#lib/stores/playlistSidebar.svelte.js';
  import PlaylistInfoDialog from './PlaylistInfoDialog.svelte';

  const playlistsQuery = usePlaylistsQuery();
  const playlistId = $derived(playlistInfoDialog.playlistId);
  const playlist = $derived(
    playlistId === null
      ? null
      : (playlistsQuery.data?.find((item) => item.id === playlistId) ?? null)
  );
</script>

{#if playlist}
  <!-- 開くたびに作り直す（開いている間に一覧が更新されても、入力は保つ） -->
  {#key playlistId}
    <PlaylistInfoDialog {playlist} onClose={() => playlistInfoDialog.close()} />
  {/key}
{/if}
