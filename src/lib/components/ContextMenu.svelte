<!--
  @component ContextMenu
  トラック用コンテキストメニュー。
  再生、キュー操作、プレイリスト追加、お気に入りの切り替え、メタデータ編集、タグの一括ツール、
  アルバムアートの変更、削除などのアクションを提供する。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { BaseContextMenu, PlaylistSubmenu } from '#lib/components/ui/index.js';
  import { addNextInQueue, addToQueue, playSingleTrack } from '#lib/stores/player.svelte.js';
  import { useSetFavoriteMutation, useShowInFolderMutation } from '#lib/queries/tracks.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { tagToolsDialog } from '#lib/stores/tagTools.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    x: number;
    y: number;
    track: Track;
    tracks?: Track[];
    selectedTrackIds?: Set<string>;
    onClose: () => void;
    onEditMetadata?: () => void;
    onAddToQueue?: () => void;
    onPlayNext?: () => void;
    onDelete?: () => void;
    /** 選択した曲を、この一覧（プレイリスト）から外す操作（指定した一覧だけに項目を出す） */
    onRemoveFromList?: () => void;
    /** 一覧から外す項目の表示名 */
    removeFromListLabel?: string;
  }

  let {
    x,
    y,
    track,
    tracks = [],
    selectedTrackIds = new Set(),
    onClose,
    onEditMetadata,
    onAddToQueue,
    onPlayNext,
    onDelete,
    onRemoveFromList,
    removeFromListLabel
  }: Props = $props();

  const showInFolderMutation = useShowInFolderMutation();
  const setFavoriteMutation = useSetFavoriteMutation();

  // 選択されたトラックの数
  const selectedCount = $derived(selectedTrackIds.size > 0 ? selectedTrackIds.size : 1);

  // 選択されたトラックのリスト
  const selectedTracks = $derived.by(() => {
    if (selectedTrackIds.size > 0 && tracks.length > 0) {
      return tracks.filter((t) => selectedTrackIds.has(t.id));
    }
    return [track];
  });

  /**
   * トラックを再生
   */
  function handlePlay() {
    if (selectedTracks.length > 0) {
      playSingleTrack(selectedTracks[0]);
    }
    onClose();
  }

  /**
   * 次に再生（キューの先頭に追加）
   */
  function handlePlayNext() {
    if (onPlayNext) {
      onPlayNext();
    } else {
      // デフォルト動作: キューの現在位置の次に挿入
      addNextInQueue(selectedTracks);
    }
    onClose();
  }

  /**
   * キューに追加（キューの最後に追加）
   */
  function handleAddToQueue() {
    if (onAddToQueue) {
      onAddToQueue();
    } else {
      addToQueue(selectedTracks);
    }
    onClose();
  }

  // 選択した曲がすべてお気に入りなら「外す」、そうでなければ「追加」（まだの曲をまとめて追加する）
  const allFavorite = $derived(selectedTracks.every((selected) => selected.isFavorite));

  /**
   * 選択した曲をまとめてお気に入りにする・お気に入りから外す（失敗はミューテーション内でトースト通知する）
   */
  function handleToggleFavorite() {
    setFavoriteMutation.mutate({
      trackIds: selectedTracks.map((selected) => selected.id),
      favorite: !allFavorite
    });
    onClose();
  }

  /**
   * メタデータを編集
   */
  function handleEditMetadata() {
    if (onEditMetadata) {
      onEditMetadata();
    }
    onClose();
  }

  /**
   * 選択した曲（一覧の順）の、タグの一括ツールを開く
   */
  function handleTagTools() {
    tagToolsDialog.open(selectedTracks);
    onClose();
  }

  /**
   * 選択した曲のアルバムアートの画面を開く
   */
  function handleAlbumArt() {
    albumArtDialog.open(selectedTracks);
    onClose();
  }

  /**
   * ファイルの場所を開く（失敗はミューテーション内でトースト通知する）
   */
  function handleShowInFolder() {
    showInFolderMutation.mutate(track.id);
    onClose();
  }

  /**
   * トラックを削除
   */
  function handleDelete() {
    if (onDelete) {
      onDelete();
    }
    onClose();
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  {#if selectedCount > 1}
    <div class="menu-header">{m.contextMenu.selectedCount(selectedCount)}</div>
    <div class="menu-divider"></div>
  {/if}

  <button class="menu-item" onclick={handlePlay} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>{m.common.play}</span>
  </button>

  <button class="menu-item" onclick={handlePlayNext} role="menuitem">
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

  <button class="menu-item" onclick={handleAddToQueue} role="menuitem">
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

  <PlaylistSubmenu tracks={selectedTracks} {onClose} />

  <button class="menu-item" onclick={handleToggleFavorite} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      fill={allFavorite ? 'currentColor' : 'none'}
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
      />
    </svg>
    <span>{allFavorite ? m.common.removeFromFavorites : m.common.addToFavorites}</span>
  </button>

  <div class="menu-divider"></div>

  {#if onEditMetadata}
    <button class="menu-item" onclick={handleEditMetadata} role="menuitem">
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
      <span>{m.contextMenu.editMetadata}</span>
      <span class="menu-shortcut">Ctrl+I</span>
    </button>
  {/if}

  <button class="menu-item" onclick={handleTagTools} role="menuitem">
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
        d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z"
      />
    </svg>
    <span>{m.contextMenu.tagTools}</span>
  </button>

  <button class="menu-item" onclick={handleAlbumArt} role="menuitem">
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
        d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
      />
    </svg>
    <span>{m.contextMenu.albumArt}</span>
  </button>

  <button class="menu-item" onclick={handleShowInFolder} role="menuitem">
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
        d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
      />
    </svg>
    <span>{m.contextMenu.showInFolder}</span>
  </button>

  {#if onRemoveFromList || onDelete}
    <div class="menu-divider"></div>
  {/if}

  {#if onRemoveFromList}
    <button
      class="menu-item"
      onclick={() => {
        onRemoveFromList();
        onClose();
      }}
      role="menuitem"
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
          d="M6 18L18 6M6 6l12 12"
        />
      </svg>
      <span>{removeFromListLabel}</span>
    </button>
  {/if}

  {#if onDelete}
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
      <span>{m.contextMenu.deleteEllipsis}</span>
    </button>
  {/if}
</BaseContextMenu>

<style>
  @reference "../../app.css";

  .menu-header {
    @apply py-2 px-4 text-xs font-semibold text-text-muted uppercase;
  }

  .menu-item {
    @apply flex items-center gap-3 w-full py-2 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover {
    @apply bg-surface-active;
  }

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
  }

  .menu-shortcut {
    @apply text-xs text-text-dimmed shrink-0;
    flex: none !important;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }

  .menu-item-danger {
    @apply text-error;
  }

  .menu-item-danger:hover {
    @apply bg-error/10;
  }
</style>
