<!--
  @component GroupContextMenu
  アルバム/アーティスト/ジャンルグループ用コンテキストメニュー。
  再生キューへの追加（次のキューに追加して再生・次のキューに追加・キューの末尾に追加・キューを置き換えて
  再生）、プレイリスト追加のアクションを提供する
  （アルバムでは、アルバムの曲の情報の編集（別のウィンドウ。アルバムアートの変更もそこで行う）も）。
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
  import { useOpenTrackEditorMutation } from '#lib/queries/trackEditor.js';
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

  const openTrackEditorMutation = useOpenTrackEditorMutation();

  /**
   * アルバムの全曲を、曲の情報の編集画面でまとめて編集する（別のウィンドウ）
   */
  function handleEditTracks() {
    openTrackEditorMutation.mutate({ trackIds: allTracks.map((track) => track.id) });
    onClose();
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-title">{group.name}</div>
  <div class="menu-subtitle">{m.common.trackCount(group.trackCount)}</div>
  <MenuSeparator />

  <QueueMenuItems tracks={allTracks} disabled={isEmpty} {onClose} />

  <MenuSeparator />

  <PlaylistSubmenu tracks={allTracks} {onClose} />

  {#if type === 'album'}
    <MenuSeparator />

    <MenuItem onSelect={handleEditTracks} disabled={isEmpty}>
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
</BaseContextMenu>
