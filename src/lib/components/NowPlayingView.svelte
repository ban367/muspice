<!--
  @component NowPlayingView
  Now Playingの画面。再生中の曲を大きく表示し、歌詞を出す。

  メインの領域いっぱいに重ねて表示する（サイドバーと下のプレーヤーはそのまま。開いていた画面は、
  閉じると元の位置のまま戻る）。プレーヤーのアルバムアートを押すと開閉し、Escキーでも閉じる。

  - 左: アルバムアート・曲の情報・お気に入りと評価・次に再生する曲
  - 右: 歌詞。時刻付きの歌詞は、再生位置の行を強調して中央へ寄せ、行を押すとその位置へ移る

  歌詞は、曲と同じ名前の`.lrc`ファイルか、埋め込みの歌詞（`get_track_lyrics`。ADR-045）。
-->
<script lang="ts">
  import { tick } from 'svelte';
  import {
    useFavoriteTracksQuery,
    useSetRatingMutation,
    useTrackLyricsQuery,
    useTracksQuery
  } from '#lib/queries/tracks.js';
  import { player } from '#lib/stores/player.svelte.js';
  import { seekPlayback } from '#lib/stores/playback.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { findCurrentLine, parseLyrics } from '#lib/utils/lyrics.js';
  import AlbumArt from './AlbumArt.svelte';
  import FavoriteButton from './library/FavoriteButton.svelte';
  import RatingStars from './library/RatingStars.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  /** 「次に再生」に出す曲数 */
  const UP_NEXT_COUNT = 3;
  /** 歌詞を自分でスクロールした後、再生位置の行へ寄せるのを休む時間（ミリ秒） */
  const AUTO_SCROLL_PAUSE_MS = 4000;

  const currentTrack = $derived(player.currentTrack);
  const trackId = $derived(currentTrack?.id ?? null);

  // 再生キューの曲は、キューに入れた時点の内容のため、評価は全曲の一覧（変更のたびに
  // 書き換わる）から、お気に入りはお気に入りの一覧から調べる
  const tracksQuery = useTracksQuery();
  const favoritesQuery = useFavoriteTracksQuery();
  const setRatingMutation = useSetRatingMutation();
  const rating = $derived(
    tracksQuery.data?.find((track) => track.id === trackId)?.rating ?? currentTrack?.rating ?? 0
  );
  const isFavorite = $derived((favoritesQuery.data ?? []).some((track) => track.id === trackId));

  const upNext = $derived(player.upcomingTracks.slice(0, UP_NEXT_COUNT));

  // 歌詞（再生中の曲の分だけを読む）
  const lyricsQuery = $derived(trackId === null ? null : useTrackLyricsQuery(trackId));
  const lyrics = $derived(lyricsQuery?.data ? parseLyrics(lyricsQuery.data.text) : null);
  const currentLine = $derived(
    lyrics?.isSynced ? findCurrentLine(lyrics.lines, player.currentTime) : -1
  );

  // 再生位置の行を、歌詞の領域の中央へ寄せる（自分でスクロールした直後は寄せない）
  let lyricsElement = $state<HTMLElement>();
  let lastUserScrollAt = 0;
  $effect(() => {
    const line = currentLine;
    if (line < 0 || !lyricsElement) return;
    if (Date.now() - lastUserScrollAt < AUTO_SCROLL_PAUSE_MS) return;
    void tick().then(() => {
      lyricsElement
        ?.querySelector<HTMLElement>(`[data-line="${line}"]`)
        ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    });
  });

  // 曲が変わったら、歌詞の先頭から表示する
  $effect(() => {
    void trackId;
    lastUserScrollAt = 0;
    lyricsElement?.scrollTo({ top: 0 });
  });

  function close() {
    ui.isNowPlayingOpen = false;
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    // 上に開いているダイアログ・メニューのEscは、そちらが受け取る
    if (event.key !== 'Escape' || event.defaultPrevented) return;
    if (document.querySelector('dialog[open]')) return;
    close();
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<section class="now-playing" aria-label={m.nowPlaying.title}>
  <button
    type="button"
    class="close-button"
    onclick={close}
    title={m.nowPlaying.close}
    aria-label={m.nowPlaying.close}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="w-5 h-5"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if currentTrack}
    <div class="content">
      <!-- 曲の情報 -->
      <div class="info">
        <div class="art">
          <AlbumArt
            src={albumArtUrl(currentTrack.id)}
            alt={m.common.albumArt}
            rounded="lg"
            placeholderType="music"
          />
        </div>
        <h2 class="track-title">{currentTrack.title || currentTrack.fileName}</h2>
        <p class="track-artist">{currentTrack.artist || m.common.unknownArtist}</p>
        {#if currentTrack.album}
          <p class="track-album">
            {currentTrack.album}{currentTrack.year ? ` · ${currentTrack.year}` : ''}
          </p>
        {/if}
        <div class="actions">
          <FavoriteButton trackId={currentTrack.id} {isFavorite} size="medium" />
          <RatingStars
            {rating}
            onChange={(value) =>
              setRatingMutation.mutateAsync({ trackId: currentTrack.id, rating: value })}
          />
        </div>

        {#if upNext.length > 0}
          <div class="up-next">
            <h3 class="section-label">{m.nowPlaying.upNext}</h3>
            <ol class="up-next-list">
              {#each upNext as track, index (index)}
                <li class="up-next-item">
                  <span class="up-next-title">{track.title || track.fileName}</span>
                  <span class="up-next-artist">{track.artist || m.common.unknownArtist}</span>
                </li>
              {/each}
            </ol>
          </div>
        {/if}
      </div>

      <!-- 歌詞 -->
      <div
        class="lyrics"
        role="region"
        aria-label={m.nowPlaying.lyrics}
        bind:this={lyricsElement}
        onwheel={() => (lastUserScrollAt = Date.now())}
        ontouchmove={() => (lastUserScrollAt = Date.now())}
      >
        {#if lyricsQuery?.isPending}
          <p class="lyrics-message">{m.common.loading}</p>
        {:else if lyricsQuery?.isError}
          <p class="lyrics-message">
            {m.nowPlaying.lyricsFailed(toErrorMessage(lyricsQuery.error))}
          </p>
        {:else if lyrics && lyrics.lines.length > 0}
          {#if lyrics.isSynced}
            <!-- 時刻付きの歌詞: 行を押すと、その位置へ移る -->
            <ol class="lyrics-lines synced">
              {#each lyrics.lines as line, index (index)}
                <li>
                  <button
                    type="button"
                    class="lyrics-line"
                    class:current={index === currentLine}
                    class:past={index < currentLine}
                    data-line={index}
                    aria-current={index === currentLine ? 'true' : undefined}
                    onclick={() => line.time !== null && seekPlayback(line.time)}
                  >
                    {line.text || '♪'}
                  </button>
                </li>
              {/each}
            </ol>
          {:else}
            <div class="lyrics-lines plain">
              {#each lyrics.lines as line, index (index)}
                <p class="lyrics-text">{line.text}</p>
              {/each}
            </div>
          {/if}
        {:else}
          <p class="lyrics-message">{m.nowPlaying.noLyrics}</p>
          <p class="lyrics-hint">{m.nowPlaying.noLyricsHint}</p>
        {/if}
      </div>
    </div>
  {:else}
    <div class="empty">{m.player.noTrack}</div>
  {/if}
</section>

<style>
  @reference "../../app.css";

  /* ページの領域（レイアウトの`.main-area`）いっぱいに重ねる。下はプレーヤー、右は右サイドバーのアイコンの分を空ける */
  .now-playing {
    @apply absolute inset-0 z-10 flex flex-col bg-base-100 pb-player-height;
    padding-right: 3rem;
  }

  .close-button {
    @apply absolute top-3 left-3 z-10 flex items-center justify-center w-9 h-9 bg-transparent border-none rounded-full text-text-muted cursor-pointer transition-colors;
  }

  .close-button:hover {
    @apply bg-surface-active text-text-primary;
  }

  .content {
    @apply flex-1 min-h-0 grid gap-8 px-10 pt-12 pb-6;
    grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
  }

  .info {
    @apply flex flex-col items-center min-h-0 overflow-y-auto text-center;
  }

  /* アルバムアート（正方形。領域の幅と高さの小さい方に合わせる） */
  .art {
    @apply w-full shrink-0 aspect-square rounded-lg overflow-hidden bg-base-300 shadow-lg;
    max-width: min(22rem, 45vh);
  }

  .track-title {
    @apply m-0 mt-5 text-xl font-bold text-text-primary break-words;
  }

  .track-artist {
    @apply m-0 mt-1 text-base text-text-secondary break-words;
  }

  .track-album {
    @apply m-0 mt-1 text-sm text-text-muted break-words;
  }

  .actions {
    @apply flex items-center gap-3 mt-3;
  }

  .up-next {
    @apply w-full mt-6 pt-4 border-t border-border text-left;
    max-width: 22rem;
  }

  .section-label {
    @apply m-0 mb-2 text-xs font-semibold uppercase text-text-muted;
  }

  .up-next-list {
    @apply list-none m-0 p-0 flex flex-col gap-1.5;
  }

  .up-next-item {
    @apply flex flex-col min-w-0;
  }

  .up-next-title {
    @apply text-sm text-text-secondary truncate;
  }

  .up-next-artist {
    @apply text-xs text-text-muted truncate;
  }

  /* 歌詞（この中でスクロールする） */
  .lyrics {
    @apply min-h-0 overflow-y-auto pr-2;
  }

  .lyrics-lines {
    @apply list-none m-0 p-0;
  }

  /* 時刻付きの歌詞: 最初と最後の行も中央へ寄せられるよう、上下を空ける */
  .lyrics-lines.synced {
    @apply flex flex-col gap-1;
    padding-block: 30vh;
  }

  .lyrics-line {
    @apply block w-full px-3 py-1.5 bg-transparent border-none rounded-md text-left text-text-muted cursor-pointer transition-colors duration-200;
    font: inherit;
    font-size: 1.125rem;
    line-height: 1.625;
  }

  .lyrics-line:hover {
    @apply bg-surface-hover text-text-secondary;
  }

  .lyrics-line.past {
    @apply text-text-dimmed;
  }

  .lyrics-line.current {
    @apply text-text-primary font-semibold;
    font-size: 1.375rem;
  }

  .lyrics-lines.plain {
    @apply py-2;
  }

  .lyrics-text {
    @apply m-0 text-base leading-loose text-text-secondary whitespace-pre-wrap break-words;
    min-height: 1.75rem;
  }

  .lyrics-message {
    @apply m-0 mt-16 text-center text-text-muted;
  }

  .lyrics-hint {
    @apply m-0 mt-2 text-center text-sm text-text-dimmed;
  }

  .empty {
    @apply flex-1 flex items-center justify-center text-text-muted;
  }

  /* 幅が狭い場合は、曲の情報の下に歌詞を並べる */
  @media (max-width: 900px) {
    .content {
      @apply block overflow-y-auto px-6;
    }

    .info {
      @apply overflow-visible mb-8;
    }

    .lyrics {
      @apply overflow-visible pr-0;
    }

    .lyrics-lines.synced {
      padding-block: 1rem;
    }
  }
</style>
