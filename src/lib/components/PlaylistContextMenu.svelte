<!--
  @component PlaylistContextMenu
  プレイリスト用コンテキストメニュー。
  再生、シャッフル再生、キュー操作、名前と説明の編集、フォルダへの移動、M3U8への書き出し、
  削除のアクションを提供する。自動プレイリストでは、条件の編集も選べる。
-->
<script lang="ts">
  import type { Playlist, PlaylistFolder } from '#lib/types/models.js';
  import { BaseContextMenu } from '#lib/components/ui/index.js';
  import {
    useDeletePlaylistMutation,
    usePlacePlaylistMutation,
    usePlaylistTracksQuery
  } from '#lib/queries/playlists.js';
  import {
    addNextInQueue,
    addToQueue,
    playTrackFromQueue,
    playShuffled
  } from '#lib/stores/player.svelte.js';
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
   * プレイリストを再生
   */
  function handlePlay() {
    if (playlistTracks.length > 0) {
      playTrackFromQueue(playlistTracks, 0);
    }
    onClose();
  }

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    playShuffled(playlistTracks);
    onClose();
  }

  /**
   * キューに追加
   */
  function handleAddToQueue() {
    addToQueue(playlistTracks);
    onClose();
  }

  /**
   * 次に再生
   */
  function handlePlayNext() {
    addNextInQueue(playlistTracks);
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
  <div class="menu-header">{playlist.name}</div>
  <div class="menu-subheader">
    {isSmart ? m.smartPlaylist.badge : m.common.trackCount(playlist.tracks.length)}
  </div>
  <div class="menu-divider"></div>

  <button
    class="menu-item"
    onclick={handlePlay}
    role="menuitem"
    disabled={playlistTracks.length === 0}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>{m.contextMenu.playPlaylist}</span>
  </button>

  <button
    class="menu-item"
    onclick={handleShufflePlay}
    role="menuitem"
    disabled={playlistTracks.length === 0}
  >
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
    <span>{m.common.shufflePlay}</span>
  </button>

  <div class="menu-divider"></div>

  <button
    class="menu-item"
    onclick={handlePlayNext}
    role="menuitem"
    disabled={playlistTracks.length === 0}
  >
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
        d="M13 5l7 7-7 7M5 5l7 7-7 7"
      />
    </svg>
    <span>{m.common.playNext}</span>
  </button>

  <button
    class="menu-item"
    onclick={handleAddToQueue}
    role="menuitem"
    disabled={playlistTracks.length === 0}
  >
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
        d="M4 6h16M4 10h16M4 14h16M4 18h7"
      />
    </svg>
    <span>{m.common.addToQueue}</span>
  </button>

  <div class="menu-divider"></div>

  {#if isSmart}
    <button class="menu-item" onclick={handleEditRules} role="menuitem">
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
      <span>{m.smartPlaylist.editRulesEllipsis}</span>
    </button>
  {/if}

  <button class="menu-item" onclick={handleEditInfo} role="menuitem">
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
    <span>{m.playlists.editInfoEllipsis}</span>
  </button>

  {#if onExport}
    <button class="menu-item" onclick={handleExport} role="menuitem">
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
      <span>{m.playlists.exportM3u}</span>
    </button>
  {/if}

  {#if folders.length > 0}
    <div class="menu-divider"></div>
    <div class="menu-label">{m.contextMenu.moveToFolder}</div>
    <div class="folder-list">
      <button
        class="menu-item"
        role="menuitemradio"
        aria-checked={playlist.folderId === null}
        onclick={() => handleMove(null)}
      >
        <span class="menu-check" aria-hidden="true">{playlist.folderId === null ? '✓' : ''}</span>
        <span>{m.contextMenu.noFolder}</span>
      </button>
      {#each folders as folder (folder.id)}
        <button
          class="menu-item"
          role="menuitemradio"
          aria-checked={playlist.folderId === folder.id}
          onclick={() => handleMove(folder.id)}
        >
          <span class="menu-check" aria-hidden="true">
            {playlist.folderId === folder.id ? '✓' : ''}
          </span>
          <span class="truncate">{folder.name}</span>
        </button>
      {/each}
    </div>
    <div class="menu-divider"></div>
  {/if}

  <button class="menu-item menu-item-danger" onclick={handleDelete} role="menuitem">
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
    <span>{m.common.delete}</span>
  </button>
</BaseContextMenu>

<style>
  @reference "../../app.css";

  .menu-header {
    @apply py-1 px-4 text-sm font-semibold text-text-primary;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-subheader {
    @apply pb-2 px-4 text-xs text-text-muted;
  }

  .menu-item {
    @apply flex items-center gap-3 w-full py-2 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover:not(:disabled) {
    @apply bg-surface-active;
  }

  .menu-item:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .menu-item-danger {
    @apply text-error-light;
  }

  .menu-item-danger:hover:not(:disabled) {
    @apply bg-error-light/10;
  }

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
  }

  .menu-label {
    @apply py-1 px-4 text-xs font-semibold text-text-muted uppercase;
  }

  /* フォルダが多い場合は、この中でスクロールする */
  .folder-list {
    max-height: 12rem;
    overflow-y: auto;
  }

  .menu-item .menu-check {
    @apply flex-none w-4 text-primary;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }
</style>
