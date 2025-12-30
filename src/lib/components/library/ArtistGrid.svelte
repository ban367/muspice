<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import { useArtistsGroupedQuery, getAlbumArt } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { gridCardSize, browseSearchQuery } from '$lib/stores/ui';
  import GroupDetail from './GroupDetail.svelte';
  import GroupContextMenu from '../GroupContextMenu.svelte';

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const isLoading = $derived(artistsQuery.isLoading);
  const isError = $derived(artistsQuery.isError);
  const allArtists = $derived(artistsQuery.data ?? []);

  // 検索でフィルタリングされたアーティスト
  const artists = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return allArtists;
    return allArtists.filter(artist =>
      artist.name.toLowerCase().includes(query)
    );
  });

  // 選択中のアーティスト（モーダル表示用）
  let selectedArtist = $state<ArtistGroup | null>(null);

  // コンテキストメニュー
  let contextMenu = $state<{ x: number; y: number; artist: ArtistGroup } | null>(null);

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

  // 右クリックメニューを表示
  function handleContextMenu(event: MouseEvent, artist: ArtistGroup) {
    event.preventDefault();
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      artist
    };
  }

  // 右クリックメニューを閉じる
  function closeContextMenu() {
    contextMenu = null;
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
      <p>アーティストを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="state-container">
      <p class="text-error-light">アーティストの読み込みに失敗しました</p>
    </div>
  {:else if allArtists.length > 0 && artists.length === 0}
    <div class="state-container">
      <svg xmlns="http://www.w3.org/2000/svg" class="w-12 h-12 text-text-dimmed/50 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <p>「{$browseSearchQuery}」に一致するアーティストが見つかりません</p>
    </div>
  {:else if artists.length > 0}
    <div class="artist-grid" style="--card-width: {cardWidth}px; --art-size: {$gridCardSize}px;">
      {#each artists as artist (artist.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="grid-card"
          onclick={() => handleArtistClick(artist)}
          ondblclick={() => handleArtistDoubleClick(artist)}
          oncontextmenu={(e) => handleContextMenu(e, artist)}
          use:intersectionObserver={{ callback: () => handleArtistVisible(artist) }}
        >
          <div class="artist-art" style="width: {$gridCardSize}px; height: {$gridCardSize}px;">
            {#if albumArtCache.has(artist.representativeTrackId)}
              <img src={albumArtCache.get(artist.representativeTrackId)} alt={artist.name} loading="lazy" />
            {:else}
              <div class="art-placeholder artist-placeholder">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                  <circle cx="12" cy="7" r="4" />
                </svg>
              </div>
            {/if}
            <div class="play-overlay rounded-full">
              <button class="play-button-circle" onclick={(e) => handlePlayClick(e, artist)} title="アーティストを再生">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>
            </div>
          </div>
          <div class="text-center min-w-0">
            <h3 class="text-[0.9375rem] font-semibold text-text-primary m-0 text-truncate">{artist.name}</h3>
            <p class="text-xs text-text-dimmed mt-1.5 m-0">{artist.albumCount}アルバム · {artist.trackCount}曲</p>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="state-container">
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

<!-- コンテキストメニュー -->
{#if contextMenu}
  <GroupContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    group={contextMenu.artist}
    type="artist"
    onClose={closeContextMenu}
  />
{/if}

<style>
@reference "../../../app.css";
  .artist-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width), 1fr));
    gap: 1.25rem;
  }

  .artist-art {
    @apply relative aspect-square rounded-full overflow-hidden mb-3;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .artist-art img {
    @apply w-full h-full object-cover;
  }

  .artist-placeholder {
    @apply flex items-center justify-center;
    background: linear-gradient(135deg, #3a3a4a, #2a2a3a);
  }

  .artist-placeholder svg {
    @apply text-text-muted;
  }
</style>
