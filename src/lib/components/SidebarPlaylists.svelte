<!--
  @component SidebarPlaylists
  サイドバーのプレイリストの一覧。

  - フォルダ（1階層）を先に、フォルダの外のプレイリストを後に出す。フォルダは開閉できる
  - 並び順は、名前・作成日・手動から選ぶ（見出しのメニュー）。手動の間は、プレイリストと
    フォルダをドラッグで並べ替えられる
  - プレイリストは、どの並び順でも、ドラッグでフォルダへ出し入れできる（右クリックのメニューからも移せる）
  - 曲の一覧からドラッグした曲は、プレイリストへ追加する（自動プレイリストには追加できない）

  並び順と、閉じているフォルダは`#lib/stores/playlistSidebar.svelte`が覚える（ADR-043）。
-->
<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import {
    usePlaylistsQuery,
    usePlaylistFoldersQuery,
    useAddTracksToPlaylistMutation,
    useCreatePlaylistMutation,
    useCreatePlaylistFolderMutation,
    useDeletePlaylistFolderMutation,
    usePlacePlaylistMutation,
    useRenamePlaylistFolderMutation,
    useReorderPlaylistFoldersMutation
  } from '#lib/queries/playlists.js';
  import { combineQueryStates } from '#lib/queries/shared.js';
  import {
    validatePlaylistFolderName,
    validatePlaylistName,
    toSafeString
  } from '#lib/utils/validation.js';
  import { confirmDestructive, promptText } from '#lib/utils/dialog.svelte.js';
  import { isTrackDrag, readDraggedTrackIds } from '#lib/utils/trackDrag.js';
  import {
    PLAYLIST_SORTS,
    buildPlaylistTree,
    placeRelative,
    playlistsIn,
    sortPlaylistItems
  } from '#lib/utils/playlistTree.js';
  import { playlistSidebar } from '#lib/stores/playlistSidebar.svelte.js';
  import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
  import type { Playlist, PlaylistFolder } from '#lib/types/models.js';
  import { BaseContextMenu } from '#lib/components/ui/index.js';
  import PlaylistContextMenu from './PlaylistContextMenu.svelte';
  import PlaylistExportDialog from './PlaylistExportDialog.svelte';
  import PlaylistIcon from './PlaylistIcon.svelte';
  import PlaylistImportButton from './PlaylistImportButton.svelte';
  import MarqueeText from './MarqueeText.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  /** ドラッグ中のプレイリスト・フォルダのIDを運ぶデータの種類（アプリの中だけで使う） */
  const PLAYLIST_DRAG_TYPE = 'application/x-muspice-playlist-id';
  const FOLDER_DRAG_TYPE = 'application/x-muspice-playlist-folder-id';

  const currentPath = $derived(page.url.pathname);

  const playlistsQuery = usePlaylistsQuery();
  const foldersQuery = usePlaylistFoldersQuery();
  const queryState = $derived(combineQueryStates(playlistsQuery, foldersQuery));
  const addTracksMutation = useAddTracksToPlaylistMutation();
  const createPlaylistMutation = useCreatePlaylistMutation();
  const createFolderMutation = useCreatePlaylistFolderMutation();
  const renameFolderMutation = useRenamePlaylistFolderMutation();
  const deleteFolderMutation = useDeletePlaylistFolderMutation();
  const placePlaylistMutation = usePlacePlaylistMutation();
  const reorderFoldersMutation = useReorderPlaylistFoldersMutation();

  const playlists = $derived(playlistsQuery.data ?? []);
  const folders = $derived(foldersQuery.data ?? []);
  const tree = $derived(buildPlaylistTree(folders, playlists, playlistSidebar.sort));
  const isManual = $derived(playlistSidebar.sort === 'manual');

  // メニュー
  let playlistMenu = $state<{ x: number; y: number; playlist: Playlist } | null>(null);
  let folderMenu = $state<{ x: number; y: number; folder: PlaylistFolder } | null>(null);
  let sectionMenu = $state<{ x: number; y: number } | null>(null);
  // M3U8へ書き出すプレイリスト（書き方を選ぶダイアログを表示する）
  let exportingPlaylist = $state.raw<Playlist | null>(null);

  function openPlaylistMenu(event: MouseEvent, playlist: Playlist) {
    event.preventDefault();
    playlistMenu = { x: event.clientX, y: event.clientY, playlist };
  }

  function openFolderMenu(event: MouseEvent, folder: PlaylistFolder) {
    event.preventDefault();
    folderMenu = { x: event.clientX, y: event.clientY, folder };
  }

  function openSectionMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    sectionMenu = { x: rect.left, y: rect.bottom + 4 };
  }

  /** 新規プレイリストを作成 */
  async function handleCreatePlaylist() {
    const name = await promptText({
      title: m.sidebar.newPlaylist,
      label: m.sidebar.playlistName,
      confirmLabel: m.sidebar.create,
      validate: (value) => validatePlaylistName(value).error ?? null
    });
    if (name === null) return;

    createPlaylistMutation.mutate(toSafeString(name, 100));
  }

  /** 新規フォルダを作成 */
  async function handleCreateFolder() {
    sectionMenu = null;
    const name = await promptText({
      title: m.sidebar.newFolder,
      label: m.sidebar.folderName,
      confirmLabel: m.sidebar.create,
      validate: validatePlaylistFolderName
    });
    if (name === null) return;

    createFolderMutation.mutate(name.trim());
  }

  async function handleRenameFolder(folder: PlaylistFolder) {
    folderMenu = null;
    const name = await promptText({
      title: m.sidebar.renameFolderTitle,
      label: m.sidebar.newFolderName,
      defaultValue: folder.name,
      confirmLabel: m.contextMenu.renameConfirm,
      validate: validatePlaylistFolderName
    });
    if (name !== null && name.trim() !== folder.name) {
      renameFolderMutation.mutate({ folderId: folder.id, name: name.trim() });
    }
  }

  async function handleDeleteFolder(folder: PlaylistFolder) {
    folderMenu = null;
    if (await confirmDestructive(m.sidebar.confirmDeleteFolder(folder.name))) {
      deleteFolderMutation.mutate(folder.id);
    }
  }

  // ---- ドラッグ&ドロップ ----

  /** ドロップ先の目印（行の前・後ろの線、フォルダの中） */
  type DropHint = { id: string; where: 'before' | 'after' | 'into' };
  let dropHint = $state<DropHint | null>(null);
  /** フォルダの外（一覧の空いているところ）へ落とそうとしているか */
  let isOverRoot = $state(false);

  const dragKind = (event: DragEvent): 'track' | 'playlist' | 'folder' | null => {
    const types = event.dataTransfer?.types ?? [];
    if (types.includes(PLAYLIST_DRAG_TYPE)) return 'playlist';
    if (types.includes(FOLDER_DRAG_TYPE)) return 'folder';
    return isTrackDrag(event) ? 'track' : null;
  };

  /** ポインターが、行の下半分にあるか（落とした行の後ろへ入れるか） */
  function isLowerHalf(event: DragEvent): boolean {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return event.clientY > rect.top + rect.height / 2;
  }

  function startDrag(event: DragEvent, type: string, id: string) {
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(type, id);
  }

  function clearDrop() {
    dropHint = null;
    isOverRoot = false;
  }

  function accept(event: DragEvent, effect: 'copy' | 'move', hint: DropHint | null) {
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = effect;
    dropHint = hint;
    isOverRoot = false;
  }

  /** プレイリストの行の上: 曲は追加、プレイリストはその行の前後（手動の間）か、同じフォルダへ */
  function handlePlaylistDragOver(event: DragEvent, playlist: Playlist) {
    const kind = dragKind(event);
    if (kind === 'track') {
      // 自動プレイリストの曲は条件で決まるため、曲のドロップは受け付けない
      if (playlist.rules !== null) return;
      accept(event, 'copy', { id: playlist.id, where: 'into' });
    } else if (kind === 'playlist') {
      const where = isManual ? (isLowerHalf(event) ? 'after' : 'before') : 'into';
      accept(event, 'move', { id: playlist.id, where });
    }
  }

  function handlePlaylistDrop(event: DragEvent, target: Playlist) {
    const kind = dragKind(event);
    if (kind === null || kind === 'folder') return;
    event.preventDefault();
    event.stopPropagation();
    const after = isLowerHalf(event);
    clearDrop();

    if (kind === 'track') {
      if (target.rules !== null) return;
      // ドラッグした曲（選択していた曲すべて）を、一覧の並び順のまま1回で追加する
      const trackIds = readDraggedTrackIds(event);
      if (trackIds.length > 0) addTracksMutation.mutate({ playlistId: target.id, trackIds });
      return;
    }

    const playlistId = event.dataTransfer?.getData(PLAYLIST_DRAG_TYPE);
    const dragged = playlists.find((playlist) => playlist.id === playlistId);
    if (!dragged || dragged.id === target.id) return;

    if (isManual) {
      const order = playlistsIn(tree, target.folderId).map((playlist) => playlist.id);
      placePlaylistMutation.mutate({
        playlistId: dragged.id,
        folderId: target.folderId,
        orderedIds: placeRelative(order, dragged.id, target.id, after)
      });
    } else if (dragged.folderId !== target.folderId) {
      placePlaylistMutation.mutate({ playlistId: dragged.id, folderId: target.folderId });
    }
  }

  /** フォルダの行の上: プレイリストは中へ、フォルダはその行の前後（手動の間だけ） */
  function handleFolderDragOver(event: DragEvent, folder: PlaylistFolder) {
    const kind = dragKind(event);
    if (kind === 'playlist') {
      accept(event, 'move', { id: folder.id, where: 'into' });
    } else if (kind === 'folder' && isManual) {
      accept(event, 'move', { id: folder.id, where: isLowerHalf(event) ? 'after' : 'before' });
    }
  }

  /** ドラッグしてきたプレイリストを、フォルダの中（いちばん後ろ）へ入れる */
  function dropPlaylistIntoFolder(event: DragEvent, target: PlaylistFolder) {
    const playlistId = event.dataTransfer?.getData(PLAYLIST_DRAG_TYPE);
    const dragged = playlists.find((playlist) => playlist.id === playlistId);
    if (!dragged || dragged.folderId === target.id) return;
    placePlaylistMutation.mutate({ playlistId: dragged.id, folderId: target.id });
    // 入れたことが分かるよう、閉じていたフォルダは開く
    playlistSidebar.setCollapsed(target.id, false);
  }

  function handleFolderDrop(event: DragEvent, target: PlaylistFolder) {
    const kind = dragKind(event);
    if (kind === null || kind === 'track') return;
    event.preventDefault();
    event.stopPropagation();
    const after = isLowerHalf(event);
    clearDrop();

    if (kind === 'playlist') {
      dropPlaylistIntoFolder(event, target);
      return;
    }

    const folderId = event.dataTransfer?.getData(FOLDER_DRAG_TYPE);
    if (!isManual || !folderId || folderId === target.id) return;
    const order = sortPlaylistItems(folders, 'manual').map((folder) => folder.id);
    reorderFoldersMutation.mutate(placeRelative(order, folderId, target.id, after));
  }

  /** フォルダの中の空いているところ（「プレイリストなし」の行など）の上: プレイリストを中へ入れる */
  function handleFolderBodyDragOver(event: DragEvent, folder: PlaylistFolder) {
    if (dragKind(event) === 'playlist') accept(event, 'move', { id: folder.id, where: 'into' });
  }

  function handleFolderBodyDrop(event: DragEvent, target: PlaylistFolder) {
    if (dragKind(event) !== 'playlist') return;
    event.preventDefault();
    event.stopPropagation();
    clearDrop();
    dropPlaylistIntoFolder(event, target);
  }

  /** 一覧の空いているところの上: プレイリストを、フォルダの外へ出す */
  function handleRootDragOver(event: DragEvent) {
    if (dragKind(event) !== 'playlist') return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropHint = null;
    isOverRoot = true;
  }

  function handleRootDrop(event: DragEvent) {
    if (dragKind(event) !== 'playlist') return;
    event.preventDefault();
    clearDrop();
    const playlistId = event.dataTransfer?.getData(PLAYLIST_DRAG_TYPE);
    const dragged = playlists.find((playlist) => playlist.id === playlistId);
    if (dragged && dragged.folderId !== null) {
      placePlaylistMutation.mutate({ playlistId: dragged.id, folderId: null });
    }
  }

  /** その行がドロップ先なら、目印の出し方（`data-drop`属性にする） */
  const dropWhere = (id: string) => (dropHint?.id === id ? dropHint.where : undefined);
</script>

{#snippet playlistItem(playlist: Playlist)}
  {@const isSmart = playlist.rules !== null}
  <li>
    <a
      href={resolve(`playlists/${playlist.id}`)}
      class="nav-item-base relative"
      class:active={currentPath === `/playlists/${playlist.id}`}
      data-drop={dropWhere(playlist.id)}
      draggable="true"
      ondragstart={(e) => startDrag(e, PLAYLIST_DRAG_TYPE, playlist.id)}
      ondragend={clearDrop}
      ondragover={(e) => handlePlaylistDragOver(e, playlist)}
      ondragleave={() => (dropHint = null)}
      ondrop={(e) => handlePlaylistDrop(e, playlist)}
      oncontextmenu={(e) => openPlaylistMenu(e, playlist)}
    >
      <PlaylistIcon smart={isSmart} class="w-5 h-5 shrink-0" />
      <MarqueeText text={playlist.name} class="flex-1" />
      <!-- 自動プレイリストの曲数は、開くまで分からないため出さない -->
      {#if !isSmart}
        <span class="text-xs text-text-dimmed shrink-0 ml-2">{playlist.tracks.length}</span>
      {/if}
    </a>
  </li>
{/snippet}

<div class="flex-1 flex flex-col min-h-0 mb-6">
  <div class="flex items-center justify-between mb-2 px-2">
    <h2 class="text-xs font-semibold uppercase tracking-wider text-text-muted m-0">
      {m.sidebar.playlists}
    </h2>
    <div class="flex items-center gap-1">
      <PlaylistImportButton />
      <button
        class="btn-icon w-6 h-6 p-0"
        title={m.sidebar.newSmartPlaylistTitle}
        aria-label={m.sidebar.newSmartPlaylistTitle}
        onclick={() => smartPlaylistDialog.openNew()}
      >
        <PlaylistIcon smart class="w-4 h-4" />
      </button>
      <button
        class="btn-icon w-6 h-6 p-0"
        title={m.sidebar.newPlaylistTitle}
        aria-label={m.sidebar.newPlaylistTitle}
        onclick={handleCreatePlaylist}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-4 h-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 4v16m8-8H4"
          />
        </svg>
      </button>
      <button
        class="btn-icon w-6 h-6 p-0"
        title={m.sidebar.playlistMenu}
        aria-label={m.sidebar.playlistMenu}
        aria-haspopup="menu"
        onclick={openSectionMenu}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-4 h-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M5 12h.01M12 12h.01M19 12h.01M6 12a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0z"
          />
        </svg>
      </button>
    </div>
  </div>

  <!-- 一覧の空いているところへ落とすと、フォルダの外へ出る -->
  <ul
    class="playlist-list"
    class:drop-root={isOverRoot}
    ondragover={handleRootDragOver}
    ondragleave={() => (isOverRoot = false)}
    ondrop={handleRootDrop}
  >
    {#if queryState.isLoading}
      <li class="px-3 py-2 text-sm text-text-dimmed">{m.common.loading}</li>
    {:else if queryState.isError}
      <li class="px-3 py-2 text-sm text-error-light">{m.common.errorOccurred}</li>
    {:else}
      {#each tree.folders as node (node.folder.id)}
        {@const isCollapsed = playlistSidebar.isCollapsed(node.folder.id)}
        <li>
          <button
            type="button"
            class="nav-item-base folder-row"
            data-drop={dropWhere(node.folder.id)}
            aria-expanded={!isCollapsed}
            title={isCollapsed ? m.sidebar.expandFolder : m.sidebar.collapseFolder}
            draggable={isManual}
            onclick={() => playlistSidebar.toggleFolder(node.folder.id)}
            ondragstart={(e) => startDrag(e, FOLDER_DRAG_TYPE, node.folder.id)}
            ondragend={clearDrop}
            ondragover={(e) => handleFolderDragOver(e, node.folder)}
            ondragleave={() => (dropHint = null)}
            ondrop={(e) => handleFolderDrop(e, node.folder)}
            oncontextmenu={(e) => openFolderMenu(e, node.folder)}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="chevron"
              class:open={!isCollapsed}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              aria-hidden="true"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 5l7 7-7 7"
              />
            </svg>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="w-5 h-5 shrink-0"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              aria-hidden="true"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
              />
            </svg>
            <span class="flex-1 truncate text-left">{node.folder.name}</span>
            <span
              class="text-xs text-text-dimmed shrink-0 ml-2"
              title={m.sidebar.playlistCountInFolder(node.playlists.length)}
            >
              {node.playlists.length}
            </span>
          </button>
          {#if !isCollapsed}
            <ul
              class="folder-children"
              ondragover={(e) => handleFolderBodyDragOver(e, node.folder)}
              ondragleave={() => (dropHint = null)}
              ondrop={(e) => handleFolderBodyDrop(e, node.folder)}
            >
              {#each node.playlists as playlist (playlist.id)}
                {@render playlistItem(playlist)}
              {/each}
              {#if node.playlists.length === 0}
                <li class="px-3 py-1.5 text-xs text-text-dimmed">{m.sidebar.emptyFolder}</li>
              {/if}
            </ul>
          {/if}
        </li>
      {/each}
      {#each tree.root as playlist (playlist.id)}
        {@render playlistItem(playlist)}
      {/each}
      {#if playlists.length === 0 && folders.length === 0}
        <li class="px-3 py-2 text-sm text-text-dimmed">{m.sidebar.noPlaylists}</li>
      {/if}
    {/if}
  </ul>
</div>

<!-- プレイリストのメニュー -->
{#if playlistMenu}
  <PlaylistContextMenu
    x={playlistMenu.x}
    y={playlistMenu.y}
    playlist={playlistMenu.playlist}
    {folders}
    onClose={() => (playlistMenu = null)}
    onExport={(playlist) => (exportingPlaylist = playlist)}
  />
{/if}

<!-- フォルダのメニュー -->
{#if folderMenu}
  {@const folder = folderMenu.folder}
  <BaseContextMenu x={folderMenu.x} y={folderMenu.y} onClose={() => (folderMenu = null)}>
    <div class="menu-header">{folder.name}</div>
    <div class="menu-divider"></div>
    <button class="menu-item" role="menuitem" onclick={() => handleRenameFolder(folder)}>
      {m.contextMenu.rename}
    </button>
    <button
      class="menu-item menu-item-danger"
      role="menuitem"
      onclick={() => handleDeleteFolder(folder)}
    >
      {m.sidebar.deleteFolder}
    </button>
  </BaseContextMenu>
{/if}

<!-- 見出しのメニュー（フォルダの作成と、並び順） -->
{#if sectionMenu}
  <BaseContextMenu x={sectionMenu.x} y={sectionMenu.y} onClose={() => (sectionMenu = null)}>
    <button class="menu-item" role="menuitem" onclick={handleCreateFolder}>
      <span class="check" aria-hidden="true"></span>
      {m.sidebar.newFolder}
    </button>
    <div class="menu-divider"></div>
    <div class="menu-label">{m.sidebar.sortBy}</div>
    {#each PLAYLIST_SORTS as sort (sort)}
      <button
        class="menu-item"
        role="menuitemradio"
        aria-checked={playlistSidebar.sort === sort}
        onclick={() => {
          playlistSidebar.sort = sort;
          sectionMenu = null;
        }}
      >
        <span class="check" aria-hidden="true">{playlistSidebar.sort === sort ? '✓' : ''}</span>
        {m.sidebar.sortOptions[sort]}
      </button>
    {/each}
  </BaseContextMenu>
{/if}

<PlaylistExportDialog playlist={exportingPlaylist} onClose={() => (exportingPlaylist = null)} />

<style>
  @reference "../../app.css";

  .playlist-list {
    @apply list-none m-0 p-0 flex-1 overflow-y-auto rounded-md;
  }

  /* フォルダの外へ出す（一覧の空いているところへのドロップ） */
  .playlist-list.drop-root {
    @apply outline-2 outline-dashed outline-primary -outline-offset-2;
  }

  .folder-row {
    @apply w-full gap-2 bg-transparent border-none cursor-pointer text-sm;
    font: inherit;
  }

  .chevron {
    @apply w-3.5 h-3.5 shrink-0 text-text-muted transition-transform duration-150;
  }

  .chevron.open {
    @apply rotate-90;
  }

  .folder-children {
    @apply list-none m-0 p-0 ml-4 border-l border-border;
  }

  /* ドロップ先の目印（行の前・後ろの線、フォルダ・プレイリストの中） */
  [data-drop='before'] {
    box-shadow: inset 0 2px 0 var(--color-primary);
  }

  [data-drop='after'] {
    box-shadow: inset 0 -2px 0 var(--color-primary);
  }

  [data-drop='into'] {
    @apply bg-primary/30 outline-2 outline-dashed outline-primary -outline-offset-2;
  }

  .menu-header {
    @apply py-1 px-4 text-sm font-semibold text-text-primary truncate;
  }

  .menu-label {
    @apply py-1.5 px-4 text-xs font-semibold text-text-muted uppercase;
  }

  .menu-item {
    @apply flex items-center gap-2 w-full py-2 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover {
    @apply bg-surface-active;
  }

  .menu-item-danger {
    @apply text-error-light;
  }

  .menu-item-danger:hover {
    @apply bg-error-light/10;
  }

  .check {
    @apply w-4 shrink-0 text-primary;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }
</style>
