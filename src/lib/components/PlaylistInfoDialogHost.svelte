<!--
  @component PlaylistInfoDialogHost
  プレイリスト・オートプレイリストの情報の編集画面を、開く要求（`playlistInfoDialog`）に合わせて表示する。
  編集するものは、IDから一覧で探す（見つからなければ閉じる）。
-->
<script lang="ts">
  import { usePlaylistsQuery } from '#lib/queries/playlists.js';
  import { useSmartPlaylistsQuery } from '#lib/queries/smartPlaylists.js';
  import { playlistInfoDialog } from '#lib/stores/playlistSidebar.svelte.js';
  import PlaylistInfoDialog from './PlaylistInfoDialog.svelte';

  const playlistsQuery = usePlaylistsQuery();
  const smartPlaylistsQuery = useSmartPlaylistsQuery();
  const target = $derived(playlistInfoDialog.target);
  const playlist = $derived.by(() => {
    if (target === null) return null;
    const list = target.kind === 'playlist' ? playlistsQuery.data : smartPlaylistsQuery.data;
    return list?.find((item) => item.id === target.id) ?? null;
  });
</script>

{#if target && playlist}
  <!-- 開くたびに作り直す（開いている間に一覧が更新されても、入力は保つ） -->
  {#key target}
    <PlaylistInfoDialog kind={target.kind} {playlist} onClose={() => playlistInfoDialog.close()} />
  {/key}
{/if}
