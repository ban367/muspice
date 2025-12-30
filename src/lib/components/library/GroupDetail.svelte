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
    <div class="modal-content max-w-3xl">
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
          <span class="text-xs uppercase tracking-wider text-text-muted mb-2">
            {type === 'album' ? 'アルバム' : type === 'artist' ? 'アーティスト' : 'ジャンル'}
          </span>
          <h2 class="text-2xl font-bold text-text-primary m-0 leading-tight">{group.name}</h2>
          {#if 'artist' in group && group.artist}
            <p class="text-base text-text-secondary mt-2 m-0">{group.artist}</p>
          {/if}
          <p class="text-sm text-text-dimmed mt-2 m-0">
            {group.trackCount}曲 · {formatTotalDuration(group.totalDuration)}
          </p>
          <div class="flex gap-3 mt-4">
            <button class="action-button primary" onclick={handlePlayAll}>
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-4 h-4">
                <path d="M8 5v14l11-7z" />
              </svg>
              再生
            </button>
            <button class="action-button" onclick={handleShufflePlay}>
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4">
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
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4">
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
@reference "../../../app.css";
  .modal-header {
    @apply flex gap-6 p-6 relative;
    background: linear-gradient(to bottom, rgba(59, 130, 246, 0.15), transparent);
  }

  .header-art {
    @apply w-40 h-40 shrink-0 rounded-lg overflow-hidden;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .header-art img {
    @apply w-full h-full object-cover;
  }

  .header-info {
    @apply flex-1 flex flex-col justify-center;
  }

  .action-button {
    @apply flex items-center gap-2 py-2.5 px-5 border-none rounded-full text-sm font-medium cursor-pointer transition-all duration-150;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .action-button:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .action-button.primary {
    @apply bg-secondary text-black;
  }

  .action-button.primary:hover {
    @apply bg-secondary-focus scale-[1.02];
  }

  .close-button {
    @apply absolute top-4 right-4 w-8 h-8 border-none rounded-full flex items-center justify-center cursor-pointer text-text-muted transition-all duration-150;
    background: rgba(255, 255, 255, 0.1);
  }

  .close-button:hover {
    background: rgba(255, 255, 255, 0.15);
    @apply text-text-primary;
  }

  .track-list {
    @apply flex-1 overflow-y-auto px-2 pb-2;
  }

  .track-row {
    @apply grid gap-3 py-2.5 px-3 items-center rounded-md cursor-pointer transition-colors duration-100;
    grid-template-columns: 3rem 1fr 4rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
  }

  .track-row.playing {
    background: rgba(29, 185, 84, 0.15);
  }

  .track-number {
    @apply text-sm text-text-dimmed text-center;
  }

  .track-row.playing .track-number {
    @apply text-secondary;
  }

  .track-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .track-title {
    @apply text-[0.9375rem] text-text-primary text-truncate;
  }

  .track-row.playing .track-title {
    @apply text-secondary;
  }

  .track-artist {
    @apply text-[0.8125rem] text-text-dimmed text-truncate;
  }

  .track-duration {
    @apply text-[0.8125rem] text-text-dimmed text-right;
  }
</style>
