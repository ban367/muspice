<!--
  @component ContextMenu
  トラック用コンテキストメニュー。
  再生、キュー操作、プレイリスト追加、お気に入りの切り替え、メタデータ編集、タグの一括ツール、
  ファイルの整理、形式の変換、音量の解析、アルバムアートの変更、削除などのアクションを提供する。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuHeading,
    MenuItem,
    MenuSeparator,
    PlaylistSubmenu
  } from '#lib/components/ui/index.js';
  import { addNextInQueue, addToQueue, playSingleTrack } from '#lib/stores/player.svelte.js';
  import { useSetFavoriteMutation, useShowInFolderMutation } from '#lib/queries/tracks.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { tagToolsDialog } from '#lib/stores/tagTools.svelte.js';
  import { fileOrganizeDialog } from '#lib/stores/fileOrganize.svelte.js';
  import { loudnessScanDialog } from '#lib/stores/loudnessScan.svelte.js';
  import { convertDialog } from '#lib/stores/convert.svelte.js';
  import { useConvertFormatsQuery } from '#lib/queries/convert.js';
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
  // 形式の変換は、変換できる環境（macOS）でだけ項目を出す
  const convertFormatsQuery = useConvertFormatsQuery();
  const canConvert = $derived((convertFormatsQuery.data?.length ?? 0) > 0);
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
   * 選択した曲（一覧の順）の、ファイルの整理の画面を開く
   */
  function handleFileOrganize() {
    fileOrganizeDialog.open(selectedTracks);
    onClose();
  }

  /**
   * 選択した曲（一覧の順）の、形式の変換の画面を開く
   */
  function handleConvert() {
    convertDialog.open(selectedTracks);
    onClose();
  }

  /**
   * 選択した曲の、音量の解析の画面を開く
   */
  function handleLoudnessScan() {
    loudnessScanDialog.open(selectedTracks);
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
    <MenuHeading>{m.contextMenu.selectedCount(selectedCount)}</MenuHeading>
    <MenuSeparator />
  {/if}

  <MenuItem onSelect={handlePlay}>
    {#snippet icon()}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="menu-icon"
        viewBox="0 0 24 24"
        fill="currentColor"
      >
        <path d="M8 5v14l11-7z" />
      </svg>
    {/snippet}
    {m.common.play}
  </MenuItem>

  <MenuItem onSelect={handlePlayNext}>
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
          d="M13 5l7 7-7 7M5 5l7 7-7 7"
        />
      </svg>
    {/snippet}
    {m.common.playNext}
  </MenuItem>

  <MenuItem onSelect={handleAddToQueue}>
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
          d="M4 6h16M4 10h16M4 14h16M4 18h7"
        />
      </svg>
    {/snippet}
    {m.common.addToQueue}
  </MenuItem>

  <MenuSeparator />

  <PlaylistSubmenu tracks={selectedTracks} {onClose} />

  <MenuItem onSelect={handleToggleFavorite}>
    {#snippet icon()}
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
    {/snippet}
    {allFavorite ? m.common.removeFromFavorites : m.common.addToFavorites}
  </MenuItem>

  <MenuSeparator />

  {#if onEditMetadata}
    <MenuItem onSelect={handleEditMetadata} shortcut="Ctrl+I">
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
      {m.contextMenu.editMetadata}
    </MenuItem>
  {/if}

  <MenuItem onSelect={handleTagTools}>
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
          d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z"
        />
      </svg>
    {/snippet}
    {m.contextMenu.tagTools}
  </MenuItem>

  <MenuItem onSelect={handleFileOrganize}>
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
          d="M9 13h6m-3-3l3 3-3 3M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
        />
      </svg>
    {/snippet}
    {m.contextMenu.fileOrganize}
  </MenuItem>

  {#if canConvert}
    <MenuItem onSelect={handleConvert}>
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
            d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
          />
        </svg>
      {/snippet}
      {m.contextMenu.convert}
    </MenuItem>
  {/if}

  <MenuItem onSelect={handleLoudnessScan}>
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
          d="M9 19V6m6 13V10M3 19v-4m18 4v-7M6 19v-9m12 9V4"
        />
      </svg>
    {/snippet}
    {m.contextMenu.loudnessScan}
  </MenuItem>

  <MenuItem onSelect={handleAlbumArt}>
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
          d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
        />
      </svg>
    {/snippet}
    {m.contextMenu.albumArt}
  </MenuItem>

  <MenuItem onSelect={handleShowInFolder}>
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
          d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
        />
      </svg>
    {/snippet}
    {m.contextMenu.showInFolder}
  </MenuItem>

  {#if onRemoveFromList || onDelete}
    <MenuSeparator />
  {/if}

  {#if onRemoveFromList}
    <MenuItem
      onSelect={() => {
        onRemoveFromList();
        onClose();
      }}
    >
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
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      {/snippet}
      {removeFromListLabel}
    </MenuItem>
  {/if}

  {#if onDelete}
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
      {m.contextMenu.deleteEllipsis}
    </MenuItem>
  {/if}
</BaseContextMenu>
