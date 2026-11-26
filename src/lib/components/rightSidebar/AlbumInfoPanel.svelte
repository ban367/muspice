<!--
  @component AlbumInfoPanel
  右サイドバーのアルバム情報パネル。上部に再生中の曲のカード（`SidePanelTrack`）を固定して出し、その下に、その曲が
  入っているアルバムの曲をスクロールして見られるように出す。再生中の曲が変わると追随する（一覧で選んだ曲は、曲情報パネル
  （`TrackInfoPanel`）に出す。ADR-078）。
  アルバムは、アルバムの画面と同じく「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめ、
  ディスクごとに曲を区切る。アルバムのない曲は、そのことを示す。
  アルバムの曲の選択・キー操作・ダブルクリックでの再生・ドラッグ（プレイリストへの追加）は、ほかの曲の一覧と同じ。
  アルバムの見出しのドラッグでは、アルバムの曲すべてを運ぶ。
  パネルの幅が狭いため、行には番号・曲名・時間だけを出し、お気に入りと評価は行にポインタを置いた時に出す。
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { Track } from '#lib/types/models.js';
  import { useAlbumTracksQuery, useSetRatingMutation } from '#lib/queries/tracks.js';
  import { player, playNow } from '#lib/stores/player.svelte.js';
  import { groupArtist } from '#lib/utils/albumKey.js';
  import { artistsOf, goToAlbum, goToArtist } from '#lib/utils/libraryReveal.js';
  import { groupByDisc } from '#lib/utils/albumDiscs.js';
  import { formatDuration, formatTotalDuration } from '#lib/utils/format.js';
  import { TrackSelection, handleTrackListKeydown } from '#lib/utils/trackSelection.svelte.js';
  import { startTrackDrag } from '#lib/utils/trackDrag.js';
  import PlayingIndicator from '../library/PlayingIndicator.svelte';
  import RatingStars from '../library/RatingStars.svelte';
  import FavoriteButton from '../library/FavoriteButton.svelte';
  import SidePanel from './SidePanel.svelte';
  import SidePanelMessage from './SidePanelMessage.svelte';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelTrack from './SidePanelTrack.svelte';
  import SidePanelTrackDetails from './SidePanelTrackDetails.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const current = $derived(player.currentTrack);

  const setRatingMutation = useSetRatingMutation();

  // 再生中の曲のアルバム（アルバムが変わった時だけ曲を取り直す。同じアルバムの次の曲へ進んでも取り直さない）
  const albumName = $derived(current?.album ?? null);
  const albumArtist = $derived(current ? groupArtist(current) : null);
  // アルバムアーティストの名前を押した時に移るアーティスト（複数のアーティストの曲は、最初のアーティスト）
  const linkedArtist = $derived(current ? (artistsOf(current)[0] ?? null) : null);
  const albumTracksQuery = $derived(
    albumName === null ? null : useAlbumTracksQuery(albumName, albumArtist)
  );

  /** 曲の一覧の区切り（アルバムのディスク） */
  interface Section {
    key: string;
    /** 見出し（ディスクが1枚だけのアルバムはnull） */
    heading: string | null;
    tracks: Track[];
  }

  const sections = $derived.by((): Section[] => {
    const discs = groupByDisc(albumTracksQuery?.data ?? []);
    return discs.map((disc) => ({
      key: String(disc.discNumber),
      heading: discs.length > 1 ? m.common.disc(disc.discNumber) : null,
      tracks: disc.tracks
    }));
  });

  // 表示順の曲（選択・再生はこの順で行う）
  const tracks = $derived(sections.flatMap((section) => section.tracks));
  const totalDuration = $derived(tracks.reduce((sum, track) => sum + (track.duration ?? 0), 0));

  /** アルバムの見出しの3行目（ジャンル・年・曲数と時間。曲数と時間は、曲を取得してから足す） */
  const albumStats = $derived(
    [
      current?.genre,
      current?.year,
      tracks.length > 0
        ? m.common.trackCountAndDuration(tracks.length, formatTotalDuration(totalDuration))
        : null
    ]
      .filter(Boolean)
      .join(' · ')
  );

  // 取得し終えて1曲もない（ライブラリから削除された、またはタグが変わってアルバムが変わった）
  const isNotFound = $derived(albumTracksQuery?.isSuccess === true && tracks.length === 0);

  // トラックの選択（クリック・キーボード）
  const selection = new TrackSelection(() => tracks);

  /**
   * 曲の情報に出す曲（パネルの中で選んだ曲があれば最後に選んだ曲、なければ再生中の曲）
   *
   * 再生中の曲は、キューに入れた時点の内容のため、取得したアルバムの曲があればそちらを使う（再生回数など）。
   */
  const detailsTrack = $derived.by(() => {
    const picked = selection.lastSelected;
    if (picked) return picked;
    if (!current) return null;
    const id = current.id;
    return tracks.find((track) => track.id === id) ?? current;
  });

  /** 曲の情報の見出しに添える文（再生中の曲か、パネルの中で選んだ別の曲か） */
  const detailsNote = $derived(
    detailsTrack && detailsTrack.id !== current?.id ? m.albumInfo.selected : m.albumInfo.playing
  );

  /** スクロールする領域（アルバムが替わった時に先頭へ戻す） */
  let body = $state<HTMLElement>();

  /** 見える位置へ出した、再生中の曲（曲が変わった時だけ出すため。リアクティブにしない） */
  let revealedTrackId: string | null = null;

  // 別のアルバムに替わったら、選択を消して先頭へ戻す（同じアルバムの曲へ進んだ時・取り直しただけでは消さない）
  $effect(() => {
    void albumName;
    void albumArtist;
    // 選択の中身には反応させない（選択を変えるたびに消えてしまう）
    untrack(() => {
      selection.reset();
      revealedTrackId = null;
      body?.scrollTo({ top: 0 });
    });
  });

  // 再生中の曲が変わったら、アルバムの曲を取得した後に、その行を見える位置へ出す
  $effect(() => {
    const trackId = current?.id ?? null;
    if (trackId === null || trackId === revealedTrackId) return;
    if (!tracks.some((track) => track.id === trackId)) return;
    revealedTrackId = trackId;
    untrack(() => {
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

  // アルバムの見出しのドラッグでは、アルバムの曲すべてを運ぶ（選択は変えない）
  function handleAlbumDragStart(event: DragEvent) {
    startTrackDrag(
      event,
      tracks.map((track) => track.id),
      { label: albumName ?? '', detail: m.common.trackCount(tracks.length) }
    );
  }

  /**
   * 一覧のキーボード操作（矢印キーで選択を移す・Enterで再生する）
   */
  function handleListKeydown(event: KeyboardEvent) {
    handleTrackListKeydown(event, selection, {
      onActivate: (trackId) => {
        const track = tracks.find((candidate) => candidate.id === trackId);
        if (track) playNow([track]);
      }
    });
  }

  // アルバムをすべて再生（再生中の曲の次に入れて再生する）
  function handlePlayAll() {
    playNow(tracks);
  }
</script>

<SidePanel title={m.rightSidebar.albumInfoTitle} bind:body>
  {#snippet top()}
    <!-- 再生中の曲（スクロールせず、常に見えるようにする） -->
    <SidePanelTrack
      label={m.rightSidebar.nowPlaying}
      track={current}
      emptyText={m.rightSidebar.nothingPlaying}
    />
    <!-- 曲の情報（再生中の曲、またはパネルの中で選んだ曲） -->
    {#if detailsTrack}
      <SidePanelTrackDetails track={detailsTrack} note={detailsNote} />
    {/if}
  {/snippet}

  {#if !current}
    <SidePanelMessage text={m.albumInfo.empty} />
  {:else if albumName === null}
    <SidePanelMessage text={m.albumInfo.noAlbum} />
  {:else if isNotFound}
    <SidePanelMessage text={m.albumInfo.notFound} />
  {:else}
    <!-- 再生中の曲のアルバム（スクロールする） -->
    <SidePanelSection title={m.albumInfo.album} flush>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="album-heading" draggable={tracks.length > 0} ondragstart={handleAlbumDragStart}>
        <div class="album-heading-text">
          <!-- 名前を押すと、アルバム・アーティストの画面へ移る（ADR-110） -->
          <button
            type="button"
            class="album-name text-link"
            title={m.contextMenu.goToAlbum}
            onclick={() => current && goToAlbum(current)}
          >
            {albumName}
          </button>
          {#if linkedArtist !== null}
            <button
              type="button"
              class="album-artist text-link"
              title={m.contextMenu.goToArtist}
              onclick={() => goToArtist(linkedArtist)}
            >
              {albumArtist || m.common.unknownArtist}
            </button>
          {:else}
            <p class="album-artist">{albumArtist || m.common.unknownArtist}</p>
          {/if}
          <!-- 曲を取得するまでは、曲数がわからない（高さは変えずに空ける） -->
          <p class="album-stats">{albumStats || ' '}</p>
        </div>
        <button
          class="album-play"
          onclick={handlePlayAll}
          disabled={tracks.length === 0}
          title={m.common.playAll}
          aria-label={m.common.playAll}
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
        </button>
      </div>

      <!-- 曲の一覧 -->
      <div
        class="track-list"
        role="listbox"
        aria-multiselectable="true"
        aria-label={albumName}
        tabindex="0"
        onkeydown={handleListKeydown}
      >
        {#each sections as section (section.key)}
          {#if section.heading !== null}
            <div class="section-heading" title={section.heading}>{section.heading}</div>
          {/if}
          {#each section.tracks as track (track.id)}
            {@const isPlaying = current.id === track.id}
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
              ondblclick={() => playNow([track])}
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
                <!-- 曲のアーティストは、アルバムのアーティストと違う場合だけ表示する（コンピレーションなど） -->
                {#if track.artist && track.artist !== albumArtist}
                  <span class="row-artist" title={track.artist}>{track.artist}</span>
                {/if}
              </div>
              <div class="row-end">
                <!-- 幅が狭いため、ポインタを置いた時・中のボタンにフォーカスがある時だけ、時間の代わりに出す -->
                <div class="row-actions">
                  <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} />
                  <RatingStars
                    rating={track.rating}
                    onChange={(value) =>
                      setRatingMutation.mutateAsync({ trackId: track.id, rating: value })}
                  />
                </div>
                <span class="row-duration">{formatDuration(track.duration)}</span>
              </div>
            </div>
          {/each}
        {/each}
      </div>
    </SidePanelSection>
  {/if}
</SidePanel>

<style>
  @reference "../../../app.css";

  /* アルバムの見出し（名前・アーティスト・ジャンルと年と曲数と時間・再生の操作） */
  .album-heading {
    @apply flex items-center gap-2 px-4 pb-1;
  }

  .album-heading-text {
    @apply flex-1 min-w-0;
  }

  .album-name {
    @apply m-0 h-5 leading-5 text-sm font-semibold text-text-primary truncate;
  }

  .album-artist {
    @apply m-0 h-4 leading-4 text-xs text-text-secondary truncate;
  }

  .album-stats {
    @apply m-0 h-4 leading-4 text-xs text-text-dimmed truncate;
  }

  /* 名前のボタンは、名前の長さだけの幅にする（余白を押しても移らない） */
  .album-name.text-link,
  .album-artist.text-link {
    @apply w-fit;
  }

  .album-play {
    @apply w-8 h-8 flex items-center justify-center shrink-0 border-none rounded-full cursor-pointer transition-all duration-150;
    @apply bg-secondary text-black;
  }

  .album-play:hover:not(:disabled) {
    @apply bg-secondary-focus;
  }

  .album-play:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .album-play svg {
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
