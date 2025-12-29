<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import { useArtistsGroupedQuery, getAlbumArt } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize } from '$lib/stores/ui';
  import GroupDetail from './GroupDetail.svelte';

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const isLoading = $derived(artistsQuery.isLoading);
  const isError = $derived(artistsQuery.isError);
  const artists = $derived(artistsQuery.data ?? []);

  // 選択中のアーティスト（モーダル表示用）
  let selectedArtist = $state<ArtistGroup | null>(null);

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

  // アーティストカードが表示されたらアートを読み込み
  function handleArtistVisible(artist: ArtistGroup) {
    if (artist.representativeTrackId) {
      loadAlbumArt(artist.representativeTrackId);
    }
  }

  // アーティストをクリック
  function handleArtistClick(artist: ArtistGroup) {
    selectedArtist = artist;
  }

  // アーティストをダブルクリック（すべて再生）
  function handleArtistDoubleClick(artist: ArtistGroup) {
    const allTracks = artist.albums.flatMap((album) => album.tracks);
    if (allTracks.length > 0) {
      playTrackFromQueue(allTracks, 0);
    }
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, artist: ArtistGroup) {
    event.stopPropagation();
    handleArtistDoubleClick(artist);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedArtist = null;
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

<div class="artist-grid-container">
  {#if isLoading}
    <div class="loading-state">
      <div class="spinner"></div>
      <p>アーティストを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="error-state">
      <p>アーティストの読み込みに失敗しました</p>
    </div>
  {:else if artists.length > 0}
    <div class="artist-grid" style="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;">
      {#each artists as artist (artist.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="artist-card"
          onclick={() => handleArtistClick(artist)}
          ondblclick={() => handleArtistDoubleClick(artist)}
          use:intersectionObserver={{ callback: () => handleArtistVisible(artist) }}
        >
          <div class="artist-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
            {#if albumArtCache.has(artist.representativeTrackId)}
              <img src={albumArtCache.get(artist.representativeTrackId)} alt={artist.name} loading="lazy" />
            {:else}
              <div class="art-placeholder">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                  <circle cx="12" cy="7" r="4" />
                </svg>
              </div>
            {/if}
            <div class="play-overlay">
              <button class="play-button" onclick={(e) => handlePlayClick(e, artist)} title="アーティストを再生">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>
            </div>
          </div>
          <div class="artist-info">
            <h3 class="artist-name">{artist.name}</h3>
            <p class="artist-meta">{artist.albumCount}アルバム · {artist.trackCount}曲</p>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty-state">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
        <circle cx="12" cy="7" r="4" />
      </svg>
      <p>アーティストがいません</p>
      <span>音楽をインポートしてアーティストを追加してください</span>
    </div>
  {/if}
</div>

<!-- 詳細モーダル -->
<GroupDetail
  group={selectedArtist}
  type="artist"
  onClose={handleCloseDetail}
/>

<style>
  .artist-grid-container {
    padding: 1rem;
    min-height: 200px;
  }

  .artist-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width), 1fr));
    gap: 1.25rem;
  }

  .artist-card {
    background: rgba(255, 255, 255, 0.03);
    border-radius: 0.5rem;
    padding: 0.75rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .artist-card:hover {
    background: rgba(255, 255, 255, 0.08);
    transform: translateY(-2px);
  }

  .artist-art {
    position: relative;
    aspect-ratio: 1;
    border-radius: 50%;
    overflow: hidden;
    margin-bottom: 0.75rem;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .artist-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .art-placeholder {
    width: 100%;
    height: 100%;
    background: linear-gradient(135deg, #3a3a4a, #2a2a3a);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .art-placeholder svg {
    width: 40%;
    height: 40%;
    color: #555;
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
    border-radius: 50%;
  }

  .artist-card:hover .play-overlay {
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

  .artist-info {
    text-align: center;
    min-width: 0;
  }

  .artist-name {
    font-size: 0.9375rem;
    font-weight: 600;
    color: #fff;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artist-meta {
    font-size: 0.75rem;
    color: #666;
    margin: 0.375rem 0 0;
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
