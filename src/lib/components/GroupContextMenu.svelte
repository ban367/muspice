<!--
  @component GroupContextMenu
  アルバム/アーティスト/ジャンルグループ用コンテキストメニュー。
  すべて再生、シャッフル再生、キュー操作、プレイリスト追加のアクションを提供する
  （アルバムでは、アルバムアートの変更も）。
  一覧の項目は曲を持たないため、メニューを開いた時にそのグループの曲を取得する。
-->
<script lang="ts">
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuItem,
    MenuSeparator,
    PlaylistSubmenu
  } from '#lib/components/ui/index.js';
  import { useGroupTracksQuery, type GroupType } from '#lib/queries/tracks.js';
  import {
    addNextInQueue,
    addToQueue,
    playTrackFromQueue,
    playShuffled
  } from '#lib/stores/player.svelte.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  type Group = AlbumSummary | ArtistSummary | GenreSummary;

  // Props
  interface Props {
    x: number;
    y: number;
    group: Group;
    type: GroupType;
    onClose: () => void;
  }

  let { x, y, group, type, onClose }: Props = $props();

  // グループ内のすべてのトラック（取得するまでは空。その間、操作は選べない）
  const groupName = $derived(group.name);
  // アルバムは、名前とアルバムをまとめたアーティストで決まる
  const albumArtist = $derived('artist' in group ? group.artist : null);
  const tracksQuery = $derived(useGroupTracksQuery(type, groupName, albumArtist));
  const allTracks = $derived(tracksQuery.data ?? []);
  const isEmpty = $derived(allTracks.length === 0);

  // タイプに応じたラベル
  const typeLabel = $derived.by(() => {
    switch (type) {
      case 'album':
        return m.fields.album;
      case 'artist':
        return m.fields.artist;
      case 'genre':
        return m.fields.genre;
    }
  });

  /**
   * すべて再生
   */
  function handlePlayAll() {
    if (allTracks.length > 0) {
      playTrackFromQueue(allTracks, 0);
    }
    onClose();
  }

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    playShuffled(allTracks);
    onClose();
  }

  /**
   * 次に再生（キューの先頭に追加）
   */
  function handlePlayNext() {
    addNextInQueue(allTracks);
    onClose();
  }

  /**
   * キューに追加（キューの最後に追加）
   */
  function handleAddToQueue() {
    addToQueue(allTracks);
    onClose();
  }

  /**
   * アルバムの全曲のアルバムアートの画面を開く
   */
  function handleAlbumArt() {
    albumArtDialog.open(allTracks);
    onClose();
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-title">{group.name}</div>
  <div class="menu-subtitle">{m.common.trackCount(group.trackCount)}</div>
  <MenuSeparator />

  <MenuItem onSelect={handlePlayAll} disabled={isEmpty}>
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
    {m.contextMenu.playGroup(typeLabel)}
  </MenuItem>

  <MenuItem onSelect={handleShufflePlay} disabled={isEmpty}>
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

  <MenuItem onSelect={handlePlayNext} disabled={isEmpty}>
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

  <MenuItem onSelect={handleAddToQueue} disabled={isEmpty}>
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

  <PlaylistSubmenu tracks={allTracks} {onClose} />

  {#if type === 'album'}
    <MenuSeparator />

    <MenuItem onSelect={handleAlbumArt} disabled={isEmpty}>
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
  {/if}
</BaseContextMenu>
