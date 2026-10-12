<!--
  @component MiniPlayer
  ミニプレーヤー（メインウィンドウの小さな表示）。アルバムアート・曲名・基本の操作だけを出す。

  ウィンドウの大きさは`miniPlayer`（`set_mini_player`）が変える。この部品は、ウィンドウいっぱいに
  重ねて表示する（下では、通常の画面とプレーヤーが動き続ける。再生の制御はそちらが持つ）。
-->
<script lang="ts">
  import { formatTime, player } from '#lib/stores/player.svelte.js';
  import {
    seekPlayback,
    skipToNextTrack,
    skipToPreviousTrack,
    togglePlayback
  } from '#lib/stores/playback.svelte.js';
  import { miniPlayer } from '#lib/stores/miniPlayer.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import AlbumArt from './AlbumArt.svelte';
  import MarqueeText from './MarqueeText.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const track = $derived(player.currentTrack);

  /** 進行バーを押した位置へ移る */
  function seekTo(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (rect.width <= 0 || player.duration <= 0) return;
    const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    seekPlayback(ratio * player.duration);
  }

  function handleProgressKeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowLeft') seekPlayback(player.currentTime - 5);
    else if (event.key === 'ArrowRight') seekPlayback(player.currentTime + 5);
  }
</script>

<section class="mini-player" aria-label={m.miniPlayer.title}>
  <div class="art">
    <AlbumArt
      src={track ? albumArtUrl(track.id) : null}
      alt={m.common.albumArt}
      rounded="md"
      placeholderType="music"
    />
  </div>

  <div class="body">
    <div class="header">
      <div class="titles">
        {#if track}
          <MarqueeText text={track.title || track.fileName} class="mini-title" />
          <MarqueeText text={track.artist || m.common.unknownArtist} class="mini-artist" />
        {:else}
          <span class="mini-title">{m.player.noTrack}</span>
        {/if}
      </div>
      <div class="window-buttons">
        <button
          type="button"
          class="icon-button"
          class:active={miniPlayer.alwaysOnTop}
          onclick={() => miniPlayer.setAlwaysOnTop(!miniPlayer.alwaysOnTop)}
          title={m.miniPlayer.alwaysOnTop}
          aria-label={m.miniPlayer.alwaysOnTop}
          aria-pressed={miniPlayer.alwaysOnTop}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z" />
          </svg>
        </button>
        <button
          type="button"
          class="icon-button"
          onclick={() => miniPlayer.exit()}
          title={m.miniPlayer.exit}
          aria-label={m.miniPlayer.exit}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4"
            />
          </svg>
        </button>
      </div>
    </div>

    <div class="controls">
      <button
        type="button"
        class="icon-button"
        onclick={skipToPreviousTrack}
        disabled={!track}
        title={m.player.previous}
        aria-label={m.player.previous}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
        </svg>
      </button>
      <button
        type="button"
        class="icon-button play"
        onclick={togglePlayback}
        disabled={!track}
        title={player.isPlaying ? m.player.pause : m.player.play}
        aria-label={player.isPlaying ? m.player.pause : m.player.play}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d={player.isPlaying ? 'M6 19h4V5H6v14zm8-14v14h4V5h-4z' : 'M8 5v14l11-7z'} />
        </svg>
      </button>
      <button
        type="button"
        class="icon-button"
        onclick={skipToNextTrack}
        disabled={!track}
        title={m.player.next}
        aria-label={m.player.next}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
        </svg>
      </button>
      <span class="time">{formatTime(player.currentTime)} / {formatTime(player.duration)}</span>
    </div>

    <div
      class="progress"
      role="slider"
      aria-label={m.player.seek}
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(player.progress)}
      tabindex="0"
      onclick={seekTo}
      onkeydown={handleProgressKeydown}
    >
      <div class="progress-fill" style="width: {player.progress}%"></div>
    </div>
  </div>
</section>

<style>
  @reference "../../app.css";

  /* ウィンドウいっぱいに重ねる（通常の画面・プレーヤー・右サイドバーより手前） */
  .mini-player {
    @apply fixed inset-0 flex items-center gap-3 p-3 bg-base-200 text-text-primary;
    z-index: 60;
  }

  .art {
    @apply h-full aspect-square shrink-0 rounded-md overflow-hidden bg-base-300;
    max-height: 6.5rem;
  }

  .body {
    @apply flex-1 min-w-0 h-full flex flex-col justify-between;
  }

  .header {
    @apply flex items-start gap-2;
  }

  .titles {
    @apply flex-1 min-w-0;
  }

  :global(.mini-title) {
    @apply text-sm font-semibold text-text-primary;
  }

  :global(.mini-artist) {
    @apply text-xs text-text-secondary;
  }

  .window-buttons {
    @apply flex items-center gap-0.5 shrink-0;
  }

  .controls {
    @apply flex items-center gap-1;
  }

  .icon-button {
    @apply flex items-center justify-center p-1.5 bg-transparent border-none rounded-full text-text-secondary cursor-pointer transition-colors;
  }

  .icon-button:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .icon-button:disabled {
    @apply opacity-30 cursor-not-allowed;
  }

  .icon-button.active {
    @apply text-primary;
  }

  .icon-button.play {
    @apply bg-text-primary text-base-100 mx-1;
  }

  .icon-button.play:hover:not(:disabled) {
    @apply bg-text-primary text-base-100 opacity-90;
  }

  .time {
    @apply ml-auto text-[0.7rem] text-text-muted tabular-nums whitespace-nowrap;
  }

  .progress {
    @apply relative h-1.5 w-full rounded-full bg-base-400 cursor-pointer overflow-hidden;
  }

  .progress-fill {
    @apply h-full bg-primary;
  }
</style>
