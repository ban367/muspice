<script lang="ts">
  import { untrack } from 'svelte';
  import type { ArtistGroup, AlbumGroup } from '#lib/types/models.js';
  import { useSetRatingMutation } from '#lib/queries/tracks.js';
  import { player, playTrackFromQueue, playShuffled } from '#lib/stores/player.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    artist: ArtistGroup;
  }

  let { artist }: Props = $props();

  const setRatingMutation = useSetRatingMutation();

  // 表示順（アルバムごと）のトラック
  const displayTracks = $derived(artist.albums.flatMap((album) => album.tracks));

  // トラックの選択（クリック・キーボード。アルバムをまたいで選択できる）
  const selection = new TrackSelection(() => displayTracks);

  // 別のアーティストに切り替わったら、選択を消す（データを取り直しただけでは消さない）
  const artistName = $derived(artist.name);
  $effect(() => {
    void artistName;
    // 選択の中身には反応させない（選択を変えるたびに消えてしまう）
    untrack(() => selection.reset());
  });

  function handleTrackClick(trackId: string, event: MouseEvent) {
    selection.click(trackId, {
      shiftKey: event.shiftKey,
      toggleKey: event.ctrlKey || event.metaKey
    });
  }

  // 選択中の曲の上で始めた場合は選択中の曲すべて、そうでなければその曲だけを運ぶ
  function handleDragStart(event: DragEvent, trackId: string) {
    const trackIds = selection.beginDrag(trackId);
    startTrackDrag(event, trackIds, m.common.trackCount(trackIds.length));
  }

  /**
   * 一覧のキーボード操作（矢印キーで選択を移す・Enterでそのアルバムの曲として再生する）
   */
  function handleListKeydown(event: KeyboardEvent) {
    handleTrackListKeydown(event, selection, {
      onActivate: (trackId) => {
        for (const album of artist.albums) {
          const index = album.tracks.findIndex((track) => track.id === trackId);
          if (index !== -1) {
            handleTrackDoubleClick(album, index);
            return;
          }
        }
      }
    });
  }

  // すべて再生
  function handlePlayAll() {
    const allTracks = artist.albums.flatMap((album) => album.tracks);
    if (allTracks.length > 0) {
      playTrackFromQueue(allTracks, 0);
    }
  }

  // シャッフル再生
  function handleShufflePlay() {
    playShuffled(artist.albums.flatMap((album) => album.tracks));
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(album: AlbumGroup, index: number) {
    playTrackFromQueue(album.tracks, index);
  }

  // アルバム再生
  function handleAlbumPlay(album: AlbumGroup) {
    if (album.tracks.length > 0) {
      playTrackFromQueue(album.tracks, 0);
    }
  }
</script>

<div class="artist-detail">
  <!-- ヘッダー -->
  <div class="detail-header">
    <h1 class="artist-name">{artist.name}</h1>
    <div class="header-actions">
      <button class="action-btn play" onclick={handlePlayAll} title={m.common.playAll}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
      <button class="action-btn" onclick={handleShufflePlay} title={m.common.shuffle}>
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
      </button>
      <button class="action-btn" title={m.common.more}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="12" cy="5" r="2" />
          <circle cx="12" cy="12" r="2" />
          <circle cx="12" cy="19" r="2" />
        </svg>
      </button>
    </div>
  </div>

  <!-- アルバム一覧 -->
  <div
    class="albums-container"
    role="listbox"
    aria-multiselectable="true"
    aria-label={artist.name}
    tabindex="0"
    onkeydown={handleListKeydown}
  >
    {#each artist.albums as album (album.name)}
      <div class="album-section">
        <!-- アルバムヘッダー -->
        <div class="album-header">
          <div class="album-art">
            <AlbumArt
              src={albumArtUrl(album.representativeTrackId)}
              alt={album.name}
              rounded="sm"
            />
          </div>
          <div class="album-info">
            <MarqueeText text={album.name} class="album-title" />
            <span class="album-meta"
              >{album.tracks[0]?.genre || m.common.unknownGenre} · {album.tracks[0]?.year ||
                ''}</span
            >
          </div>
          <div class="album-actions">
            <button
              class="album-action-btn"
              onclick={() => handleAlbumPlay(album)}
              title={m.common.play}
            >
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            </button>
            <button class="album-action-btn" title={m.common.more}>
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <circle cx="12" cy="5" r="2" />
                <circle cx="12" cy="12" r="2" />
                <circle cx="12" cy="19" r="2" />
              </svg>
            </button>
          </div>
        </div>

        <!-- トラックリスト -->
        <div class="track-list">
          {#each album.tracks as track, index (track.id)}
            <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              class="track-row"
              class:selected={selection.has(track.id)}
              class:playing={player.currentTrack?.id === track.id}
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track.id)}
              onclick={(e) => handleTrackClick(track.id, e)}
              ondblclick={() => handleTrackDoubleClick(album, index)}
              role="option"
              aria-selected={selection.has(track.id)}
              data-track-id={track.id}
            >
              <span class="track-number" class:playing={player.currentTrack?.id === track.id}>
                {#if player.currentTrack?.id === track.id}
                  <PlayingIndicator size="small" />
                {:else}
                  {index + 1}
                {/if}
              </span>
              <div class="track-info">
                <MarqueeText text={track.title || track.fileName} class="track-title" />
              </div>
              <RatingStars
                rating={track.rating}
                onChange={(rating) => setRatingMutation.mutateAsync({ trackId: track.id, rating })}
              />
              <span class="track-duration">{formatDuration(track.duration)}</span>
              <button class="track-action-btn" title={m.common.more}>
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                  <circle cx="12" cy="5" r="2" />
                  <circle cx="12" cy="12" r="2" />
                  <circle cx="12" cy="19" r="2" />
                </svg>
              </button>
            </div>
          {/each}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  @reference "../../../app.css";

  .artist-detail {
    @apply flex flex-col h-full overflow-hidden;
  }

  .detail-header {
    @apply flex items-center justify-between p-6 border-b border-border shrink-0;
  }

  .artist-name {
    @apply text-3xl font-bold text-text-primary m-0;
  }

  .header-actions {
    @apply flex items-center gap-2;
  }

  .action-btn {
    @apply w-10 h-10 flex items-center justify-center border-none rounded-full cursor-pointer transition-all duration-150;
    @apply bg-surface-active;
    color: var(--color-text-secondary);
  }

  .action-btn:hover {
    @apply bg-surface-strong;
    @apply text-text-primary;
  }

  .action-btn.play {
    @apply bg-secondary text-black;
  }

  .action-btn.play:hover {
    @apply bg-secondary-focus;
  }

  .action-btn svg {
    @apply w-5 h-5;
  }

  /* 一覧はフォーカスを受けてキー操作を扱う（枠は出さず、選択中の行の色で示す） */
  .albums-container {
    @apply flex-1 overflow-y-auto outline-none;
  }

  .album-section {
    @apply py-4;
  }

  .album-section:not(:last-child) {
    @apply border-b border-border;
  }

  .album-header {
    @apply flex items-center gap-4 px-6 mb-2;
  }

  .album-art {
    @apply w-24 h-24 rounded-lg overflow-hidden shrink-0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .album-info {
    @apply flex-1 flex flex-col gap-1 min-w-0;
  }

  :global(.album-title) {
    @apply text-lg font-semibold text-text-primary;
  }

  .album-meta {
    @apply text-sm text-text-muted;
  }

  .album-actions {
    @apply flex items-center gap-1;
  }

  .album-action-btn {
    @apply w-8 h-8 flex items-center justify-center border-none bg-transparent rounded-full cursor-pointer text-text-muted transition-all duration-150;
  }

  .album-action-btn:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .album-action-btn svg {
    @apply w-4 h-4;
  }

  .track-list {
    @apply px-4;
  }

  .track-row {
    @apply grid gap-3 py-2 px-2 items-center rounded-md cursor-pointer transition-colors duration-100 select-none;
    grid-template-columns: 2rem 1fr auto 3rem 2rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
  }

  .track-row.selected {
    @apply bg-primary/20;
  }

  .track-row.playing {
    @apply bg-secondary/15;
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
    @apply text-sm text-text-primary;
  }

  .track-row.playing :global(.track-title) {
    @apply text-secondary;
  }

  .track-duration {
    @apply text-sm text-text-dimmed text-right;
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
