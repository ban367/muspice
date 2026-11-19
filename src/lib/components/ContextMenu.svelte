<!--
  @component ContextMenu
  トラック用コンテキストメニュー。
  再生キューへの追加（次のキューに追加して再生・次のキューに追加・キューの末尾に追加・キューを置き換えて
  再生）、プレイリスト追加、お気に入りの切り替え、曲の情報の編集（別のウィンドウ。アルバムアートの変更も
  そこで行う）、タグの一括ツール、ファイルの整理、形式の変換、音量の解析、削除などのアクションを提供する。
-->
<script lang="ts">
  import { EMPTY_ID_SET } from '#lib/utils/collections.js';
  import type { Track } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuHeading,
    MenuItem,
    MenuSeparator,
    MenuSub,
    PlaylistSubmenu,
    QueueMenuItems
  } from '#lib/components/ui/index.js';
  import { albumOf, artistsOf, goToAlbum, goToArtist } from '#lib/utils/libraryReveal.js';
  import { useSetFavoriteMutation, useShowInFolderMutation } from '#lib/queries/tracks.js';
  import { useOpenTrackEditorMutation } from '#lib/queries/trackEditor.js';
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
    selectedTrackIds?: ReadonlySet<string>;
    onClose: () => void;
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
    selectedTrackIds = EMPTY_ID_SET,
    onClose,
    onDelete,
    onRemoveFromList,
    removeFromListLabel
  }: Props = $props();

  const showInFolderMutation = useShowInFolderMutation();
  // 形式の変換は、変換できる環境（macOS）でだけ項目を出す
  const convertFormatsQuery = useConvertFormatsQuery();
  const canConvert = $derived((convertFormatsQuery.data?.length ?? 0) > 0);
  const setFavoriteMutation = useSetFavoriteMutation();
  const openTrackEditorMutation = useOpenTrackEditorMutation();

  // 選択されたトラックの数
  const selectedCount = $derived(selectedTrackIds.size > 0 ? selectedTrackIds.size : 1);

  // 選択されたトラックのリスト
  const selectedTracks = $derived.by(() => {
    if (selectedTrackIds.size > 0 && tracks.length > 0) {
      return tracks.filter((t) => selectedTrackIds.has(t.id));
    }
    return [track];
  });

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
   * 選択した曲（一覧の順）の、曲の情報の編集画面を開く（別のウィンドウ。失敗はトースト通知する）
   */
  function handleEditMetadata() {
    openTrackEditorMutation.mutate(selectedTracks.map((selected) => selected.id));
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

  // 右クリックした曲のアルバムと、アーティストの画面でまとめるアーティスト（ADR-110）
  const hasAlbum = $derived(albumOf(track) !== null);
  const artists = $derived(artistsOf(track));

  /** 右クリックした曲の、アルバムの画面を開く */
  function handleGoToAlbum() {
    void goToAlbum(track);
    onClose();
  }

  /** そのアーティストの、アーティストの画面を開く */
  function handleGoToArtist(artist: string) {
    void goToArtist(artist);
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

<!-- 「アーティストへ移動」のアイコン（項目とサブメニューで使う） -->
{#snippet artistIcon()}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    class="menu-icon"
    fill="none"
    viewBox="0 0 24 24"
    stroke="currentColor"
    stroke-width="2"
  >
    <path stroke-linecap="round" stroke-linejoin="round" d="M20 21a8 8 0 0 0-16 0" />
    <circle cx="12" cy="8" r="4" />
  </svg>
{/snippet}

<BaseContextMenu {x} {y} {onClose}>
  {#if selectedCount > 1}
    <MenuHeading>{m.contextMenu.selectedCount(selectedCount)}</MenuHeading>
    <MenuSeparator />
  {/if}

  <QueueMenuItems tracks={selectedTracks} {onClose} />

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

  {#if hasAlbum || artists.length > 0}
    <MenuSeparator />
  {/if}

  {#if hasAlbum}
    <MenuItem onSelect={handleGoToAlbum}>
      {#snippet icon()}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="menu-icon"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="9" />
          <circle cx="12" cy="12" r="2.5" />
        </svg>
      {/snippet}
      {m.contextMenu.goToAlbum}
    </MenuItem>
  {/if}

  <!-- アーティストが複数ある曲（「A; B」）は、サブメニューで選ぶ -->
  {#if artists.length === 1}
    <MenuItem onSelect={() => handleGoToArtist(artists[0])} icon={artistIcon}>
      {m.contextMenu.goToArtist}
    </MenuItem>
  {:else if artists.length > 1}
    <MenuSub label={m.contextMenu.goToArtist} icon={artistIcon}>
      {#each artists as artist (artist)}
        <MenuItem onSelect={() => handleGoToArtist(artist)}>{artist}</MenuItem>
      {/each}
    </MenuSub>
  {/if}

  <MenuSeparator />

  <MenuItem onSelect={handleEditMetadata}>
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
