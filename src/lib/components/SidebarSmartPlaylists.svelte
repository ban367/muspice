<!--
  @component SidebarSmartPlaylists
  サイドバーのオートプレイリスト（条件で曲を集めるプレイリスト）の一覧（ADR-127）。
  プレイリストの一覧（`SidebarPlaylists`）の上に、別の領域として出す。

  - フォルダはない。並び順は、名前・作成日・手動から選ぶ（見出しのメニュー。プレイリストとは別に覚える）
  - 手動の間は、ドラッグで並べ替えられる
  - 曲のドロップは受け付けない（曲は条件で決まる）
  - 曲数は出さない（開くまで分からない）
-->
<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import {
    useReorderSmartPlaylistsMutation,
    useSaveSmartPlaylistAsPlaylistMutation,
    useSmartPlaylistsQuery
  } from '#lib/queries/smartPlaylists.js';
  import type { PlaylistExportTarget } from '#lib/queries/playlists.js';
  import {
    PLAYLIST_SORTS,
    isPlaylistSort,
    placeRelative,
    sortPlaylistItems
  } from '#lib/utils/playlistTree.js';
  import { playlistSidebar } from '#lib/stores/playlistSidebar.svelte.js';
  import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
  import type { SmartPlaylist } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuHeading,
    MenuRadioGroup,
    MenuRadioItem
  } from '#lib/components/ui/index.js';
  import SmartPlaylistContextMenu from './SmartPlaylistContextMenu.svelte';
  import PlaylistExportDialog from './PlaylistExportDialog.svelte';
  import PlaylistIcon from './PlaylistIcon.svelte';
  import MarqueeText from './MarqueeText.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  /** ドラッグ中のオートプレイリストのIDを運ぶデータの種類（アプリの中だけで使う） */
  const DRAG_TYPE = 'application/x-muspice-smart-playlist-id';

  const currentPath = $derived(page.url.pathname);

  const smartPlaylistsQuery = useSmartPlaylistsQuery();
  const reorderMutation = useReorderSmartPlaylistsMutation();
  const saveAsPlaylistMutation = useSaveSmartPlaylistAsPlaylistMutation();

  const smartPlaylists = $derived(
    sortPlaylistItems(smartPlaylistsQuery.data ?? [], playlistSidebar.smartSort)
  );
  const isManual = $derived(playlistSidebar.smartSort === 'manual');

  // メニュー
  let itemMenu = $state<{ x: number; y: number; smartPlaylist: SmartPlaylist } | null>(null);
  let sectionMenu = $state<{ x: number; y: number } | null>(null);
  // M3U8へ書き出すオートプレイリスト（書き方を選ぶダイアログを表示する）
  let exporting = $state.raw<PlaylistExportTarget | null>(null);

  function openItemMenu(event: MouseEvent, smartPlaylist: SmartPlaylist) {
    event.preventDefault();
    itemMenu = { x: event.clientX, y: event.clientY, smartPlaylist };
  }

  function openSectionMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    sectionMenu = { x: rect.left, y: rect.bottom + 4 };
  }

  // ---- ドラッグでの並べ替え（手動の間だけ） ----

  /** ドロップ先の目印（行の前・後ろの線） */
  let dropHint = $state<{ id: string; where: 'before' | 'after' } | null>(null);

  const isSmartPlaylistDrag = (event: DragEvent) =>
    event.dataTransfer?.types.includes(DRAG_TYPE) ?? false;

  /** ポインターが、行の下半分にあるか（落とした行の後ろへ入れるか） */
  function isLowerHalf(event: DragEvent): boolean {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return event.clientY > rect.top + rect.height / 2;
  }

  function handleDragStart(event: DragEvent, smartPlaylist: SmartPlaylist) {
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(DRAG_TYPE, smartPlaylist.id);
  }

  function handleDragOver(event: DragEvent, target: SmartPlaylist) {
    if (!isManual || !isSmartPlaylistDrag(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    dropHint = { id: target.id, where: isLowerHalf(event) ? 'after' : 'before' };
  }

  function handleDrop(event: DragEvent, target: SmartPlaylist) {
    if (!isManual || !isSmartPlaylistDrag(event)) return;
    event.preventDefault();
    const after = isLowerHalf(event);
    dropHint = null;
    const draggedId = event.dataTransfer?.getData(DRAG_TYPE);
    if (!draggedId || draggedId === target.id) return;
    const order = smartPlaylists.map((playlist) => playlist.id);
    reorderMutation.mutate(placeRelative(order, draggedId, target.id, after));
  }

  const dropWhere = (id: string) => (dropHint?.id === id ? dropHint.where : undefined);
</script>

<div class="smart-playlists-section">
  <div class="nav-section-header">
    <h2 class="text-xs font-semibold uppercase tracking-wider text-text-muted m-0">
      {m.sidebar.smartPlaylists}
    </h2>
    <div class="flex items-center gap-1">
      <button
        class="btn-icon w-6 h-6 p-0"
        title={m.sidebar.newSmartPlaylistTitle}
        aria-label={m.sidebar.newSmartPlaylistTitle}
        onclick={() => smartPlaylistDialog.openNew()}
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
        title={m.sidebar.smartPlaylistMenu}
        aria-label={m.sidebar.smartPlaylistMenu}
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

  <ul class="smart-playlist-list">
    {#if smartPlaylistsQuery.isLoading}
      <li class="nav-message">{m.common.loading}</li>
    {:else if smartPlaylistsQuery.isError}
      <li class="nav-message text-error-light">{m.common.errorOccurred}</li>
    {:else}
      {#each smartPlaylists as smartPlaylist (smartPlaylist.id)}
        <li>
          <a
            href={resolve(`smart-playlists/${smartPlaylist.id}`)}
            class="nav-item-base relative"
            class:active={currentPath === `/smart-playlists/${smartPlaylist.id}`}
            data-drop={dropWhere(smartPlaylist.id)}
            draggable={isManual}
            ondragstart={(e) => handleDragStart(e, smartPlaylist)}
            ondragend={() => (dropHint = null)}
            ondragover={(e) => handleDragOver(e, smartPlaylist)}
            ondragleave={() => (dropHint = null)}
            ondrop={(e) => handleDrop(e, smartPlaylist)}
            oncontextmenu={(e) => openItemMenu(e, smartPlaylist)}
          >
            <PlaylistIcon smart class="nav-icon" />
            <MarqueeText text={smartPlaylist.name} class="flex-1" />
          </a>
        </li>
      {/each}
      {#if smartPlaylists.length === 0}
        <li class="nav-message">{m.sidebar.noSmartPlaylists}</li>
      {/if}
    {/if}
  </ul>
</div>

<!-- オートプレイリストのメニュー -->
{#if itemMenu}
  <SmartPlaylistContextMenu
    x={itemMenu.x}
    y={itemMenu.y}
    smartPlaylist={itemMenu.smartPlaylist}
    onClose={() => (itemMenu = null)}
    onExport={(target) => (exporting = target)}
    onSaveAsPlaylist={(smartPlaylistId, name) =>
      saveAsPlaylistMutation.mutate({ smartPlaylistId, name })}
  />
{/if}

<!-- 見出しのメニュー（並び順） -->
{#if sectionMenu}
  <BaseContextMenu x={sectionMenu.x} y={sectionMenu.y} onClose={() => (sectionMenu = null)}>
    <MenuHeading>{m.sidebar.sortBy}</MenuHeading>
    <MenuRadioGroup
      value={playlistSidebar.smartSort}
      onValueChange={(value) => {
        if (isPlaylistSort(value)) playlistSidebar.smartSort = value;
      }}
    >
      {#each PLAYLIST_SORTS as sort (sort)}
        <MenuRadioItem value={sort}>{m.sidebar.sortOptions[sort]}</MenuRadioItem>
      {/each}
    </MenuRadioGroup>
  </BaseContextMenu>
{/if}

<PlaylistExportDialog playlist={exporting} onClose={() => (exporting = null)} />

<style>
  @reference "../../app.css";

  /* 項目が多い時は、サイドバーの4割までにして、この中でスクロールする（プレイリストの一覧の場所を残す） */
  .smart-playlists-section {
    @apply shrink-0 flex flex-col min-h-0 max-h-[40%] mb-3;
  }

  .smart-playlist-list {
    @apply list-none m-0 p-0 min-h-0 overflow-y-auto rounded-md;
  }

  /* ドロップ先の目印（行の前・後ろの線） */
  [data-drop='before'] {
    box-shadow: inset 0 2px 0 var(--color-primary);
  }

  [data-drop='after'] {
    box-shadow: inset 0 -2px 0 var(--color-primary);
  }
</style>
