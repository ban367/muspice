<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { useAlbumsGroupedQuery, getAlbumArt } from '$lib/queries/tracks';
  import { playTrackFromQueue, currentTrack, isPlaying } from '$lib/stores/player';

  // クエリ
  const albumsQuery = useAlbumsGroupedQuery();
  const isLoading = $derived(albumsQuery.isLoading);
  const isError = $derived(albumsQuery.isError);
  const albums = $derived(albumsQuery.data ?? []);

  // 展開中のアルバム名
  let expandedAlbum = $state<string | null>(null);

  // アルバムアートのキャッシュ
  let albumArtCache = $state<Map<string, string>>(new Map());
  let loadingArts = $state<Set<string>>(new Set());

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

  // アルバムをクリック（アコーディオン開閉）
  function handleAlbumClick(album: AlbumGroup) {
    if (expandedAlbum === album.name) {
      expandedAlbum = null;
    } else {
      expandedAlbum = album.name;
    }
  }

  // アルバムの全曲を再生
  function handlePlayAll(event: MouseEvent, album: AlbumGroup) {
    event.stopPropagation();
    if (album.tracks.length > 0) {
      playTrackFromQueue(album.tracks, 0);
    }
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(album: AlbumGroup, index: number) {
    playTrackFromQueue(album.tracks, index);
  }

  // 再生時間のフォーマット
  function formatDuration(seconds: number | null): string {
    if (seconds === null) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
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
    <div class="album-list">
      {#each albums as album (album.name)}
        <div
          class="album-item"
          class:expanded={expandedAlbum === album.name}
          use:intersectionObserver={{ callback: () => handleAlbumVisible(album) }}
        >
          <!-- アルバムヘッダー -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="album-header" onclick={() => handleAlbumClick(album)}>
            <div class="album-art">
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
            </div>
            <div class="album-info">
              <h3 class="album-name">{album.name}</h3>
              <p class="album-artist">{album.artist || '不明なアーティスト'}</p>
              <p class="album-meta">{album.trackCount}曲</p>
            </div>
            <div class="album-actions">
              <button class="play-button" onclick={(e) => handlePlayAll(e, album)} title="アルバムを再生">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>
              <span class="expand-icon" class:rotated={expandedAlbum === album.name}>
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </span>
            </div>
          </div>

          <!-- 展開時のトラックリスト -->
          {#if expandedAlbum === album.name}
            <div class="track-list">
              {#each album.tracks as track, index (track.id)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  class="track-row"
                  class:playing={$currentTrack?.id === track.id}
                  ondblclick={() => handleTrackDoubleClick(album, index)}
                >
                  <span class="track-number">
                    {#if $currentTrack?.id === track.id}
                      <span class="playing-indicator" class:animating={$isPlaying}>
                        <span class="bar"></span>
                        <span class="bar"></span>
                        <span class="bar"></span>
                      </span>
                    {:else}
                      {index + 1}
                    {/if}
                  </span>
                  <div class="track-info">
                    <span class="track-title">{track.title || track.fileName}</span>
                    <span class="track-artist">{track.artist || '不明なアーティスト'}</span>
                  </div>
                  <span class="track-duration">{formatDuration(track.duration)}</span>
                </div>
              {/each}
            </div>
          {/if}
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

<style>
  .album-grid-container {
    padding: 1rem;
    min-height: 200px;
  }

  .album-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .album-item {
    background: rgba(255, 255, 255, 0.03);
    border-radius: 0.5rem;
    overflow: hidden;
    transition: background 0.2s ease;
  }

  .album-item:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .album-item.expanded {
    background: rgba(255, 255, 255, 0.05);
  }

  .album-header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .album-header:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .album-art {
    width: 56px;
    height: 56px;
    flex-shrink: 0;
    border-radius: 0.375rem;
    overflow: hidden;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
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
    width: 50%;
    height: 50%;
    color: #444;
  }

  .album-info {
    flex: 1;
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
    margin: 0.125rem 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .album-meta {
    font-size: 0.75rem;
    color: #666;
    margin: 0.125rem 0 0;
  }

  .album-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .play-button {
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 50%;
    background: #1db954;
    color: #000;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: transform 0.15s ease, opacity 0.15s ease;
    opacity: 0;
  }

  .album-header:hover .play-button {
    opacity: 1;
  }

  .play-button:hover {
    transform: scale(1.08);
  }

  .play-button svg {
    width: 18px;
    height: 18px;
    margin-left: 2px;
  }

  .expand-icon {
    width: 24px;
    height: 24px;
    color: #666;
    transition: transform 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .expand-icon.rotated {
    transform: rotate(180deg);
  }

  .expand-icon svg {
    width: 18px;
    height: 18px;
  }

  /* トラックリスト */
  .track-list {
    padding: 0 0.5rem 0.5rem;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    animation: slideDown 0.2s ease;
  }

  @keyframes slideDown {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .track-row {
    display: grid;
    grid-template-columns: 2.5rem 1fr 4rem;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.25rem;
    cursor: pointer;
    transition: background 0.1s ease;
  }

  .track-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .track-row.playing {
    background: rgba(29, 185, 84, 0.15);
  }

  .track-number {
    font-size: 0.8125rem;
    color: #666;
    text-align: center;
  }

  .track-row.playing .track-number {
    color: #1db954;
  }

  .playing-indicator {
    display: flex;
    gap: 2px;
    justify-content: center;
    align-items: flex-end;
    height: 1rem;
  }

  .playing-indicator .bar {
    width: 3px;
    background: #1db954;
    border-radius: 1px;
  }

  .playing-indicator .bar:nth-child(1) { height: 60%; }
  .playing-indicator .bar:nth-child(2) { height: 100%; }
  .playing-indicator .bar:nth-child(3) { height: 40%; }

  /* アニメーションは再生中のみ */
  .playing-indicator.animating .bar {
    animation: equalize 0.8s ease infinite;
  }

  .playing-indicator.animating .bar:nth-child(1) { animation-delay: 0s; }
  .playing-indicator.animating .bar:nth-child(2) { animation-delay: 0.2s; }
  .playing-indicator.animating .bar:nth-child(3) { animation-delay: 0.4s; }

  @keyframes equalize {
    0%, 100% { transform: scaleY(0.3); }
    50% { transform: scaleY(1); }
  }

  .track-info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .track-title {
    font-size: 0.875rem;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-row.playing .track-title {
    color: #1db954;
  }

  .track-artist {
    font-size: 0.75rem;
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-duration {
    font-size: 0.75rem;
    color: #666;
    text-align: right;
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
