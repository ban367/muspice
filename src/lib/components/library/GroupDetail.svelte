<script lang="ts">
  import type { Track, AlbumGroup, ArtistGroup, GenreGroup } from '$lib/types/models';
  import { playTrackFromQueue, currentTrack } from '$lib/stores/player';
  import { getAlbumArt } from '$lib/queries/tracks';
  import PlayingIndicator from './PlayingIndicator.svelte';

  // Props
  interface Props {
    group: AlbumGroup | ArtistGroup | GenreGroup | null;
    type: 'album' | 'artist' | 'genre';
    onClose: () => void;
  }

  let { group, type, onClose }: Props = $props();

  // アルバムアートのキャッシュ
  let albumArtCache = $state<Map<string, string>>(new Map());
  let loadingArt = $state(false);

  // グループからトラックリストを取得
  const tracks = $derived.by((): Track[] => {
    if (!group) return [];
    if ('tracks' in group) {
      return group.tracks;
    }
    if ('albums' in group) {
      return group.albums.flatMap((album) => album.tracks);
    }
    return [];
  });

  // アルバムアートを読み込み
  $effect(() => {
    if (group && group.representativeTrackId && !albumArtCache.has(group.representativeTrackId)) {
      loadAlbumArt(group.representativeTrackId);
    }
  });

  async function loadAlbumArt(trackId: string) {
    if (loadingArt || albumArtCache.has(trackId)) return;
    loadingArt = true;
    try {
      const art = await getAlbumArt(trackId);
      if (art) {
        albumArtCache.set(trackId, `data:${art.mimeType};base64,${art.data}`);
        albumArtCache = new Map(albumArtCache);
      }
    } finally {
      loadingArt = false;
    }
  }

  // 再生時間のフォーマット
  function formatDuration(seconds: number | null): string {
    if (seconds === null) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  // 合計時間のフォーマット
  function formatTotalDuration(seconds: number): string {
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    if (hours > 0) {
      return `${hours}時間${mins}分`;
    }
    return `${mins}分`;
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(index: number) {
    playTrackFromQueue(tracks, index);
  }

  // すべて再生
  function handlePlayAll() {
    if (tracks.length > 0) {
      playTrackFromQueue(tracks, 0);
    }
  }

  // シャッフル再生
  function handleShufflePlay() {
    if (tracks.length > 0) {
      const shuffled = [...tracks].sort(() => Math.random() - 0.5);
      playTrackFromQueue(shuffled, 0);
    }
  }

  // モーダル外クリックで閉じる
  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      onClose();
    }
  }

  // キーボードイベント
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      onClose();
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if group}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-backdrop" onclick={handleBackdropClick}>
    <div class="modal-content">
      <!-- ヘッダー -->
      <div class="modal-header">
        <div class="header-art">
          {#if albumArtCache.has(group.representativeTrackId)}
            <img src={albumArtCache.get(group.representativeTrackId)} alt={group.name} />
          {:else}
            <div class="art-placeholder">
              {#if type === 'album'}
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <circle cx="12" cy="12" r="10" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              {:else if type === 'artist'}
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                  <circle cx="12" cy="7" r="4" />
                </svg>
              {:else}
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
                  <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
                </svg>
              {/if}
            </div>
          {/if}
        </div>
        <div class="header-info">
          <span class="header-type">
            {type === 'album' ? 'アルバム' : type === 'artist' ? 'アーティスト' : 'ジャンル'}
          </span>
          <h2 class="header-title">{group.name}</h2>
          {#if 'artist' in group && group.artist}
            <p class="header-artist">{group.artist}</p>
          {/if}
          <p class="header-meta">
            {group.trackCount}曲 · {formatTotalDuration(group.totalDuration)}
          </p>
          <div class="header-actions">
            <button class="action-button primary" onclick={handlePlayAll}>
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
              再生
            </button>
            <button class="action-button" onclick={handleShufflePlay}>
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="16 3 21 3 21 8" />
                <line x1="4" y1="20" x2="21" y2="3" />
                <polyline points="21 16 21 21 16 21" />
                <line x1="15" y1="15" x2="21" y2="21" />
                <line x1="4" y1="4" x2="9" y2="9" />
              </svg>
              シャッフル
            </button>
          </div>
        </div>
        <button class="close-button" onclick={onClose} title="閉じる">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <!-- トラックリスト -->
      <div class="track-list">
        {#each tracks as track, index (track.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="track-row"
            class:playing={$currentTrack?.id === track.id}
            ondblclick={() => handleTrackDoubleClick(index)}
          >
            <span class="track-number">
              {#if $currentTrack?.id === track.id}
                <PlayingIndicator size="small" />
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
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 2rem;
  }

  .modal-content {
    background: #1e1e2e;
    border-radius: 1rem;
    width: 100%;
    max-width: 800px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    gap: 1.5rem;
    padding: 1.5rem;
    background: linear-gradient(to bottom, rgba(59, 130, 246, 0.15), transparent);
    position: relative;
  }

  .header-art {
    width: 160px;
    height: 160px;
    flex-shrink: 0;
    border-radius: 0.5rem;
    overflow: hidden;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .header-art img {
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
    width: 48px;
    height: 48px;
    color: #666;
  }

  .header-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .header-type {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: #888;
    margin-bottom: 0.5rem;
  }

  .header-title {
    font-size: 1.75rem;
    font-weight: 700;
    color: #fff;
    margin: 0;
    line-height: 1.2;
  }

  .header-artist {
    font-size: 1rem;
    color: #aaa;
    margin: 0.5rem 0 0;
  }

  .header-meta {
    font-size: 0.875rem;
    color: #666;
    margin: 0.5rem 0 0;
  }

  .header-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
  }

  .action-button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 1.25rem;
    border: none;
    border-radius: 2rem;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .action-button:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .action-button.primary {
    background: #1db954;
    color: #000;
  }

  .action-button.primary:hover {
    background: #1ed760;
    transform: scale(1.02);
  }

  .action-button svg {
    width: 1rem;
    height: 1rem;
  }

  .close-button {
    position: absolute;
    top: 1rem;
    right: 1rem;
    width: 2rem;
    height: 2rem;
    border: none;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: #888;
    transition: all 0.15s ease;
  }

  .close-button:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
  }

  .close-button svg {
    width: 1rem;
    height: 1rem;
  }

  .track-list {
    flex: 1;
    overflow-y: auto;
    padding: 0 0.5rem 0.5rem;
  }

  .track-row {
    display: grid;
    grid-template-columns: 3rem 1fr 4rem;
    align-items: center;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem;
    border-radius: 0.375rem;
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
    font-size: 0.875rem;
    color: #666;
    text-align: center;
  }

  .track-row.playing .track-number {
    color: #1db954;
  }

  .track-info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .track-title {
    font-size: 0.9375rem;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-row.playing .track-title {
    color: #1db954;
  }

  .track-artist {
    font-size: 0.8125rem;
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-duration {
    font-size: 0.8125rem;
    color: #666;
    text-align: right;
  }
</style>
