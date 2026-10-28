<!--
  @component PlaylistContextMenu
  プレイリスト用コンテキストメニュー。
  再生キューへの追加（次のキューに追加して再生・次のキューに追加・キューの末尾に追加・キューを置き換えて
  再生）、シャッフル再生、名前と説明の編集、フォルダへの移動、M3U8への書き出し、
  削除のアクションを提供する。自動プレイリストでは、条件の編集も選べる。
-->
<script lang="ts">
  import type { Playlist, PlaylistFolder } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuHeading,
    MenuItem,
    MenuRadioGroup,
    MenuRadioItem,
    MenuSeparator,
    QueueMenuItems
  } from '#lib/components/ui/index.js';
  import {
    useDeletePlaylistMutation,
    usePlacePlaylistMutation,
    usePlaylistTracksQuery
  } from '#lib/queries/playlists.js';
  import { playShuffled } from '#lib/stores/player.svelte.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
  import { playlistInfoDialog } from '#lib/stores/playlistSidebar.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    x: number;
    y: number;
    playlist: Playlist;
    /** 移す先に出すフォルダ（1つもなければ、「フォルダへ移動」を出さない） */
    folders?: readonly PlaylistFolder[];
    onClose: () => void;
    /** 「M3U8で書き出す」を選んだ時に呼ぶ（省略すると、項目を出さない） */
    onExport?: (playlist: Playlist) => void;
  }

  let { x, y, playlist, folders = [], onClose, onExport }: Props = $props();

  // ミューテーション
  const deletePlaylistMutation = useDeletePlaylistMutation();
  const placePlaylistMutation = usePlacePlaylistMutation();

  // プレイリスト内のトラック（プレイリストの中の並び順。メニューを開いた時に取得する。
  // 取得するまでは空で、その間、再生・キューの操作は選べない）
  const playlistId = $derived(playlist.id);
  const isSmart = $derived(playlist.rules !== null);
  const tracksQuery = $derived(usePlaylistTracksQuery(playlistId, isSmart));
  const playlistTracks = $derived(tracksQuery.data ?? []);

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    playShuffled(playlistTracks);
    onClose();
  }

  /**
   * プレイリストの名前と説明を編集する
   */
  function handleEditInfo() {
    playlistInfoDialog.open(playlist.id);
    onClose();
  }

  /**
   * プレイリストを、フォルダへ移す（nullは、フォルダの外へ出す）
   */
  // 移動先の「フォルダなし」を表す値（フォルダのIDと重ならない）
  const NO_FOLDER = '';

  function handleMove(folderId: string | null) {
    // メニューを閉じるとこのコンポーネントが破棄されるため、結果は待たない（失敗はトーストで通知される）
    if (folderId !== playlist.folderId) {
      placePlaylistMutation.mutate({ playlistId: playlist.id, folderId });
    }
    onClose();
  }

  /**
   * 自動プレイリストの条件を編集する
   */
  function handleEditRules() {
    smartPlaylistDialog.openEdit(playlist);
    onClose();
  }

  /**
   * プレイリストをM3U8へ書き出す（書き方を選ぶダイアログは、呼び出し側が表示する）
   */
  function handleExport() {
    onExport?.(playlist);
    onClose();
  }

  /**
   * プレイリストを削除
   */
  async function handleDelete() {
    // メニューを閉じるとこのコンポーネントは破棄されるため、使う値は先に取り出しておく
    const { id, name } = playlist;
    onClose();

    if (await confirmDestructive(m.contextMenu.confirmDeletePlaylist(name))) {
      deletePlaylistMutation.mutate(id);
    }
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-title">{playlist.name}</div>
  <div class="menu-subtitle">
    {isSmart ? m.smartPlaylist.badge : m.common.trackCount(playlist.tracks.length)}
  </div>
  <MenuSeparator />

  <QueueMenuItems tracks={playlistTracks} disabled={playlistTracks.length === 0} {onClose} />

  <MenuItem onSelect={handleShufflePlay} disabled={playlistTracks.length === 0}>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
        />
      </svg>
    {/snippet}
    {m.common.shufflePlay}
  </MenuItem>

  <MenuSeparator />

  {#if isSmart}
    <MenuItem onSelect={handleEditRules}>
      {#snippet icon()}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="menu-icon"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
          />
        </svg>
      {/snippet}
      {m.smartPlaylist.editRulesEllipsis}
    </MenuItem>
  {/if}

  <MenuItem onSelect={handleEditInfo}>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
        />
      </svg>
    {/snippet}
    {m.playlists.editInfoEllipsis}
  </MenuItem>

  {#if onExport}
    <MenuItem onSelect={handleExport}>
      {#snippet icon()}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="menu-icon"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
          />
        </svg>
      {/snippet}
      {m.playlists.exportM3u}
    </MenuItem>
  {/if}

  {#if folders.length > 0}
    <MenuSeparator />
    <MenuHeading>{m.contextMenu.moveToFolder}</MenuHeading>
    <!-- フォルダが多い場合は、この中でスクロールする -->
    <MenuRadioGroup
      value={playlist.folderId ?? NO_FOLDER}
      onValueChange={(value) => handleMove(value === NO_FOLDER ? null : value)}
      class="max-h-48 overflow-y-auto"
    >
      <MenuRadioItem value={NO_FOLDER}>{m.contextMenu.noFolder}</MenuRadioItem>
      {#each folders as folder (folder.id)}
        <MenuRadioItem value={folder.id}>{folder.name}</MenuRadioItem>
      {/each}
    </MenuRadioGroup>
    <MenuSeparator />
  {/if}

  <MenuItem onSelect={handleDelete} danger>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
        />
      </svg>
    {/snippet}
    {m.common.delete}
  </MenuItem>
</BaseContextMenu>
