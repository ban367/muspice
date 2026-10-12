<script lang="ts">
  import { untrack } from 'svelte';
  import type { AlbumSummary, Track } from '#lib/types/models.js';
  import { useAlbumTracksQuery, useSetRatingMutation } from '#lib/queries/tracks.js';
  import { player, playTrackFromQueue, playShuffled } from '#lib/stores/player.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import FavoriteButton from './FavoriteButton.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    album: AlbumSummary;
  }

  let { album }: Props = $props();

  const setRatingMutation = useSetRatingMutation();

  // アルバムの曲（ディスク番号・トラック番号の順。表示・再生・選択はこの順で行う）
  const albumName = $derived(album.name);
  const albumArtist = $derived(album.artist);
  const tracksQuery = $derived(useAlbumTracksQuery(albumName, albumArtist));
  const tracks = $derived(tracksQuery.data ?? []);

  // マルチディスクアルバムかどうか
  const hasMultipleDiscs = $derived(new Set(tracks.map((t) => t.discNumber ?? 1)).size > 1);

  // ディスクごとにグループ化されたトラック
  const groupedTracks = $derived.by(() => {
    if (!hasMultipleDiscs) {
      return [{ discNumber: 1, tracks }];
    }

    const discNumbers = [...new Set(tracks.map((t) => t.discNumber ?? 1))].sort((a, b) => a - b);

    return discNumbers.map((discNumber) => ({
      discNumber,
      tracks: tracks.filter((t) => (t.discNumber ?? 1) === discNumber)
    }));
  });

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => tracks);

  // 別のアルバムに切り替わったら、選択を消す（データを取り直しただけでは消さない）
  $effect(() => {
    void albumName;
    void albumArtist;
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
   * 一覧のキーボード操作（矢印キーで選択を移す・Enterで再生する）
   */
  function handleListKeydown(event: KeyboardEvent) {
    handleTrackListKeydown(event, selection, {
      onActivate: (trackId) => {
        const index = tracks.findIndex((track) => track.id === trackId);
        if (index !== -1) handleTrackDoubleClick(index);
      }
    });
  }

  // トラック番号を取得
  function formatTrackNumber(track: Track): string {
    return (track.trackNumber ?? 0).toString();
  }

  // すべて再生
  function handlePlayAll() {
    if (tracks.length > 0) {
      playTrackFromQueue(tracks, 0);
    }
  }

  // シャッフル再生
  function handleShufflePlay() {
    playShuffled(tracks);
  }

  // トラックをダブルクリックで再生
  function handleTrackDoubleClick(index: number) {
    playTrackFromQueue(tracks, index);
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
      <AlbumArt src={albumArtUrl(album.representativeTrackId)} alt={album.name} rounded="lg" />
    </div>
    <div class="album-info">
      <h1 class="album-name">{album.name}</h1>
      <p class="album-artist">{album.artist || m.common.unknownArtist}</p>
      <p class="album-meta">
        {tracks[0]?.genre || ''}{tracks[0]?.genre && tracks[0]?.year ? ' · ' : ''}{tracks[0]
          ?.year || ''}
      </p>
      <p class="album-stats">
        {m.common.trackCountAndDuration(album.trackCount, formatTotalDuration(album.totalDuration))}
      </p>
      <div class="header-actions">
        <button class="action-btn play" onclick={handlePlayAll} title={m.common.playAll}>
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
          {m.common.play}
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
          {m.common.shuffle}
        </button>
        <button class="action-btn icon-only" title={m.common.more}>
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
  <div
    class="track-list"
    role="listbox"
    aria-multiselectable="true"
    aria-label={album.name}
    tabindex="0"
    onkeydown={handleListKeydown}
  >
    {#each groupedTracks as discGroup, discIndex (discGroup.discNumber)}
      {#if hasMultipleDiscs}
        <div class="disc-header">
          <span class="disc-label">Disc {discGroup.discNumber}</span>
        </div>
      {/if}
      {#each discGroup.tracks as track, trackIndexInDisc (track.id)}
        <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
        <div
          class="track-row"
          class:selected={selection.has(track.id)}
          class:missing={track.isMissing}
          title={track.isMissing ? m.common.fileMissing : undefined}
          class:playing={player.currentTrack?.id === track.id}
          draggable="true"
          ondragstart={(e) => handleDragStart(e, track.id)}
          onclick={(e) => handleTrackClick(track.id, e)}
          ondblclick={() =>
            handleTrackDoubleClick(getGlobalTrackIndex(discIndex, trackIndexInDisc))}
          role="option"
          aria-selected={selection.has(track.id)}
          data-track-id={track.id}
        >
          <span class="track-number" class:playing={player.currentTrack?.id === track.id}>
            {#if player.currentTrack?.id === track.id}
              <PlayingIndicator size="small" />
            {:else}
              {formatTrackNumber(track)}
            {/if}
          </span>
          <div class="track-info">
            <MarqueeText text={track.title || track.fileName} class="track-title" />
            <!-- 曲のアーティストは、アルバムのアーティストと違う場合だけ表示する（コンピレーションなど） -->
            {#if track.artist && track.artist !== album.artist}
              <span class="track-artist">{track.artist}</span>
            {/if}
          </div>
          <div class="flex items-center gap-2">
            <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
            <RatingStars
              rating={track.rating}
              onChange={(rating) => setRatingMutation.mutateAsync({ trackId: track.id, rating })}
            />
          </div>
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
    /* アクセントカラーを薄くした色から透明へ */
    background: linear-gradient(
      to bottom,
      color-mix(in oklab, var(--color-primary) 15%, transparent),
      transparent
    );
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
    @apply bg-surface-active text-text-primary;
  }

  .action-btn:hover {
    @apply bg-surface-strong;
  }

  .action-btn.play {
    @apply bg-secondary text-black;
  }

  .action-btn.play:hover {
    @apply bg-secondary-focus scale-[1.02];
  }

  .action-btn.icon-only {
    @apply w-10 h-10 p-0 justify-center;
    @apply bg-surface-active;
  }

  .action-btn svg {
    @apply w-5 h-5;
  }

  /* 一覧はフォーカスを受けてキー操作を扱う（枠は出さず、選択中の行の色で示す） */
  .track-list {
    @apply flex-1 overflow-y-auto px-4 pb-4 outline-none;
  }

  .disc-header {
    @apply flex items-center gap-3 py-3 px-3 mt-2 first:mt-0;
    @apply border-b border-border;
  }

  .disc-label {
    @apply text-sm font-semibold text-text-secondary uppercase tracking-wider;
  }

  .track-row {
    @apply grid gap-3 py-1.5 px-3 items-center rounded-md cursor-pointer transition-colors duration-100 select-none;
    grid-template-columns: 2.5rem 1fr auto 4rem 2rem;
  }

  .track-row:hover {
    @apply bg-surface-hover;
  }

  /* ファイルが見つからない曲（再生できない）は薄く表示する */
  .track-row.missing {
    @apply opacity-50;
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
