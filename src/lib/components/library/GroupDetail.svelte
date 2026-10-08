<script lang="ts">
  import { untrack } from 'svelte';
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import {
    useGroupTracksQuery,
    useSetRatingMutation,
    type GroupType
  } from '#lib/queries/tracks.js';
  import { player, playTrackFromQueue, playShuffled } from '#lib/stores/player.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    group: AlbumSummary | ArtistSummary | GenreSummary | null;
    type: GroupType;
    onClose: () => void;
  }

  let { group, type, onClose }: Props = $props();

  const setRatingMutation = useSetRatingMutation();

  // 開いているグループの曲（一覧の項目は曲を持たないため、開いた時に取得する）
  const groupName = $derived(group?.name ?? null);
  const tracksQuery = $derived(groupName === null ? null : useGroupTracksQuery(type, groupName));
  const tracks = $derived(tracksQuery?.data ?? []);

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => tracks);

  // 別のグループを開いたら（閉じた時も）、選択を消す（データを取り直しただけでは消さない）
  $effect(() => {
    void groupName;
    // 選択の中身には反応させない（選択を変えるたびに消えてしまう）
    untrack(() => selection.reset());
  });

  function handleTrackClick(trackId: string, event: MouseEvent) {
    selection.click(trackId, {
      shiftKey: event.shiftKey,
      toggleKey: event.ctrlKey || event.metaKey
    });
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
    playShuffled(tracks);
  }
</script>

<Modal
  open={group !== null}
  {onClose}
  label={group?.name}
  class="max-w-3xl"
  bodyClass="flex flex-col flex-1 min-h-0"
>
  {#if group}
    <!-- ヘッダー -->
    <div class="modal-header">
      <div class="header-art">
        <AlbumArt
          src={albumArtUrl(group.representativeTrackId)}
          alt={group.name}
          rounded="lg"
          placeholderType={type === 'artist' ? 'person' : 'disc'}
        />
      </div>
      <div class="header-info">
        <span class="text-xs uppercase tracking-wider text-text-muted mb-2">
          {type === 'album' ? m.fields.album : type === 'artist' ? m.fields.artist : m.fields.genre}
        </span>
        <h2 class="text-2xl font-bold text-text-primary m-0 leading-tight">{group.name}</h2>
        {#if 'artist' in group && group.artist}
          <p class="text-base text-text-secondary mt-2 m-0">{group.artist}</p>
        {/if}
        <p class="text-sm text-text-dimmed mt-2 m-0">
          {m.common.trackCountAndDuration(
            group.trackCount,
            formatTotalDuration(group.totalDuration)
          )}
        </p>
        <div class="flex gap-3 mt-4">
          <button class="action-button primary" onclick={handlePlayAll}>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 24 24"
              fill="currentColor"
              class="w-4 h-4"
            >
              <path d="M8 5v14l11-7z" />
            </svg>
            {m.common.play}
          </button>
          <button class="action-button" onclick={handleShufflePlay}>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              class="w-4 h-4"
            >
              <polyline points="16 3 21 3 21 8" />
              <line x1="4" y1="20" x2="21" y2="3" />
              <polyline points="21 16 21 21 16 21" />
              <line x1="15" y1="15" x2="21" y2="21" />
              <line x1="4" y1="4" x2="9" y2="9" />
            </svg>
            {m.common.shuffle}
          </button>
        </div>
      </div>
      <button class="close-button" onclick={onClose} title={m.common.close}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          class="w-4 h-4"
        >
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- トラックリスト -->
    <div
      class="track-list"
      role="listbox"
      aria-multiselectable="true"
      aria-label={group.name}
      tabindex="0"
      onkeydown={handleListKeydown}
    >
      {#each tracks as track, index (track.id)}
        <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
        <div
          class="track-row"
          class:selected={selection.has(track.id)}
          class:playing={player.currentTrack?.id === track.id}
          onclick={(e) => handleTrackClick(track.id, e)}
          ondblclick={() => handleTrackDoubleClick(index)}
          role="option"
          aria-selected={selection.has(track.id)}
          data-track-id={track.id}
        >
          <span class="track-number">
            {#if player.currentTrack?.id === track.id}
              <PlayingIndicator size="small" />
            {:else}
              {index + 1}
            {/if}
          </span>
          <div class="track-info">
            <span class="track-title">{track.title || track.fileName}</span>
            <span class="track-artist">{track.artist || m.common.unknownArtist}</span>
          </div>
          <RatingStars
            rating={track.rating}
            onChange={(rating) => setRatingMutation.mutateAsync({ trackId: track.id, rating })}
          />
          <span class="track-duration">{formatDuration(track.duration)}</span>
        </div>
      {/each}
    </div>
  {/if}
</Modal>

<style>
  @reference "../../../app.css";
  .modal-header {
    @apply flex gap-6 p-6 relative;
    /* アクセントカラーを薄くした色から透明へ */
    background: linear-gradient(
      to bottom,
      color-mix(in oklab, var(--color-primary) 15%, transparent),
      transparent
    );
  }

  .header-art {
    @apply w-40 h-40 shrink-0 rounded-lg overflow-hidden;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .header-info {
    @apply flex-1 flex flex-col justify-center;
  }

  .action-button {
    @apply flex items-center gap-2 py-2.5 px-5 border-none rounded-full text-sm font-medium cursor-pointer transition-all duration-150;
    @apply bg-surface-active text-text-primary;
  }

  .action-button:hover {
    @apply bg-surface-strong;
  }

  .action-button.primary {
    @apply bg-secondary text-black;
  }

  .action-button.primary:hover {
    @apply bg-secondary-focus scale-[1.02];
  }

  .close-button {
    @apply absolute top-4 right-4 w-8 h-8 border-none rounded-full flex items-center justify-center cursor-pointer text-text-muted transition-all duration-150;
    @apply bg-surface-active;
  }

  .close-button:hover {
    @apply bg-surface-strong;
    @apply text-text-primary;
  }

  /* 一覧はフォーカスを受けてキー操作を扱う（枠は出さず、選択中の行の色で示す） */
  .track-list {
    @apply flex-1 overflow-y-auto px-2 pb-2 outline-none;
  }

  .track-row {
    @apply grid gap-3 py-2.5 px-3 items-center rounded-md cursor-pointer transition-colors duration-100 select-none;
    grid-template-columns: 3rem 1fr auto 4rem;
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
    @apply text-sm text-text-dimmed text-center;
  }

  .track-row.playing .track-number {
    @apply text-secondary;
  }

  .track-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .track-title {
    @apply text-[0.9375rem] text-text-primary overflow-hidden text-ellipsis whitespace-nowrap;
  }

  .track-row.playing .track-title {
    @apply text-secondary;
  }

  .track-artist {
    @apply text-[0.8125rem] text-text-dimmed overflow-hidden text-ellipsis whitespace-nowrap;
  }

  .track-duration {
    @apply text-[0.8125rem] text-text-dimmed text-right;
  }
</style>
