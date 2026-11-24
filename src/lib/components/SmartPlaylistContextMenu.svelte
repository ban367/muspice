<!--
  @component SmartPlaylistContextMenu
  オートプレイリスト用のコンテキストメニュー（ADR-127）。
  再生キューへの追加、条件の編集、名前と説明の編集、プレイリストとして保存（今の曲で、通常の
  プレイリストを作る）、M3U8への書き出し、削除のアクションを提供する。
-->
<script lang="ts">
  import type { SmartPlaylist } from '#lib/types/models.js';
  import type { PlaylistExportTarget } from '#lib/queries/playlists.js';
  import {
    BaseContextMenu,
    MenuItem,
    MenuSeparator,
    QueueMenuItems
  } from '#lib/components/ui/index.js';
  import {
    useDeleteSmartPlaylistMutation,
    useSmartPlaylistTracksQuery
  } from '#lib/queries/smartPlaylists.js';
  import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
  import {
    promptSaveAsPlaylistName,
    smartPlaylistDialog
  } from '#lib/stores/smartPlaylist.svelte.js';
  import { playlistInfoDialog } from '#lib/stores/playlistSidebar.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    x: number;
    y: number;
    smartPlaylist: SmartPlaylist;
    onClose: () => void;
    /** 「M3U8で書き出す」を選んだ時に呼ぶ（書き方を選ぶダイアログは、呼び出し側が表示する） */
    onExport: (target: PlaylistExportTarget) => void;
    /** 「プレイリストとして保存...」で、名前を決めた時に呼ぶ */
    onSaveAsPlaylist: (smartPlaylistId: string, name: string) => void;
  }

  let { x, y, smartPlaylist, onClose, onExport, onSaveAsPlaylist }: Props = $props();

  const deleteMutation = useDeleteSmartPlaylistMutation();

  // 条件に合う曲（メニューを開いた時に求める。求めるまでは空で、その間、再生・キューの操作は選べない）
  const smartPlaylistId = $derived(smartPlaylist.id);
  const tracksQuery = $derived(useSmartPlaylistTracksQuery(smartPlaylistId));
  const tracks = $derived(tracksQuery.data ?? []);

  function handleEditRules() {
    smartPlaylistDialog.openEdit(smartPlaylist);
    onClose();
  }

  function handleEditInfo() {
    playlistInfoDialog.openSmartPlaylist(smartPlaylist.id);
    onClose();
  }

  async function handleSaveAsPlaylist() {
    // メニューを閉じるとこのコンポーネントは破棄されるため、使う値は先に取り出しておく
    const { id, name } = smartPlaylist;
    onClose();
    const newName = await promptSaveAsPlaylistName(name);
    if (newName !== null) onSaveAsPlaylist(id, newName);
  }

  function handleExport() {
    onExport({ kind: 'smartPlaylist', id: smartPlaylist.id, name: smartPlaylist.name });
    onClose();
  }

  async function handleDelete() {
    const { id, name } = smartPlaylist;
    onClose();

    if (await confirmDestructive(m.smartPlaylist.confirmDelete(name))) {
      deleteMutation.mutate(id);
    }
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-title">{smartPlaylist.name}</div>
  <div class="menu-subtitle">{m.smartPlaylist.title}</div>
  <MenuSeparator />

  <QueueMenuItems {tracks} disabled={tracks.length === 0} {onClose} />

  <MenuSeparator />

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

  <MenuItem onSelect={handleSaveAsPlaylist}>
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
          d="M4 6h16M4 10h16M4 14h10m4 0v6m-3-3h6"
        />
      </svg>
    {/snippet}
    {m.smartPlaylist.saveAsPlaylist}
  </MenuItem>

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

  <MenuSeparator />

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
