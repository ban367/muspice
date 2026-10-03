<script lang="ts">
  import {
    player,
    formatTime,
    toggleShuffle,
    toggleRepeat,
    type RepeatMode
  } from '#lib/stores/player.svelte.js';
  import {
    createPlaybackController,
    type PlaybackController
  } from '#lib/stores/playback.svelte.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import { untrack } from 'svelte';
  import AlbumArt from './AlbumArt.svelte';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import MarqueeText from './MarqueeText.svelte';

  // 再生に使う2つのaudio要素（ギャップレス再生で、次の曲をもう一方に先読みする）
  let audioElement = $state<HTMLAudioElement>();
  let standbyAudioElement = $state<HTMLAudioElement>();
  let progressBar = $state<HTMLElement>();
  let volumeBar = $state<HTMLElement>();
  let isDraggingProgress = $state(false);
  let isDraggingVolume = $state(false);

  // 再生の制御（読み込み・キュー遷移・リピート・イコライザ・音量の正規化）はコントローラーに委ねる
  let playback: PlaybackController | null = null;

  // 再生の設定（設定ウィンドウで変えると`SettingsChanged`で更新され、再生中の曲にも反映される）
  const settingsQuery = useSettingsQuery();

  $effect(() => {
    if (!audioElement || !standbyAudioElement) return;
    const audios = [audioElement, standbyAudioElement] as const;
    // audio要素が変わったときだけ作り直す（作成中に読む再生状態には反応させない）
    const controller = untrack(() =>
      createPlaybackController(audios, {
        normalizationMode: () => settingsQuery.data?.volumeNormalization ?? 'off',
        gapless: () => settingsQuery.data?.gaplessPlayback ?? true,
        crossfadeSeconds: () => settingsQuery.data?.crossfadeSeconds ?? 0
      })
    );
    playback = controller;
    return () => {
      controller.destroy();
      playback = null;
    };
  });

  // 再生中トラックのアルバムアート
  const currentArtUrl = $derived(albumArtUrl(player.currentTrack?.id));

  /**
   * バー上のマウス位置を0〜1の割合に変換
   */
  function ratioAt(bar: HTMLElement, clientX: number): number {
    const rect = bar.getBoundingClientRect();
    return Math.max(0, Math.min((clientX - rect.left) / rect.width, 1));
  }

  /**
   * 進行バーの位置へシーク
   */
  function seekTo(clientX: number) {
    if (!progressBar || !player.duration) return;
    playback?.seek(ratioAt(progressBar, clientX) * player.duration);
  }

  function startDraggingProgress() {
    isDraggingProgress = true;
    playback?.setScrubbing(true);
  }

  function onDragProgress(event: MouseEvent) {
    if (isDraggingProgress) seekTo(event.clientX);
  }

  function stopDraggingProgress() {
    if (!isDraggingProgress) return;
    isDraggingProgress = false;
    playback?.setScrubbing(false);
  }

  /**
   * 音量バーの位置に音量を合わせる
   */
  function setVolumeAt(clientX: number) {
    if (volumeBar) player.volume = ratioAt(volumeBar, clientX);
  }

  function startDraggingVolume() {
    isDraggingVolume = true;
  }

  function onDragVolume(event: MouseEvent) {
    if (isDraggingVolume) setVolumeAt(event.clientX);
  }

  function stopDraggingVolume() {
    isDraggingVolume = false;
  }

  // ミュート解除時に戻す音量
  let previousVolume = 1;

  /**
   * ミュートを切り替え
   */
  function toggleMute() {
    if (player.volume > 0) {
      previousVolume = player.volume;
      player.volume = 0;
    } else {
      player.volume = previousVolume || 1;
    }
  }

  /**
   * リピートモードのアイコンを取得
   */
  function getRepeatIcon(mode: RepeatMode): string {
    switch (mode) {
      case 'off':
        return 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z';
      case 'all':
        return 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z';
      case 'one':
        return 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zM12 12v4h-1v-3h-1v-1h2z';
    }
  }

  /**
   * グローバルキーボードショートカットを処理
   */
  function handleGlobalKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
      return;
    }

    const withModifier = event.ctrlKey || event.metaKey;
    const plain = !event.ctrlKey && !event.metaKey && !event.altKey;

    switch (event.code) {
      case 'Space':
        event.preventDefault();
        playback?.togglePlayPause();
        break;
      case 'ArrowLeft':
        if (withModifier) {
          event.preventDefault();
          playback?.previous();
        }
        break;
      case 'ArrowRight':
        if (withModifier) {
          event.preventDefault();
          playback?.next();
        }
        break;
      case 'ArrowUp':
        if (withModifier) {
          event.preventDefault();
          player.volume = Math.min(1, player.volume + 0.1);
        }
        break;
      case 'ArrowDown':
        if (withModifier) {
          event.preventDefault();
          player.volume = Math.max(0, player.volume - 0.1);
        }
        break;
      case 'KeyM':
        if (plain) {
          event.preventDefault();
          toggleMute();
        }
        break;
      case 'KeyS':
        if (plain) {
          event.preventDefault();
          toggleShuffle();
        }
        break;
      case 'KeyR':
        if (plain) {
          event.preventDefault();
          toggleRepeat();
        }
        break;
    }
  }
</script>

<svelte:window
  onmousemove={(event) => {
    onDragProgress(event);
    onDragVolume(event);
  }}
  onmouseup={() => {
    stopDraggingProgress();
    stopDraggingVolume();
  }}
  onkeydown={handleGlobalKeydown}
/>

<!-- 非表示のオーディオ要素（イベントはPlaybackControllerが購読する） -->
<audio bind:this={audioElement} class="hidden"></audio>
<audio bind:this={standbyAudioElement} class="hidden"></audio>

<!-- プレイヤーUI -->
<div class="player-container">
  {#if player.currentTrack}
    <!-- トラック情報 -->
    <div class="flex items-center gap-3 min-w-0">
      <div class="album-art">
        <AlbumArt src={currentArtUrl} alt="アルバムアート" placeholderType="music" />
      </div>
      <div class="min-w-0">
        <MarqueeText
          text={player.currentTrack.title || player.currentTrack.fileName}
          class="text-sm font-semibold mb-0.5"
        />
        <MarqueeText
          text={`${player.currentTrack.artist || '不明なアーティスト'}${player.currentTrack.album ? ' • ' + player.currentTrack.album : ''}`}
          class="text-xs text-text-secondary"
        />
      </div>
    </div>

    <!-- 再生コントロール -->
    <div class="flex flex-col items-center gap-2">
      <div class="flex justify-center items-center gap-3">
        <button
          class="control-button"
          class:active={player.isShuffleEnabled}
          onclick={toggleShuffle}
          title="シャッフル (S)"
          aria-label="シャッフル"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path
              d="M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z"
            />
          </svg>
        </button>

        <button
          class="control-button"
          onclick={() => playback?.previous()}
          disabled={!player.hasPreviousTrack}
          title="前へ (Ctrl+←)"
          aria-label="前のトラック"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
          </svg>
        </button>

        <button
          class="play-pause-button"
          onclick={() => playback?.togglePlayPause()}
          title={player.isPlaying ? '一時停止 (Space)' : '再生 (Space)'}
          aria-label={player.isPlaying ? '一時停止' : '再生'}
        >
          {#if player.isPlaying}
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="28"
              height="28"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <rect x="6" y="4" width="4" height="16" />
              <rect x="14" y="4" width="4" height="16" />
            </svg>
          {:else}
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="28"
              height="28"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path d="M8 5v14l11-7z" />
            </svg>
          {/if}
        </button>

        <button
          class="control-button"
          onclick={() => playback?.next()}
          disabled={!player.hasNextTrack}
          title="次へ (Ctrl+→)"
          aria-label="次のトラック"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
          </svg>
        </button>

        <button
          class="control-button"
          class:active={player.repeatMode !== 'off'}
          onclick={toggleRepeat}
          title="リピート (R): {player.repeatMode === 'off'
            ? 'オフ'
            : player.repeatMode === 'all'
              ? '全曲'
              : '1曲'}"
          aria-label="リピート"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d={getRepeatIcon(player.repeatMode)} />
          </svg>
          {#if player.repeatMode === 'one'}
            <span class="absolute bottom-0.5 right-0.5 text-[0.5rem] font-bold">1</span>
          {/if}
        </button>
      </div>

      <!-- 進行バー -->
      <div class="flex items-center gap-2 w-full max-w-[600px]">
        <span class="text-[0.7rem] text-text-secondary min-w-[35px] text-center"
          >{formatTime(player.currentTime)}</span
        >
        <div
          bind:this={progressBar}
          class="progress-bar-base flex-1"
          role="slider"
          aria-label="再生位置"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={player.progress}
          tabindex="0"
          onclick={(e) => seekTo(e.clientX)}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft') {
              playback?.seekBy(-5);
            } else if (e.key === 'ArrowRight') {
              playback?.seekBy(5);
            }
          }}
          onmousedown={startDraggingProgress}
        >
          <div class="progress-fill-base" style="width: {player.progress}%"></div>
          <div class="progress-handle" style="left: {player.progress}%"></div>
        </div>
        <span class="text-[0.7rem] text-text-secondary min-w-[35px] text-center"
          >{formatTime(player.duration)}</span
        >
      </div>
    </div>

    <!-- 右側コントロール -->
    <div class="flex items-center justify-end gap-3">
      <div class="flex items-center gap-2">
        <button
          class="control-button small"
          onclick={toggleMute}
          title="ミュート (M)"
          aria-label="ミュート"
        >
          {#if player.volume === 0}
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path
                d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"
              />
            </svg>
          {:else if player.volume < 0.5}
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path
                d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"
              />
            </svg>
          {:else}
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path
                d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"
              />
            </svg>
          {/if}
        </button>
        <div
          bind:this={volumeBar}
          class="volume-bar"
          role="slider"
          aria-label="音量"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={player.volume * 100}
          tabindex="0"
          onclick={(e) => setVolumeAt(e.clientX)}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
              player.volume = Math.max(0, player.volume - 0.1);
            } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
              player.volume = Math.min(1, player.volume + 0.1);
            }
          }}
          onmousedown={startDraggingVolume}
        >
          <div class="progress-fill-base" style="width: {player.volume * 100}%"></div>
          <div class="progress-handle" style="left: {player.volume * 100}%"></div>
        </div>
      </div>
    </div>
  {:else}
    <div class="col-span-3 flex items-center justify-center gap-3 text-text-dimmed py-4">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="24"
        height="24"
        viewBox="0 0 24 24"
        fill="currentColor"
        class="opacity-50"
      >
        <path
          d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"
        />
      </svg>
      <span>トラックを選択して再生</span>
    </div>
  {/if}
</div>

<style>
  @reference "../../app.css";

  /* プレイヤーコンテナ */
  .player-container {
    @apply fixed bottom-0 left-0 right-0 z-50
           px-6 py-3 grid items-center gap-4
           border-t border-border text-text-primary
           grid-cols-(--grid-player) bg-linear-to-t from-base-100 to-base-300
           shadow-[0_-4px_20px_rgba(0,0,0,0.5)];
  }

  /* アルバムアート */
  .album-art {
    @apply w-14 h-14 rounded-md overflow-hidden shrink-0 bg-base-300;
  }

  /* コントロールボタン */
  .control-button {
    @apply bg-transparent border-none text-text-secondary cursor-pointer
           p-2 rounded-full transition-all duration-200
           flex items-center justify-center relative;
  }

  .control-button:hover {
    @apply text-text-primary bg-surface-active;
  }

  .control-button:active {
    @apply scale-95;
  }

  .control-button:disabled {
    @apply opacity-30 cursor-not-allowed;
  }

  .control-button:disabled:hover {
    @apply bg-transparent text-text-secondary;
  }

  .control-button.active {
    @apply text-secondary;
  }

  .control-button.small {
    @apply p-1.5;
  }

  /* 再生/一時停止ボタン */
  .play-pause-button {
    @apply bg-white text-black p-2.5 rounded-full cursor-pointer
           flex items-center justify-center transition-transform duration-150 border-none;
  }

  .play-pause-button:hover {
    @apply scale-105;
  }

  /* 音量バー */
  .volume-bar {
    @apply w-20 h-1 bg-progress-bg rounded-full relative cursor-pointer;
  }

  .volume-bar:hover .progress-fill-base {
    @apply bg-secondary;
  }

  .volume-bar:hover .progress-handle {
    @apply opacity-100;
  }
</style>
