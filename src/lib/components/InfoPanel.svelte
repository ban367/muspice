<!--
  @component InfoPanel
  右サイドバーの情報パネル。グリッドで選んだアルバム・アーティストの曲と、曲の一覧で選んだ曲の
  アルバムを、一覧を見たまま表示する（曲の一覧で選んだ曲は、パネルの一覧の中でも選んで示す）。
  アルバムはディスクごと、アーティストはアルバムごとに曲を区切る。アルバムのない曲は、曲だけを表示する。
  曲の選択・キー操作・ダブルクリックでの再生・ドラッグ（プレイリストへの追加）は、ほかの曲の一覧と同じ。
  パネルの幅が狭いため、行には番号・曲名・時間だけを出し、お気に入りと評価は行にポインタを置いた時に出す。
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { Track } from '#lib/types/models.js';
  import { ui, type InfoTarget } from '#lib/stores/ui.svelte.js';
  import {
    useAlbumTracksQuery,
    useArtistAlbumsQuery,
    useSetRatingMutation
  } from '#lib/queries/tracks.js';
  import { player, playTrackFromQueue, playShuffled } from '#lib/stores/player.svelte.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { albumKey } from '#lib/utils/albumKey.js';
  import { groupByDisc } from '#lib/utils/albumDiscs.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import AlbumArt from './AlbumArt.svelte';
  import PlayingIndicator from './library/PlayingIndicator.svelte';
  import RatingStars from './library/RatingStars.svelte';
  import FavoriteButton from './library/FavoriteButton.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 表示する項目（まだ選んでいなければnull） */
    target: InfoTarget | null;
  }

  let { target }: Props = $props();

  const setRatingMutation = useSetRatingMutation();

  // 表示する項目の名前（名前が変わった時だけ曲を取り直す。同じ項目を選び直しても取り直さない）
  const albumName = $derived(target?.type === 'album' ? target.album.name : null);
  const albumArtist = $derived(target?.type === 'album' ? target.album.artist : null);
  const artistName = $derived(target?.type === 'artist' ? target.artist.name : null);

  // アルバムは曲を、アーティストはアルバムごとの曲を取得する
  const albumTracksQuery = $derived(
    albumName === null ? null : useAlbumTracksQuery(albumName, albumArtist)
  );
  const artistAlbumsQuery = $derived(artistName === null ? null : useArtistAlbumsQuery(artistName));

  /** 曲の一覧の区切り（アルバムのディスク・アーティストのアルバム） */
  interface Section {
    key: string;
    /** 見出し（ディスクが1枚だけのアルバムはnull） */
    heading: string | null;
    /** 区切りの最初の曲の、一覧の全体（`tracks`）の中の位置 */
    start: number;
    tracks: Track[];
  }

  const sections = $derived.by((): Section[] => {
    const groups = artistAlbumsQuery
      ? (artistAlbumsQuery.data ?? []).map((album) => ({
          key: albumKey(album),
          heading: album.name,
          tracks: album.tracks
        }))
      : groupByDisc(albumTracksQuery?.data ?? []).map((disc, _, discs) => ({
          key: String(disc.discNumber),
          heading: discs.length > 1 ? m.common.disc(disc.discNumber) : null,
          tracks: disc.tracks
        }));
    let start = 0;
    return groups.map((group) => {
      const section = { ...group, start };
      start += group.tracks.length;
      return section;
    });
  });

  // 表示順の曲（選択・再生はこの順で行う。アーティストは、アルバムをまたいで続けて再生する）
  const tracks = $derived(sections.flatMap((section) => section.tracks));

  // 取得し終えて1曲もない（項目の曲が削除された、またはタグが変わって項目の名前が変わった）
  const isNotFound = $derived(
    (albumTracksQuery ?? artistAlbumsQuery)?.isSuccess === true && tracks.length === 0
  );

  // 見出しの値（曲を取得するまでは、一覧の項目の値を使う。取得した後は、曲の変更に追随させる）
  const summary = $derived(
    target?.type === 'album' ? target.album : target?.type === 'artist' ? target.artist : null
  );
  const trackCount = $derived(tracks.length > 0 ? tracks.length : (summary?.trackCount ?? 0));
  const totalDuration = $derived(
    tracks.length > 0
      ? tracks.reduce((sum, track) => sum + (track.duration ?? 0), 0)
      : (summary?.totalDuration ?? 0)
  );
  const artTrackId = $derived(tracks[0]?.id ?? summary?.representativeTrackId ?? null);

  // 曲のアーティストを出すかの基準（アルバムはまとめたアーティスト、アーティストはその名前）
  const groupArtist = $derived(
    target?.type === 'album'
      ? target.album.artist
      : target?.type === 'artist'
        ? target.artist.name
        : null
  );

  // 曲の一覧で選んだ曲（アルバムの中で選んで示す。グリッドで選んだ項目はnull）
  const chosenTrackId = $derived(target?.type === 'album' ? (target.trackId ?? null) : null);

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => tracks);

  /** スクロールする領域（項目を切り替えた時に先頭へ戻す） */
  let body = $state<HTMLElement>();

  /** 曲の一覧で選んだ曲のうち、パネルの一覧で選んだもの（選び直した時だけ選ぶため。リアクティブにしない） */
  let shownChosenTrackId: string | null = null;

  // 別の項目に切り替わったら、選択を消して先頭へ戻す（データを取り直しただけでは消さない）
  $effect(() => {
    void albumName;
    void albumArtist;
    void artistName;
    // 選択の中身には反応させない（選択を変えるたびに消えてしまう）
    untrack(() => {
      selection.reset();
      shownChosenTrackId = null;
      body?.scrollTo({ top: 0 });
    });
  });

  // 曲の一覧で選んだ曲を、曲を取得した後に選び、その行を見える位置へ出す。曲の一覧で選び直した時
  // だけ行い、取り直しただけでは選び直さない（パネルの中で選んだ曲を残す）
  $effect(() => {
    const trackId = chosenTrackId;
    if (trackId === null) {
      // グリッドで選んだ項目に替わった（後で同じ曲を選び直した時も、選び直す）
      shownChosenTrackId = null;
      return;
    }
    if (trackId === shownChosenTrackId) return;
    if (!tracks.some((track) => track.id === trackId)) return;
    shownChosenTrackId = trackId;
    untrack(() => {
      selection.selectOnly(trackId);
      body
        ?.querySelector(`[data-track-id="${CSS.escape(trackId)}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    });
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
        if (index !== -1) playTrackFromQueue(tracks, index);
      }
    });
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

  // パネルを閉じる（横に並べている間は、パネルの外を押しても閉じないため）
  function close() {
    ui.isRightSidebarExpanded = false;
  }
</script>

<div class="panel-header">
  <h3>
    {target?.type === 'album'
      ? m.fields.album
      : target?.type === 'artist'
        ? m.fields.artist
        : target?.type === 'track'
          ? m.infoPanel.track
          : m.rightSidebar.infoTitle}
  </h3>
  <button
    class="close-button"
    onclick={close}
    title={m.infoPanel.close}
    aria-label={m.infoPanel.close}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      aria-hidden="true"
    >
      <line x1="18" y1="6" x2="6" y2="18" />
      <line x1="6" y1="6" x2="18" y2="18" />
    </svg>
  </button>
</div>

<!-- 見出しも曲と一緒にスクロールする（狭いウィンドウでも曲の行を多く見せる） -->
<div class="panel-body" bind:this={body}>
  {#if target === null}
    <p class="panel-message">{m.infoPanel.empty}</p>
  {:else if target.type === 'track'}
    <!-- アルバムのない曲: 曲だけを表示する -->
    <div class="summary">
      <div class="summary-art">
        <AlbumArt
          src={albumArtUrl(target.track.id)}
          alt={target.track.title || target.track.fileName}
          rounded="lg"
          placeholderType="music"
        />
      </div>
      <h2 class="summary-name">{target.track.title || target.track.fileName}</h2>
      <p class="summary-artist">{target.track.artist || m.common.unknownArtist}</p>
      {#if target.track.genre || target.track.year}
        <p class="summary-meta">
          {[target.track.genre, target.track.year].filter(Boolean).join(' · ')}
        </p>
      {/if}
    </div>
    <p class="panel-message">{m.infoPanel.noAlbum}</p>
  {:else}
    <div class="summary">
      {#if target.type === 'album'}
        <button
          type="button"
          class="summary-art"
          title={m.albumArtDialog.open}
          aria-label={m.albumArtDialog.open}
          onclick={() => albumArtDialog.open(tracks)}
          disabled={tracks.length === 0}
        >
          <AlbumArt
            src={artTrackId === null ? null : albumArtUrl(artTrackId)}
            alt={target.album.name}
            rounded="lg"
          />
        </button>
        <h2 class="summary-name">{target.album.name}</h2>
        <p class="summary-artist">{target.album.artist || m.common.unknownArtist}</p>
        {#if tracks[0]?.genre || tracks[0]?.year}
          <p class="summary-meta">
            {[tracks[0]?.genre, tracks[0]?.year].filter(Boolean).join(' · ')}
          </p>
        {/if}
      {:else}
        <div class="summary-art">
          <AlbumArt
            src={artTrackId === null ? null : albumArtUrl(artTrackId)}
            alt={target.artist.name}
            rounded="lg"
            placeholderType="person"
          />
        </div>
        <h2 class="summary-name">{target.artist.name}</h2>
        {#if !isNotFound}
          <p class="summary-meta">
            {m.library.albumSummary(artistAlbumsQuery?.data?.length ?? target.artist.albumCount)}
          </p>
        {/if}
      {/if}
      <!-- 見つからない時は、曲数（一覧の項目の古い値）と操作を出さない -->
      {#if !isNotFound}
        <!-- 曲の一覧で選んだ曲のアルバムは、曲を取得するまで曲数がわからない（高さは変えずに空ける） -->
        <p class="summary-stats">
          {trackCount > 0
            ? m.common.trackCountAndDuration(trackCount, formatTotalDuration(totalDuration))
            : '\u00a0'}
        </p>
        <div class="summary-actions">
          <button
            class="action-button primary"
            onclick={handlePlayAll}
            disabled={tracks.length === 0}
            title={m.common.playAll}
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
            {m.common.play}
          </button>
          <button class="action-button" onclick={handleShufflePlay} disabled={tracks.length === 0}>
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
        </div>
      {/if}
    </div>

    {#if isNotFound}
      <p class="panel-message">{m.infoPanel.notFound}</p>
    {:else}
      <!-- 曲の一覧 -->
      <div
        class="track-list"
        role="listbox"
        aria-multiselectable="true"
        aria-label={target.type === 'album' ? target.album.name : target.artist.name}
        tabindex="0"
        onkeydown={handleListKeydown}
      >
        {#each sections as section (section.key)}
          {#if section.heading !== null}
            <div class="section-heading" title={section.heading}>{section.heading}</div>
          {/if}
          {#each section.tracks as track, indexInSection (track.id)}
            {@const isPlaying = player.currentTrack?.id === track.id}
            <!-- キー操作は一覧（listbox）で受けるため、行はフォーカスを受けない -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              class="track-row"
              class:selected={selection.has(track.id)}
              class:missing={track.isMissing}
              class:playing={isPlaying}
              title={track.isMissing ? m.common.fileMissing : undefined}
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track.id)}
              onclick={(e) => handleTrackClick(track.id, e)}
              ondblclick={() => playTrackFromQueue(tracks, section.start + indexInSection)}
              role="option"
              aria-selected={selection.has(track.id)}
              data-track-id={track.id}
            >
              <span class="row-number">
                {#if isPlaying}
                  <PlayingIndicator size="small" />
                {:else}
                  {track.trackNumber ?? ''}
                {/if}
              </span>
              <div class="row-info">
                <span class="row-title" title={track.title || track.fileName}>
                  {track.title || track.fileName}
                </span>
                <!-- 曲のアーティストは、項目のアーティストと違う場合だけ表示する（コンピレーションなど） -->
                {#if track.artist && track.artist !== groupArtist}
                  <span class="row-artist" title={track.artist}>{track.artist}</span>
                {/if}
              </div>
              <div class="row-end">
                <!-- 幅が狭いため、ポインタを置いた時・中のボタンにフォーカスがある時だけ、時間の代わりに出す -->
                <div class="row-actions">
                  <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
                  <RatingStars
                    rating={track.rating}
                    onChange={(rating) =>
                      setRatingMutation.mutateAsync({ trackId: track.id, rating })}
                  />
                </div>
                <span class="row-duration">{formatDuration(track.duration)}</span>
              </div>
            </div>
          {/each}
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  @reference "../../app.css";

  .panel-header {
    @apply flex items-center justify-between px-4 py-3 border-b border-border shrink-0;
  }

  .panel-header h3 {
    @apply m-0 text-sm font-semibold text-text-primary;
  }

  .close-button {
    @apply w-6 h-6 flex items-center justify-center border-none rounded-full bg-transparent cursor-pointer text-text-muted transition-all duration-150;
  }

  .close-button:hover {
    @apply bg-surface-active text-text-primary;
  }

  .close-button svg {
    @apply w-4 h-4;
  }

  .panel-body {
    @apply flex-1 min-h-0 overflow-y-auto;
  }

  .panel-message {
    @apply m-0 py-8 px-4 text-center text-sm text-text-dimmed;
  }

  /* 見出し（ジャケット画像・名前・操作） */
  .summary {
    @apply flex flex-col items-center px-4 pt-4 pb-3 text-center;
    /* アクセントカラーを薄くした色から透明へ */
    background: linear-gradient(
      to bottom,
      color-mix(in oklab, var(--color-primary) 15%, transparent),
      transparent
    );
  }

  .summary-art {
    @apply w-48 h-48 shrink-0 rounded-lg overflow-hidden p-0 border-none bg-transparent;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  button.summary-art {
    @apply cursor-pointer transition-opacity;
  }

  button.summary-art:hover:not(:disabled) {
    @apply opacity-85;
  }

  button.summary-art:disabled {
    @apply cursor-default;
  }

  .summary-name {
    @apply m-0 mt-3 text-lg font-bold leading-tight text-text-primary break-words;
  }

  .summary-artist {
    @apply m-0 mt-1 text-sm text-text-secondary;
  }

  .summary-meta {
    @apply m-0 mt-1 text-xs text-text-muted;
  }

  .summary-stats {
    @apply m-0 mt-1 text-xs text-text-dimmed;
  }

  .summary-actions {
    @apply flex items-center gap-2 mt-3;
  }

  .action-button {
    @apply flex items-center gap-1.5 py-1.5 px-4 border-none rounded-full text-sm font-medium cursor-pointer transition-all duration-150;
    @apply bg-surface-active text-text-primary;
  }

  .action-button:hover:not(:disabled) {
    @apply bg-surface-strong;
  }

  .action-button.primary {
    @apply bg-secondary text-black;
  }

  .action-button.primary:hover:not(:disabled) {
    @apply bg-secondary-focus;
  }

  .action-button:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .action-button svg {
    @apply w-4 h-4;
  }

  /* 一覧はフォーカスを受けてキー操作を扱う（枠は出さず、選択中の行の色で示す） */
  .track-list {
    @apply px-2 pb-2 outline-none;
  }

  .section-heading {
    @apply mt-2 px-2 py-1.5 border-b border-border text-xs font-semibold text-text-secondary truncate;
  }

  .track-row {
    @apply grid gap-2 py-1.5 px-2 items-center rounded-md cursor-pointer transition-colors duration-100 select-none;
    grid-template-columns: 1.5rem minmax(0, 1fr) auto;
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

  .row-number {
    @apply flex items-center justify-center text-xs text-text-dimmed;
  }

  .track-row.playing .row-number {
    @apply text-secondary;
  }

  .row-info {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .row-title {
    @apply text-[0.8125rem] text-text-primary truncate;
  }

  .track-row.playing .row-title {
    @apply text-secondary;
  }

  .row-artist {
    @apply text-[0.6875rem] text-text-dimmed truncate;
  }

  .row-end {
    @apply flex items-center;
  }

  /* 幅を0にして隠す（display: noneと違い、中のボタンへTabでフォーカスを移せる） */
  .row-actions {
    @apply flex items-center gap-1.5 w-0 overflow-hidden opacity-0;
  }

  .track-row:hover .row-actions,
  .row-actions:focus-within {
    @apply w-auto overflow-visible opacity-100;
  }

  .row-duration {
    @apply text-xs text-text-dimmed text-right tabular-nums;
  }

  .track-row:hover .row-duration,
  .track-row:has(.row-actions:focus-within) .row-duration {
    @apply hidden;
  }
</style>
