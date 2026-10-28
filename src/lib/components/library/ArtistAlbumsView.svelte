<!--
  @component ArtistAlbumsView
  アーティスト（アルバムアーティスト）のアルバムごとの曲。アーティストとアルバムアーティストの画面の
  リスト表示の右のペインで使う。

  - 見出しにアーティストの名前と、すべて再生・シャッフルのボタンを出す
  - アルバムごとに見出し（ジャケット画像・名前・ジャンルと年・曲数と時間）を出し、その下に曲を並べる
    （ディスクが複数あるアルバムは、ディスクで区切る）
  - アルバムの見出しをダブルクリックすると、アルバムを再生する。右クリックでアルバムのメニューを開く
  - 曲の選択はアルバムをまたいで行える。最後に選んだ曲を、右サイドバーの曲情報パネルに出す
-->
<script lang="ts">
  import type { AlbumGroup, ArtistSummary, Track } from '#lib/types/models.js';
  import { flattenAlbumTracks, useSetRatingMutation } from '#lib/queries/tracks.js';
  import { player, playNow, playShuffled } from '#lib/stores/player.svelte.js';
  import { followSelectedTrack } from '#lib/stores/ui.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { albumGroupSummary } from '#lib/utils/albumArtists.js';
  import { groupByDisc } from '#lib/utils/albumDiscs.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import PlayingIndicator from './PlayingIndicator.svelte';
  import RatingStars from './RatingStars.svelte';
  import FavoriteButton from './FavoriteButton.svelte';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import ContextMenu from '../ContextMenu.svelte';
  import GroupContextMenu from '../GroupContextMenu.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    /** 見出しの名前 */
    title: string;
    /** 表示するアルバムと曲（表示順） */
    albums: AlbumGroup[];
    /**
     * 一覧をまとめたアーティスト（アーティストのないアルバムの一覧はnull）
     *
     * アルバムをまとめたアーティスト・曲のアーティストは、これと違う場合だけ出す（ほかのアーティストとの
     * 曲のアルバム・コンピレーション・フィーチャリングなど）。
     */
    artist: string | null;
    /** 見出しの「その他」のメニューで扱うアーティスト（なければ、ボタンを出さない） */
    menuArtist?: ArtistSummary;
  }

  let { title, albums, artist, menuArtist }: Props = $props();

  const setRatingMutation = useSetRatingMutation();

  // 表示順（アルバムごと）のトラック（アルバムの曲はディスクの順に並んでいるため、ディスクで区切っても同じ順）
  const displayTracks = $derived(flattenAlbumTracks(albums));

  // アルバムごとの、ディスクで区切った曲（ディスクが1枚だけのアルバムは、見出しを出さない）
  const sections = $derived(
    albums.map((album) => {
      const discs = groupByDisc(album.tracks);
      return { album, discs, hasMultipleDiscs: discs.length > 1 };
    })
  );

  // トラックの選択（クリック・キーボード。アルバムをまたいで選択できる）
  const selection = new TrackSelection(() => displayTracks);

  // 選んでいる曲を、曲情報パネルに出す（数は、表示している曲のうち選んでいる曲）
  followSelectedTrack(() => {
    const track = selection.lastSelected;
    if (track === null) return null;
    return { track, count: displayTracks.filter((t) => selection.has(t.id)).length };
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
        const track = displayTracks.find((candidate) => candidate.id === trackId);
        if (track) playTrack(track);
      }
    });
  }

  // すべて再生（再生中の曲の次に入れて再生する）
  function handlePlayAll() {
    playNow(displayTracks);
  }

  // シャッフル再生
  function handleShufflePlay() {
    playShuffled(displayTracks);
  }

  // その曲を、再生中の曲の次に入れて再生する（ダブルクリック・Enter）
  function playTrack(track: Track) {
    playNow([track]);
  }

  // アルバムを、再生中の曲の次に入れて再生する（アルバムの見出しのダブルクリック・再生のボタン）
  function handleAlbumPlay(album: AlbumGroup) {
    playNow(album.tracks);
  }

  // ---- メニュー（行の右クリック・「その他」のボタン） ----

  /** 開いている曲のメニュー */
  let trackMenu = $state<{ x: number; y: number; track: Track } | null>(null);
  /** 開いているアーティスト・アルバムのメニュー（`album`がnullなら、アーティスト） */
  let groupMenu = $state<{ x: number; y: number; album: AlbumGroup | null } | null>(null);

  /** メニューを開く位置（右クリックはその位置、ボタンはボタンの下） */
  function menuPosition(event: MouseEvent): { x: number; y: number } {
    if (event.type === 'contextmenu') return { x: event.clientX, y: event.clientY };
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: rect.left, y: rect.bottom + 4 };
  }

  /** 曲のメニューを開く（選択中の曲の上なら、選択中の曲すべてが対象） */
  function openTrackMenu(event: MouseEvent, track: Track) {
    event.preventDefault();
    // 行のクリック（選択の変更）にしない
    event.stopPropagation();
    selection.ensureSelected(track.id);
    trackMenu = { ...menuPosition(event), track };
  }

  /** アルバムのメニューを開く（見出しの右クリック・「その他」のボタン） */
  function openAlbumMenu(event: MouseEvent, album: AlbumGroup) {
    event.preventDefault();
    groupMenu = { ...menuPosition(event), album };
  }
</script>

<div class="artist-albums">
  <!-- ヘッダー -->
  <div class="detail-header">
    <h1 class="artist-name">{title}</h1>
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
      {#if menuArtist}
        <button
          class="action-btn"
          title={m.common.more}
          aria-label={m.common.more}
          aria-haspopup="menu"
          onclick={(event) => (groupMenu = { ...menuPosition(event), album: null })}
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="12" cy="5" r="2" />
            <circle cx="12" cy="12" r="2" />
            <circle cx="12" cy="19" r="2" />
          </svg>
        </button>
      {/if}
    </div>
  </div>

  <!-- アルバム一覧 -->
  <div
    class="albums-container"
    role="listbox"
    aria-multiselectable="true"
    aria-label={title}
    tabindex="0"
    onkeydown={handleListKeydown}
  >
    <!-- 同じ名前のアルバムが複数あることがある（ほかのアーティストとの曲のアルバムなど）ため、代表の曲で見分ける -->
    {#each sections as { album, discs, hasMultipleDiscs } (album.representativeTrackId)}
      <div class="album-section">
        <!-- アルバムヘッダー -->
        <div class="album-header">
          <!-- 再生とメニューは、横のボタンからも使える -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="album-summary"
            ondblclick={() => handleAlbumPlay(album)}
            oncontextmenu={(e) => openAlbumMenu(e, album)}
          >
            <div class="album-art">
              <AlbumArt
                src={albumArtUrl(album.representativeTrackId)}
                alt={album.name}
                rounded="sm"
              />
            </div>
            <div class="album-info">
              <MarqueeText text={album.name} class="album-title" />
              {#if album.artist !== artist}
                <!-- ほかのアーティストとの曲のアルバム: アルバムをまとめるアーティストを出す -->
                <span class="album-meta">{album.artist || m.common.unknownArtist}</span>
              {/if}
              <span class="album-meta"
                >{[album.tracks[0]?.genre || m.common.unknownGenre, album.tracks[0]?.year]
                  .filter(Boolean)
                  .join(' · ')}</span
              >
              <span class="album-stats"
                >{m.common.trackCountAndDuration(
                  album.trackCount,
                  formatTotalDuration(album.totalDuration)
                )}</span
              >
            </div>
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
            <button
              class="album-action-btn"
              title={m.common.more}
              aria-label={m.common.more}
              aria-haspopup="menu"
              onclick={(event) => openAlbumMenu(event, album)}
            >
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
          {#each discs as disc (disc.discNumber)}
            {#if hasMultipleDiscs}
              <div class="disc-header">{m.common.disc(disc.discNumber)}</div>
            {/if}
            {#each disc.tracks as track (track.id)}
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
                oncontextmenu={(e) => openTrackMenu(e, track)}
                ondblclick={() => playTrack(track)}
                role="option"
                aria-selected={selection.has(track.id)}
                data-track-id={track.id}
              >
                <span class="track-number" class:playing={player.currentTrack?.id === track.id}>
                  {#if player.currentTrack?.id === track.id}
                    <PlayingIndicator size="small" />
                  {:else}
                    {track.trackNumber ?? ''}
                  {/if}
                </span>
                <div class="track-info">
                  <MarqueeText text={track.title || track.fileName} class="track-title" />
                  <!-- 曲のアーティストは、一覧のアーティストと違う場合だけ表示する（フィーチャリング・コンピレーションなど） -->
                  {#if track.artist && track.artist !== artist}
                    <span class="track-artist">{track.artist}</span>
                  {/if}
                </div>
                <div class="row-end">
                  <!-- 幅が狭い時は、ポインタを置いた時・中のボタンにフォーカスがある時だけ、時間の代わりに出す -->
                  <div class="row-actions">
                    <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
                    <RatingStars
                      rating={track.rating}
                      onChange={(rating) =>
                        setRatingMutation.mutateAsync({ trackId: track.id, rating })}
                    />
                  </div>
                  <span class="track-duration">{formatDuration(track.duration)}</span>
                </div>
                <button
                  class="track-action-btn"
                  title={m.common.more}
                  aria-label={m.common.more}
                  aria-haspopup="menu"
                  onclick={(e) => openTrackMenu(e, track)}
                  ondblclick={(e) => e.stopPropagation()}
                >
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
    {/each}
  </div>
</div>

{#if trackMenu}
  <ContextMenu
    x={trackMenu.x}
    y={trackMenu.y}
    track={trackMenu.track}
    tracks={displayTracks}
    selectedTrackIds={selection.ids}
    onClose={() => (trackMenu = null)}
  />
{/if}

{#if groupMenu}
  {#if groupMenu.album}
    <GroupContextMenu
      x={groupMenu.x}
      y={groupMenu.y}
      group={albumGroupSummary(groupMenu.album)}
      type="album"
      onClose={() => (groupMenu = null)}
    />
  {:else if menuArtist}
    <GroupContextMenu
      x={groupMenu.x}
      y={groupMenu.y}
      group={menuArtist}
      type="artist"
      onClose={() => (groupMenu = null)}
    />
  {/if}
{/if}

<style>
  @reference "../../../app.css";

  /* 行の幅に合わせて、お気に入りと評価の出し方を変える（下の`@container`） */
  .artist-albums {
    @apply flex flex-col h-full overflow-hidden;
    container-type: inline-size;
  }

  .detail-header {
    @apply flex items-center justify-between gap-4 p-6 border-b border-border shrink-0;
  }

  .artist-name {
    @apply text-3xl font-bold text-text-primary m-0 truncate;
  }

  .header-actions {
    @apply flex items-center gap-2 shrink-0;
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
    @apply flex items-center gap-2 px-4 mb-2;
  }

  /* ジャケット画像と名前 */
  .album-summary {
    @apply flex-1 flex items-center gap-4 min-w-0 p-2;
  }

  .album-art {
    @apply w-24 h-24 rounded-lg overflow-hidden shrink-0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .album-info {
    @apply flex-1 flex flex-col gap-1 min-w-0;
  }

  /* アルバム名（`MarqueeText`に渡したクラス）。ほかの部品の同じ名前のクラスに当たらないよう、見出しの中に限る */
  .album-info :global(.album-title) {
    @apply text-lg font-semibold text-text-primary;
  }

  .album-meta {
    @apply text-sm text-text-muted truncate;
  }

  .album-stats {
    @apply text-sm text-text-dimmed;
  }

  .album-actions {
    @apply flex items-center gap-1 shrink-0;
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

  .disc-header {
    @apply py-2 px-2 mt-2 first:mt-0 text-xs font-semibold text-text-secondary uppercase tracking-wider;
    @apply border-b border-border;
  }

  .track-row {
    @apply grid gap-3 py-2 px-2 items-center rounded-md cursor-pointer transition-colors duration-100 select-none;
    grid-template-columns: 2rem 1fr auto 2rem;
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
    @apply text-sm text-text-dimmed text-center flex items-center justify-center gap-1;
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

  /* 曲名（`MarqueeText`に渡したクラス）。ほかの部品の同じ名前のクラスに当たらないよう、行の中に限る */
  .track-row :global(.track-title) {
    @apply text-sm text-text-primary;
  }

  .track-row.playing :global(.track-title) {
    @apply text-secondary;
  }

  .track-artist {
    @apply text-xs text-text-dimmed truncate;
  }

  .row-end {
    @apply flex items-center gap-3;
  }

  .row-actions {
    @apply flex items-center gap-2;
  }

  .track-duration {
    @apply w-12 text-sm text-text-dimmed text-right tabular-nums;
  }

  /* 幅が狭い時（情報パネルを開いて本文が狭くなった時など）は、曲名を読めるよう、お気に入りと評価を
     ポインタを置いた時だけ時間の代わりに出す（情報パネルの行と同じ） */
  @container (max-width: 40rem) {
    .row-end {
      @apply gap-0;
    }

    /* 幅を0にして隠す（display: noneと違い、中のボタンへTabでフォーカスを移せる） */
    .row-actions {
      @apply w-0 overflow-hidden opacity-0;
    }

    .track-row:hover .row-actions,
    .row-actions:focus-within {
      @apply w-auto overflow-visible opacity-100;
    }

    .track-row:hover .track-duration,
    .track-row:has(.row-actions:focus-within) .track-duration {
      @apply hidden;
    }
  }

  .track-action-btn {
    @apply w-6 h-6 flex items-center justify-center border-none bg-transparent rounded-full cursor-pointer text-text-muted opacity-0 transition-all duration-150;
  }

  .track-row:hover .track-action-btn,
  .track-action-btn:focus-visible {
    @apply opacity-100;
  }

  .track-action-btn:hover {
    @apply bg-surface-active text-text-primary;
  }

  .track-action-btn svg {
    @apply w-4 h-4;
  }
</style>
