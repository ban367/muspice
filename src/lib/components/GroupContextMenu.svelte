<!--
  @component GroupContextMenu
  アルバム/アーティスト/ジャンルグループ用コンテキストメニュー。
  再生キューへの追加（次のキューに追加して再生・次のキューに追加・キューの末尾に追加・キューを置き換えて
  再生）、シャッフル再生、プレイリスト追加のアクションを提供する
  （アルバムでは、アルバムアートの変更も）。
  一覧の項目は曲を持たないため、メニューを開いた時にそのグループの曲を取得する。
-->
<script lang="ts">
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import {
    BaseContextMenu,
    MenuItem,
    MenuSeparator,
    PlaylistSubmenu,
    QueueMenuItems
  } from '#lib/components/ui/index.js';
  import { useGroupTracksQuery, type GroupType } from '#lib/queries/tracks.js';
  import { playShuffled } from '#lib/stores/player.svelte.js';
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

  /**
   * シャッフル再生
   */
  function handleShufflePlay() {
    playShuffled(allTracks);
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

  <QueueMenuItems tracks={allTracks} disabled={isEmpty} {onClose} />

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
