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
      const art = await getAlbumArt(trackId);
      if (art) {
        albumArtCache.set(trackId, `data:${art.mimeType};base64,${art.data}`);
        albumArtCache = new Map(albumArtCache);
      }
    } finally {
      loadingArts.delete(trackId);
      loadingArts = new Set(loadingArts);
    }
  }

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

<div class="album-grid-container">
  {#if isLoading}
    <div class="loading-state">
      <div class="spinner"></div>
      <p>アルバムを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="error-state">
      <p>アルバムの読み込みに失敗しました</p>
    </div>
  {:else if albums.length > 0}
    <div class="album-grid" style="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;">
      {#each albums as album (album.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="album-card"
          onclick={() => handleAlbumClick(album)}
          ondblclick={() => handleAlbumDoubleClick(album)}
          use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
        >
          <div class="album-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
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
              <button class="play-button" onclick={(e) => handlePlayClick(e, album)} title="アルバムを再生">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>
            </div>
          </div>
          <div class="album-info">
            <h3 class="album-name">{album.name}</h3>
            <p class="album-artist">{album.artist || '不明なアーティスト'}</p>
            <p class="album-meta">{album.trackCount}曲</p>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty-state">
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
  .album-grid-container {
    padding: 1rem;
    min-height: 200px;
  }

  .album-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width), 1fr));
    gap: 1.25rem;
  }

  .album-card {
    background: rgba(255, 255, 255, 0.03);
    border-radius: 0.5rem;
    padding: 0.75rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .album-card:hover {
    background: rgba(255, 255, 255, 0.08);
    transform: translateY(-2px);
  }

  .album-art {
    position: relative;
    aspect-ratio: 1;
    border-radius: 0.375rem;
    overflow: hidden;
    margin-bottom: 0.75rem;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .album-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .art-placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(135deg, #2a2a3a, #1a1a2a);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .art-placeholder svg {
    width: 40%;
    height: 40%;
    color: #444;
  }

  .play-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.2s ease;
  }

  .album-card:hover .play-overlay {
    opacity: 1;
  }

  .play-button {
    width: 48px;
    height: 48px;
    border: none;
    border-radius: 50%;
    background: #1db954;
    color: #000;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: transform 0.15s ease;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .play-button:hover {
    transform: scale(1.08);
  }

  .play-button svg {
    width: 24px;
    height: 24px;
    margin-left: 2px;
  }

  .album-info {
    min-width: 0;
  }

  .album-name {
    font-size: 0.9375rem;
    font-weight: 600;
    color: #fff;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .album-artist {
    font-size: 0.8125rem;
    color: #888;
    margin: 0.25rem 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .album-meta {
    font-size: 0.75rem;
    color: #666;
    margin: 0.25rem 0 0;
  }

  /* 状態表示 */
  .loading-state,
  .error-state,
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 4rem 2rem;
    color: #666;
    text-align: center;
  }

  .loading-state .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: #3b82f6;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    margin-bottom: 1rem;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .empty-state svg {
    width: 64px;
    height: 64px;
    color: #444;
    margin-bottom: 1rem;
  }

  .empty-state p {
    font-size: 1rem;
    color: #888;
    margin: 0;
  }

  .empty-state span {
    font-size: 0.875rem;
    color: #666;
    margin-top: 0.5rem;
  }
</style>
