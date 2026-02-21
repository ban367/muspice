<!--
  @component GroupContextMenu
  アルバム/アーティスト/ジャンルグループ用コンテキストメニュー。
  すべて再生、シャッフル再生、キュー操作、プレイリスト追加のアクションを提供する。
-->
<script lang="ts">
  import type { Track, AlbumGroup, ArtistGroup, GenreGroup } from '$lib/types/models';
  import { BaseContextMenu, PlaylistSubmenu } from '$lib/components/ui';
  import { playTrackFromQueue, playQueue, currentTrackIndex } from '$lib/stores/player';
  import { get } from 'svelte/store';

  // グループタイプ
  type GroupType = 'album' | 'artist' | 'genre';
  type Group = AlbumGroup | ArtistGroup | GenreGroup;

  // Props
  interface Props {
    x: number;
    y: number;
    group: Group;
    type: GroupType;
    onClose: () => void;
  }

  let { x, y, group, type, onClose }: Props = $props();

  // グループ内のすべてのトラック
  const allTracks = $derived.by((): Track[] => {
    if (type === 'artist') {
      return (group as ArtistGroup).albums.flatMap((album) => album.tracks);
    } else {
      return (group as AlbumGroup | GenreGroup).tracks;
    }
  });

  // タイプに応じたラベル
  const typeLabel = $derived.by(() => {
    switch (type) {
      case 'album':
        return 'アルバム';
      case 'artist':
        return 'アーティスト';
      case 'genre':
        return 'ジャンル';
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
    if (allTracks.length > 0) {
      const shuffled = [...allTracks].sort(() => Math.random() - 0.5);
      playTrackFromQueue(shuffled, 0);
    }
    onClose();
  }

  /**
   * 次に再生（キューの先頭に追加）
   */
  function handlePlayNext() {
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);

    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...allTracks);
    playQueue.set(newQueue);
    onClose();
  }

  /**
   * キューに追加（キューの最後に追加）
   */
  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...allTracks]);
    onClose();
  }
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-header">{group.name}</div>
  <div class="menu-subheader">{allTracks.length}曲</div>
  <div class="menu-divider"></div>

  <button class="menu-item" onclick={handlePlayAll} role="menuitem">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="menu-icon"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M8 5v14l11-7z" />
    </svg>
    <span>{typeLabel}を再生</span>
  </button>

  <button class="menu-item" onclick={handleShufflePlay} role="menuitem">
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
    <span>シャッフル再生</span>
  </button>

  <div class="menu-divider"></div>

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
    <span>次に再生</span>
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
    <span>キューに追加</span>
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

  .menu-item:hover {
    @apply bg-surface-active;
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
