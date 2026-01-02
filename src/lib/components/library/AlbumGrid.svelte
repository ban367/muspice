<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { useAlbumsGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize, browseSearchQuery } from '$lib/stores/ui';
  import {
    loadAlbumArt,
    getCachedAlbumArt,
    isCached,
    albumArtCacheVersion
  } from '$lib/stores/albumArtCache';
  import { formatDuration } from '$lib/utils/format';
  import GroupDetail from './GroupDetail.svelte';
  import GroupContextMenu from '../GroupContextMenu.svelte';
  import MarqueeText from '../MarqueeText.svelte';

  // Props
  interface Props {
    displayMode?: 'grid' | 'list';
  }

  let { displayMode = 'grid' }: Props = $props();

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const isLoading = $derived(albumsQuery.isLoading);
  const isError = $derived(albumsQuery.isError);
  const allAlbums = $derived(albumsQuery.data ?? []);

  // 検索でフィルタリングされたアルバム
  const albums = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return allAlbums;
    return allAlbums.filter(
      (album) =>
        album.name.toLowerCase().includes(query) ||
        (album.artist && album.artist.toLowerCase().includes(query))
    );
  });

  // 選択中のアルバム（モーダル表示用）
  let selectedAlbum = $state<AlbumGroup | null>(null);

  // コンテキストメニュー
  let contextMenu = $state<{ x: number; y: number; album: AlbumGroup } | null>(null);

  // キャッシュ更新の追跡（リアクティビティのため）
  // eslint-disable-next-line no-unused-vars
  const cacheVersion = $derived($albumArtCacheVersion);

  // カードサイズの計算
  const cardWidth = $derived($gridCardSize + 24); // padding分を追加

  // アルバムカードが表示されたらアートを読み込み
  function handleAlbumVisible(album: AlbumGroup) {
    if (album.representativeTrackId) {
      loadAlbumArt(album.representativeTrackId);
    }
  }

  // アルバムをクリック
  function handleAlbumClick(album: AlbumGroup) {
    selectedAlbum = album;
  }

  // アルバムをダブルクリック（すべて再生）
  function handleAlbumDoubleClick(album: AlbumGroup) {
    if (album.tracks.length > 0) {
      playTrackFromQueue(album.tracks, 0);
    }
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, album: AlbumGroup) {
    event.stopPropagation();
    handleAlbumDoubleClick(album);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedAlbum = null;
  }

  // 右クリックメニューを表示
  function handleContextMenu(event: MouseEvent, album: AlbumGroup) {
    event.preventDefault();
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      album
    };
  }

  // 右クリックメニューを閉じる
  function closeContextMenu() {
    contextMenu = null;
  }

  // アルバムの総再生時間を計算
  function getTotalDuration(album: AlbumGroup): number {
    return album.tracks.reduce((sum, track) => sum + (track.duration || 0), 0);
  }

  // Intersection Observer アクション
  function intersectionObserver(node: HTMLElement, options: { callback: () => void }) {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            options.callback();
            observer.unobserve(node);
          }
        });
      },
      { rootMargin: '100px' }
    );

    observer.observe(node);

    return {
      destroy() {
        observer.disconnect();
      }
    };
  }
</script>

<div class="p-4 min-h-[200px]">
  {#if isLoading}
    <div class="state-container">
      <div class="spinner"></div>
      <p>アルバムを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="state-container">
      <p class="text-error-light">アルバムの読み込みに失敗しました</p>
    </div>
  {:else if allAlbums.length > 0 && albums.length === 0}
    <div class="state-container">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-12 h-12 text-text-dimmed/50 mb-4"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
        />
      </svg>
      <p>「{$browseSearchQuery}」に一致するアルバムが見つかりません</p>
    </div>
  {:else if albums.length > 0}
    {#if displayMode === 'grid'}
      <!-- グリッド表示 -->
      <div class="album-grid" style="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;">
        {#each albums as album (album.name)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="grid-card"
            onclick={() => handleAlbumClick(album)}
            ondblclick={() => handleAlbumDoubleClick(album)}
            oncontextmenu={(e) => handleContextMenu(e, album)}
            use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
          >
            <div class="grid-card-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
              {#if isCached(album.representativeTrackId)}
                <img
                  src={getCachedAlbumArt(album.representativeTrackId)}
                  alt={album.name}
                  loading="lazy"
                />
              {:else}
                <div class="art-placeholder">
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                  >
                    <circle cx="12" cy="12" r="10" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                </div>
              {/if}
              <div class="play-overlay">
                <button
                  class="play-button-circle"
                  onclick={(e) => handlePlayClick(e, album)}
                  title="アルバムを再生"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                </button>
              </div>
            </div>
            <div class="min-w-0 w-full">
              <MarqueeText
                text={album.name}
                class="text-[0.9375rem] font-semibold text-text-primary m-0"
              />
              <MarqueeText
                text={album.artist || '不明なアーティスト'}
                class="text-[0.8125rem] text-text-muted mt-1 m-0"
              />
              <p class="text-xs text-text-dimmed mt-1 m-0">{album.trackCount}曲</p>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <!-- リスト表示 -->
      <div class="album-list">
        {#each albums as album (album.name)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="list-row"
            onclick={() => handleAlbumClick(album)}
            ondblclick={() => handleAlbumDoubleClick(album)}
            oncontextmenu={(e) => handleContextMenu(e, album)}
            use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
          >
            <div class="list-art">
              {#if isCached(album.representativeTrackId)}
                <img
                  src={getCachedAlbumArt(album.representativeTrackId)}
                  alt={album.name}
                  loading="lazy"
                />
              {:else}
                <div class="art-placeholder small">
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                  >
                    <circle cx="12" cy="12" r="10" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                </div>
              {/if}
            </div>
            <div class="list-info">
              <MarqueeText text={album.name} class="list-title" />
              <MarqueeText text={album.artist || '不明なアーティスト'} class="list-artist" />
            </div>
            <div class="list-meta">
              <span>{album.trackCount}曲</span>
            </div>
            <div class="list-duration">
              {formatDuration(getTotalDuration(album))}
            </div>
            <button
              class="list-play-btn"
              onclick={(e) => handlePlayClick(e, album)}
              title="アルバムを再生"
            >
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            </button>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="state-container">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
      >
        <circle cx="12" cy="12" r="10" />
        <circle cx="12" cy="12" r="3" />
      </svg>
      <p>アルバムがありません</p>
      <span>音楽をインポートしてアルバムを追加してください</span>
    </div>
  {/if}
</div>

<!-- 詳細モーダル -->
<GroupDetail group={selectedAlbum} type="album" onClose={handleCloseDetail} />

<!-- コンテキストメニュー -->
{#if contextMenu}
  <GroupContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    group={contextMenu.album}
    type="album"
    onClose={closeContextMenu}
  />
{/if}

<style>
  @reference "../../../app.css";
  .album-grid {
    @apply grid grid-cols-[repeat(auto-fill,minmax(var(--card-width),1fr))] gap-5;
  }

  /* リスト表示スタイル */
  .album-list {
    @apply flex flex-col;
  }

  .list-row {
    @apply grid gap-3 px-3 py-2.5 items-center rounded-md cursor-pointer transition-colors duration-100
           grid-cols-[3rem_1fr_5rem_4rem_2.5rem];
  }

  .list-row:hover {
    @apply bg-surface-hover;
  }

  .list-art {
    @apply w-12 h-12 rounded overflow-hidden shrink-0 flex items-center justify-center;
  }

  .list-art img {
    @apply w-full h-full object-cover;
  }

  .art-placeholder.small {
    @apply w-full h-full;
  }

  .art-placeholder.small svg {
    @apply w-6 h-6;
  }

  .list-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .list-title {
    @apply text-sm font-medium text-text-primary truncate;
  }

  .list-artist {
    @apply text-xs text-text-muted truncate;
  }

  .list-meta {
    @apply text-xs text-text-dimmed text-right;
  }

  .list-duration {
    @apply text-xs text-text-dimmed text-right;
  }

  .list-play-btn {
    @apply w-8 h-8 flex items-center justify-center bg-transparent border-none rounded-full text-text-muted cursor-pointer transition-all duration-150 opacity-0;
  }

  .list-row:hover .list-play-btn {
    @apply opacity-100;
  }

  .list-play-btn:hover {
    @apply bg-primary text-primary-content;
  }

  .list-play-btn svg {
    @apply w-4 h-4;
  }
</style>
