<script lang="ts">
  import type { AlbumGroup, Track } from '$lib/types/models';
  import { playTrackFromQueue, currentTrack } from '$lib/stores/player';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import { formatDuration, formatTotalDuration } from '$lib/utils/format';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

  // Props
  interface Props {
    album: AlbumGroup;
  }

  let { album }: Props = $props();

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // アルバムアートを読み込み
  $effect(() => {
    if (album && album.representativeTrackId) {
      loadAlbumArt(album.representativeTrackId);
    }
  });

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }

  // 総再生時間
  const totalDuration = $derived(
    album.tracks.reduce((sum, track) => sum + (track.duration || 0), 0)
  );

  // マルチディスクアルバムかどうか
  const hasMultipleDiscs = $derived(() => {
    const discNumbers = new Set(album.tracks.map((t) => t.discNumber ?? 1));
    return discNumbers.size > 1;
  });

  // ディスクごとにグループ化されたトラック
  const groupedTracks = $derived.by(() => {
    if (!hasMultipleDiscs()) {
      return [{ discNumber: 1, tracks: album.tracks }];
    }

    const groups = new Map<number, Track[]>();
    for (const track of album.tracks) {
      const disc = track.discNumber ?? 1;
      if (!groups.has(disc)) {
        groups.set(disc, []);
      }
      groups.get(disc)!.push(track);
    }

    return Array.from(groups.entries())
      .sort((a, b) => a[0] - b[0])
      .map(([discNumber, tracks]) => ({ discNumber, tracks }));
  });

  // トラック番号を取得
  function formatTrackNumber(track: Track): string {
    return (track.trackNumber ?? 0).toString();
  }

  // すべて再生
  function handlePlayAll() {
    if (album.tracks.length > 0) {
      playTrackFromQueue(album.tracks, 0);
    }
  }

  // シャッフル再生
  function handleShufflePlay() {
    if (album.tracks.length > 0) {
      const shuffled = [...album.tracks].sort(() => Math.random() - 0.5);
      playTrackFromQueue(shuffled, 0);
    }
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(index: number) {
    playTrackFromQueue(album.tracks, index);
  }

  // グループ内のトラックインデックスを取得（全体のインデックス用）
  function getGlobalTrackIndex(discIndex: number, trackIndexInDisc: number): number {
    let index = 0;
    for (let i = 0; i < discIndex; i++) {
      index += groupedTracks[i].tracks.length;
    }
    return index + trackIndexInDisc;
  }
</script>

<div class="album-detail">
  <!-- ヘッダー -->
  <div class="detail-header">
    <div class="album-art">
      <AlbumArt src={getArt(album.representativeTrackId)} alt={album.name} rounded="lg" />
    </div>
    <div class="album-info">
      <h1 class="album-name">{album.name}</h1>
      <p class="album-artist">{album.artist || '不明なアーティスト'}</p>
      <p class="album-meta">
        {album.tracks[0]?.genre || ''}{album.tracks[0]?.genre && album.tracks[0]?.year
          ? ' · '
          : ''}{album.tracks[0]?.year || ''}
      </p>
      <p class="album-stats">{album.trackCount}曲 · {formatTotalDuration(totalDuration)}</p>
      <div class="header-actions">
        <button class="action-btn play" onclick={handlePlayAll} title="すべて再生">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
          再生
        </button>
        <button class="action-btn" onclick={handleShufflePlay} title="シャッフル">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="16 3 21 3 21 8" />
            <line x1="4" y1="20" x2="21" y2="3" />
            <polyline points="21 16 21 21 16 21" />
            <line x1="15" y1="15" x2="21" y2="21" />
            <line x1="4" y1="4" x2="9" y2="9" />
          </svg>
          シャッフル
        </button>
        <button class="action-btn icon-only" title="その他">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="12" cy="5" r="2" />
            <circle cx="12" cy="12" r="2" />
            <circle cx="12" cy="19" r="2" />
          </svg>
        </button>
      </div>
    </div>
  </div>

  <!-- トラックリスト -->
  <div class="track-list">
    {#each groupedTracks as discGroup, discIndex (discGroup.discNumber)}
      {#if hasMultipleDiscs()}
        <div class="disc-header">
          <span class="disc-label">Disc {discGroup.discNumber}</span>
        </div>
      {/if}
      {#each discGroup.tracks as track, trackIndexInDisc (track.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="track-row"
          class:playing={$currentTrack?.id === track.id}
          ondblclick={() =>
            handleTrackDoubleClick(getGlobalTrackIndex(discIndex, trackIndexInDisc))}
        >
          <span class="track-number" class:playing={$currentTrack?.id === track.id}>
            {#if $currentTrack?.id === track.id}
              <PlayingIndicator size="small" />
            {:else}
              {formatTrackNumber(track)}
            {/if}
          </span>
          <div class="track-info">
            <MarqueeText text={track.title || track.fileName} class="track-title" />
            <span class="track-artist">{track.artist || album.artist || '不明なアーティスト'}</span>
          </div>
          <span class="track-duration">{formatDuration(track.duration)}</span>
          <button class="track-action-btn" title="その他">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <circle cx="12" cy="5" r="2" />
              <circle cx="12" cy="12" r="2" />
              <circle cx="12" cy="19" r="2" />
            </svg>
          </button>
        </div>
      {/each}
    {/each}
  </div>
</div>

<style>
  @reference "../../../app.css";

  .album-detail {
    @apply flex flex-col h-full overflow-hidden;
  }

  .detail-header {
    @apply flex gap-6 p-6 shrink-0;
    background: linear-gradient(to bottom, rgba(59, 130, 246, 0.15), transparent);
  }

  .album-art {
    @apply w-48 h-48 rounded-lg overflow-hidden shrink-0;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .album-info {
    @apply flex flex-col justify-center;
  }

  .album-name {
    @apply text-2xl font-bold text-text-primary m-0 leading-tight;
  }

  .album-artist {
    @apply text-lg text-text-secondary mt-1 m-0;
  }

  .album-meta {
    @apply text-sm text-text-muted mt-1 m-0;
  }

  .album-stats {
    @apply text-sm text-text-dimmed mt-1 m-0;
  }

  .header-actions {
    @apply flex items-center gap-3 mt-4;
  }

  .action-btn {
    @apply flex items-center gap-2 py-2.5 px-5 border-none rounded-full text-sm font-medium cursor-pointer transition-all duration-150;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .action-btn.play {
    @apply bg-secondary text-black;
  }

  .action-btn.play:hover {
    @apply bg-secondary-focus scale-[1.02];
  }

  .action-btn.icon-only {
    @apply w-10 h-10 p-0 justify-center;
    background: rgba(255, 255, 255, 0.1);
  }

  .action-btn svg {
    @apply w-5 h-5;
  }

  .track-list {
    @apply flex-1 overflow-y-auto px-4 pb-4;
  }

  .disc-header {
    @apply flex items-center gap-3 py-3 px-3 mt-2 first:mt-0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .disc-label {
    @apply text-sm font-semibold text-text-secondary uppercase tracking-wider;
  }

  .track-row {
    @apply grid gap-3 py-1.5 px-3 items-center rounded-md cursor-pointer transition-colors duration-100;
    grid-template-columns: 2.5rem 1fr 4rem 2rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
  }

  .track-row.playing {
    background: rgba(29, 185, 84, 0.15);
  }

  .track-number {
    @apply text-sm text-text-dimmed text-center flex items-center gap-1;
  }

  .track-number.playing {
    @apply text-secondary font-medium;
  }

  .track-row.playing .track-number {
    @apply text-secondary;
  }

  .track-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  :global(.track-title) {
    @apply text-[0.9375rem] text-text-primary;
  }

  .track-row.playing :global(.track-title) {
    @apply text-secondary;
  }

  .track-artist {
    @apply text-[0.8125rem] text-text-dimmed truncate;
  }

  .track-duration {
    @apply text-[0.8125rem] text-text-dimmed text-right;
  }

  .track-action-btn {
    @apply w-6 h-6 flex items-center justify-center border-none bg-transparent rounded-full cursor-pointer text-text-muted opacity-0 transition-all duration-150;
  }

  .track-row:hover .track-action-btn {
    @apply opacity-100;
  }

  .track-action-btn:hover {
    @apply bg-surface-active text-text-primary;
  }

  .track-action-btn svg {
    @apply w-4 h-4;
  }
</style>
