<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { useAlbumsGroupedQuery, getAlbumArt } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize } from '$lib/stores/ui';
  import GroupDetail from './GroupDetail.svelte';

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const isLoading = $derived(albumsQuery.isLoading);
  const isError = $derived(albumsQuery.isError);
  const albums = $derived(albumsQuery.data ?? []);

  // 選択中のアルバム（モーダル表示用）
  let selectedAlbum = $state<AlbumGroup | null>(null);

  // アルバムアートのキャッシュ
  let albumArtCache = $state<Map<string, string>>(new Map());
  let loadingArts = $state<Set<string>>(new Set());

  // カードサイズの計算
  const cardWidth = $derived($gridCardSize + 24); // padding分を追加

  // アルバムアートを読み込み
  async function loadAlbumArt(trackId: string) {
    if (loadingArts.has(trackId) || albumArtCache.has(trackId)) return;

    loadingArts.add(trackId);
    loadingArts = new Set(loadingArts);

    try {
      console.log('Loading album art for:', trackId);
      const art = await getAlbumArt(trackId);
      console.log('Album art result:', art ? 'found' : 'not found', art);
      if (art) {
        const dataUrl = `data:${art.mimeType};base64,${art.data}`;
        console.log('Data URL created, length:', dataUrl.length);
        albumArtCache.set(trackId, dataUrl);
        albumArtCache = new Map(albumArtCache);
        console.log('Cache updated, size:', albumArtCache.size);
      }
    } catch (err) {
      console.error('Error loading album art:', err);
    } finally {
      loadingArts.delete(trackId);
      loadingArts = new Set(loadingArts);
    }
  }

  // アルバムカードが表示されたらアートを読み込み
  function handleAlbumVisible(album: AlbumGroup) {
    console.log('Album visible:', album.name, 'trackId:', album.representativeTrackId);
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

  // Intersection Observer アクション
  function intersectionObserver(node: HTMLElement, options: { callback: () => void }) {
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          options.callback();
          observer.unobserve(node);
        }
      });
    }, { rootMargin: '100px' });

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
  {:else if albums.length > 0}
    <div class="album-grid" style="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;">
      {#each albums as album (album.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="grid-card"
          onclick={() => handleAlbumClick(album)}
          ondblclick={() => handleAlbumDoubleClick(album)}
          use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
        >
          <div class="grid-card-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
            {#if albumArtCache.has(album.representativeTrackId)}
              <img src={albumArtCache.get(album.representativeTrackId)} alt={album.name} loading="lazy" />
            {:else}
              <div class="art-placeholder">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <circle cx="12" cy="12" r="10" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              </div>
            {/if}
            <div class="play-overlay">
              <button class="play-button-circle" onclick={(e) => handlePlayClick(e, album)} title="アルバムを再生">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>
            </div>
          </div>
          <div class="min-w-0">
            <h3 class="text-[0.9375rem] font-semibold text-text-primary m-0 text-truncate">{album.name}</h3>
            <p class="text-[0.8125rem] text-text-muted mt-1 m-0 text-truncate">{album.artist || '不明なアーティスト'}</p>
            <p class="text-xs text-text-dimmed mt-1 m-0">{album.trackCount}曲</p>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="state-container">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <circle cx="12" cy="12" r="10" />
        <circle cx="12" cy="12" r="3" />
      </svg>
      <p>アルバムがありません</p>
      <span>音楽をインポートしてアルバムを追加してください</span>
    </div>
  {/if}
</div>

<!-- 詳細モーダル -->
<GroupDetail
  group={selectedAlbum}
  type="album"
  onClose={handleCloseDetail}
/>

<style>
  .album-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width), 1fr));
    gap: 1.25rem;
  }
</style>
