<!--
  @component GroupContextMenu
  アルバム/アーティスト/ジャンルグループ用コンテキストメニュー。
  すべて再生、シャッフル再生、キュー操作、プレイリスト追加のアクションを提供する。
  一覧の項目は曲を持たないため、メニューを開いた時にそのグループの曲を取得する。
-->
<script lang="ts">
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import { BaseContextMenu, PlaylistSubmenu } from '#lib/components/ui/index.js';
  import { useGroupTracksQuery, type GroupType } from '#lib/queries/tracks.js';
  import {
    addNextInQueue,
    addToQueue,
    playTrackFromQueue,
    playShuffled
  } from '#lib/stores/player.svelte.js';
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
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-header">{group.name}</div>
  <div class="menu-subheader">{m.common.trackCount(group.trackCount)}</div>
  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handlePlayAll} role="menuitem" disabled={isEmpty}>
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>{m.contextMenu.playGroup(typeLabel)}</span>
  </button>

  <button class="menu-item" onclick={handleShufflePlay} role="menuitem" disabled={isEmpty}>
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

  <button class="menu-item" onclick={handlePlayNext} role="menuitem" disabled={isEmpty}>
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

  <button class="menu-item" onclick={handleAddToQueue} role="menuitem" disabled={isEmpty}>
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

  <PlaylistSubmenu tracks={allTracks} {onClose} />
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

  .menu-icon {
    @apply w-4 h-4 shrink-0;
  }

  .menu-item span {
    @apply flex-1;
  }

  .menu-divider {
    @apply h-px bg-border my-2;
  }
</style>
