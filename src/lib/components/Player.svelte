<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import {
    currentTrack,
    isPlaying,
    currentTime,
    duration,
    volume,
    progress,
    formatTime,
    resetPlayer,
    playNextTrack,
    playPreviousTrack,
    hasNextTrack,
    hasPreviousTrack,
    isShuffleEnabled,
    repeatMode,
    toggleShuffle,
    toggleRepeat,
    type RepeatMode
  } from '$lib/stores/player';
  import type { Track, AlbumArt as AlbumArtType } from '$lib/types/models';
  import { handleError as reportError } from '$lib/stores/error';
  import AlbumArt from './AlbumArt.svelte';
  import { incrementPlayCount } from '$lib/queries/tracks';
  import MarqueeText from './MarqueeText.svelte';

  let audioElement: HTMLAudioElement;
  let isDraggingProgress = $state(false);
  let isDraggingVolume = $state(false);
  let lastPlayedTrackId = $state<string | null>(null);

  // アルバムアートキャッシュ
  let albumArtUrl = $state<string | null>(null);

  // 現在のトラックが変更されたときに再生を開始
  $effect(() => {
    if ($currentTrack && audioElement) {
      if ($currentTrack.id !== lastPlayedTrackId) {
        lastPlayedTrackId = $currentTrack.id;
        loadAndPlayTrack($currentTrack);
        loadAlbumArt($currentTrack.id);
      }
    } else if (!$currentTrack) {
      lastPlayedTrackId = null;
      albumArtUrl = null;
    }
  });

  // 音量が変更されたときにオーディオ要素に反映
  $effect(() => {
    if (audioElement) {
      audioElement.volume = $volume;
    }
  });

  /**
   * アルバムアートを取得
   */
  async function loadAlbumArt(trackId: string) {
    try {
      const art = await invoke<AlbumArtType | null>('get_album_art', { trackId });
      if (art) {
        albumArtUrl = `data:${art.mimeType};base64,${art.data}`;
      } else {
        albumArtUrl = null;
      }
    } catch {
      albumArtUrl = null;
    }
  }

  /**
   * トラックを読み込んで再生
   */
  async function loadAndPlayTrack(track: Track) {
    try {
      // トラックのファイルパスを取得
      const filePath = await invoke<string>('get_track_file_path', {
        trackId: track.id
      });

      // Tauriのファイルパスを変換
      const assetUrl = convertFileSrc(filePath);

      // トラックが変更されている可能性があるため確認
      if ($currentTrack?.id !== track.id) {
        return;
      }

      // オーディオソースを設定
      audioElement.src = assetUrl;

      // 再生を開始
      try {
        await audioElement.play();
        isPlaying.set(true);
      } catch (playError) {
        // AbortErrorは無視
        if (playError instanceof DOMException && playError.name === 'AbortError') {
          return;
        }
        throw playError;
      }

      // 現在再生中のトラックをバックエンドに通知
      await invoke('set_current_track', { trackId: track.id });

      // 再生回数をインクリメント
      incrementPlayCount(track.id);
    } catch (error) {
      console.error('トラックの再生に失敗しました:', error);
      reportError(error, 'トラックの再生に失敗しました');
      isPlaying.set(false);
    }
  }

  /**
   * 再生/一時停止を切り替え
   */
  async function togglePlayPause() {
    if (!audioElement || !$currentTrack) return;

    try {
      if ($isPlaying) {
        audioElement.pause();
        isPlaying.set(false);
      } else {
        await audioElement.play();
        isPlaying.set(true);
      }
    } catch (error) {
      console.error('再生/一時停止の切り替えに失敗しました:', error);
    }
  }

  /**
   * 進行バーをクリックしてシーク
   */
  function seekToPosition(event: MouseEvent) {
    if (!audioElement || !$duration) return;

    const progressBar = event.currentTarget as HTMLElement;
    const rect = progressBar.getBoundingClientRect();
    const clickX = event.clientX - rect.left;
    const percentage = clickX / rect.width;
    const newTime = percentage * $duration;

    audioElement.currentTime = newTime;
    currentTime.set(newTime);
  }

  /**
   * 進行バーのドラッグ開始
   */
  function startDraggingProgress() {
    isDraggingProgress = true;
  }

  /**
   * 進行バーのドラッグ中
   */
  function onDragProgress(event: MouseEvent) {
    if (!isDraggingProgress || !audioElement || !$duration) return;

    const progressBar = document.getElementById('progress-bar');
    if (!progressBar) return;

    const rect = progressBar.getBoundingClientRect();
    const clickX = Math.max(0, Math.min(event.clientX - rect.left, rect.width));
    const percentage = clickX / rect.width;
    const newTime = percentage * $duration;

    audioElement.currentTime = newTime;
    currentTime.set(newTime);
  }

  /**
   * 進行バーのドラッグ終了
   */
  function stopDraggingProgress() {
    isDraggingProgress = false;
  }

  /**
   * 音量バーをクリックして音量変更
   */
  function changeVolume(event: MouseEvent) {
    const volumeBar = event.currentTarget as HTMLElement;
    const rect = volumeBar.getBoundingClientRect();
    const clickX = event.clientX - rect.left;
    const percentage = Math.max(0, Math.min(clickX / rect.width, 1));

    volume.set(percentage);
  }

  /**
   * 音量バーのドラッグ開始
   */
  function startDraggingVolume() {
    isDraggingVolume = true;
  }

  /**
   * 音量バーのドラッグ中
   */
  function onDragVolume(event: MouseEvent) {
    if (!isDraggingVolume) return;

    const volumeBar = document.getElementById('volume-bar');
    if (!volumeBar) return;

    const rect = volumeBar.getBoundingClientRect();
    const clickX = Math.max(0, Math.min(event.clientX - rect.left, rect.width));
    const percentage = clickX / rect.width;

    volume.set(percentage);
  }

  /**
   * 音量バーのドラッグ終了
   */
  function stopDraggingVolume() {
    isDraggingVolume = false;
  }

  /**
   * オーディオ要素のイベントハンドラー
   */
  function handleTimeUpdate() {
    if (!isDraggingProgress && audioElement) {
      currentTime.set(audioElement.currentTime);
    }
  }

  function handleLoadedMetadata() {
    if (audioElement) {
      duration.set(audioElement.duration);
    }
  }

  function handleEnded() {
    isPlaying.set(false);
    currentTime.set(0);

    // リピートモードに応じて処理
    if ($repeatMode === 'one') {
      // 1曲リピート: 同じトラックを再生
      if (audioElement) {
        audioElement.currentTime = 0;
        audioElement
          .play()
          .then(() => {
            isPlaying.set(true);
          })
          .catch(console.error);
      }
    } else {
      // 次のトラックがあれば自動再生
      const hasNext = playNextTrack();
      if (!hasNext) {
        resetPlayer();
      }
    }
  }

  function handleError(e: Event) {
    const target = e.target as HTMLAudioElement;
    console.error('オーディオの再生エラーが発生しました', {
      error: target.error,
      src: target.src
    });

    let errorMessage = '再生エラーが発生しました';
    if (target.error) {
      switch (target.error.code) {
        case target.error.MEDIA_ERR_ABORTED:
          return;
        case target.error.MEDIA_ERR_NETWORK:
          errorMessage = 'ネットワークエラーが発生しました';
          break;
        case target.error.MEDIA_ERR_DECODE:
          errorMessage = 'デコードエラー: ファイルが破損しているか未対応の形式です';
          break;
        case target.error.MEDIA_ERR_SRC_NOT_SUPPORTED:
          errorMessage = '未対応のフォーマットか、ファイルが見つかりません';
          break;
      }
    }

    reportError(errorMessage);
    isPlaying.set(false);
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

    switch (event.code) {
      case 'Space':
        event.preventDefault();
        togglePlayPause();
        break;
      case 'ArrowLeft':
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          playPreviousTrack();
        }
        break;
      case 'ArrowRight':
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          playNextTrack();
        }
        break;
      case 'ArrowUp':
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          volume.set(Math.min(1, $volume + 0.1));
        }
        break;
      case 'ArrowDown':
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          volume.set(Math.max(0, $volume - 0.1));
        }
        break;
      case 'KeyM':
        if (!event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          if ($volume > 0) {
            previousVolume = $volume;
            volume.set(0);
          } else {
            volume.set(previousVolume || 1);
          }
        }
        break;
      case 'KeyS':
        if (!event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          toggleShuffle();
        }
        break;
      case 'KeyR':
        if (!event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          toggleRepeat();
        }
        break;
    }
  }

  let previousVolume = 1;

  onMount(() => {
    window.addEventListener('mousemove', onDragProgress);
    window.addEventListener('mouseup', stopDraggingProgress);
    window.addEventListener('mousemove', onDragVolume);
    window.addEventListener('mouseup', stopDraggingVolume);
    window.addEventListener('keydown', handleGlobalKeydown);
  });

  onDestroy(() => {
    window.removeEventListener('mousemove', onDragProgress);
    window.removeEventListener('mouseup', stopDraggingProgress);
    window.removeEventListener('mousemove', onDragVolume);
    window.removeEventListener('mouseup', stopDraggingVolume);
    window.removeEventListener('keydown', handleGlobalKeydown);
    resetPlayer();
  });
</script>

<!-- 非表示のオーディオ要素 -->
<audio
  bind:this={audioElement}
  ontimeupdate={handleTimeUpdate}
  onloadedmetadata={handleLoadedMetadata}
  onended={handleEnded}
  onerror={handleError}
  class="hidden"
></audio>

<!-- プレイヤーUI -->
<div class="player-container">
  {#if $currentTrack}
    <!-- トラック情報 -->
    <div class="flex items-center gap-3 min-w-0">
      <div class="album-art">
        <AlbumArt src={albumArtUrl} alt="アルバムアート" placeholderType="music" />
      </div>
      <div class="min-w-0">
        <MarqueeText
          text={$currentTrack.title || $currentTrack.fileName}
          class="text-sm font-semibold mb-0.5"
        />
        <MarqueeText
          text={`${$currentTrack.artist || '不明なアーティスト'}${$currentTrack.album ? ' • ' + $currentTrack.album : ''}`}
          class="text-xs text-text-secondary"
        />
      </div>
    </div>

    <!-- 再生コントロール -->
    <div class="flex flex-col items-center gap-2">
      <div class="flex justify-center items-center gap-3">
        <button
          class="control-button"
          class:active={$isShuffleEnabled}
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
          onclick={() => playPreviousTrack()}
          disabled={!$hasPreviousTrack}
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
          onclick={togglePlayPause}
          title={$isPlaying ? '一時停止 (Space)' : '再生 (Space)'}
          aria-label={$isPlaying ? '一時停止' : '再生'}
        >
          {#if $isPlaying}
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
          onclick={() => playNextTrack()}
          disabled={!$hasNextTrack}
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
          class:active={$repeatMode !== 'off'}
          onclick={toggleRepeat}
          title="リピート (R): {$repeatMode === 'off'
            ? 'オフ'
            : $repeatMode === 'all'
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
            <path d={getRepeatIcon($repeatMode)} />
          </svg>
          {#if $repeatMode === 'one'}
            <span class="absolute bottom-0.5 right-0.5 text-[0.5rem] font-bold">1</span>
          {/if}
        </button>
      </div>

      <!-- 進行バー -->
      <div class="flex items-center gap-2 w-full max-w-[600px]">
        <span class="text-[0.7rem] text-text-secondary min-w-[35px] text-center"
          >{formatTime($currentTime)}</span
        >
        <div
          id="progress-bar"
          class="progress-bar-base flex-1"
          role="slider"
          aria-label="再生位置"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={$progress}
          tabindex="0"
          onclick={seekToPosition}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft') {
              audioElement.currentTime = Math.max(0, audioElement.currentTime - 5);
            } else if (e.key === 'ArrowRight') {
              audioElement.currentTime = Math.min($duration, audioElement.currentTime + 5);
            }
          }}
          onmousedown={startDraggingProgress}
        >
          <div class="progress-fill-base" style="width: {$progress}%"></div>
          <div class="progress-handle" style="left: {$progress}%"></div>
        </div>
        <span class="text-[0.7rem] text-text-secondary min-w-[35px] text-center"
          >{formatTime($duration)}</span
        >
      </div>
    </div>

    <!-- 右側コントロール -->
    <div class="flex items-center justify-end gap-3">
      <div class="flex items-center gap-2">
        <button
          class="control-button small"
          onclick={() => {
            if ($volume > 0) {
              previousVolume = $volume;
              volume.set(0);
            } else {
              volume.set(previousVolume || 1);
            }
          }}
          title="ミュート (M)"
          aria-label="ミュート"
        >
          {#if $volume === 0}
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
          {:else if $volume < 0.5}
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
          id="volume-bar"
          class="volume-bar"
          role="slider"
          aria-label="音量"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={$volume * 100}
          tabindex="0"
          onclick={changeVolume}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
              volume.set(Math.max(0, $volume - 0.1));
            } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
              volume.set(Math.min(1, $volume + 0.1));
            }
          }}
          onmousedown={startDraggingVolume}
        >
          <div class="progress-fill-base" style="width: {$volume * 100}%"></div>
          <div class="progress-handle" style="left: {$volume * 100}%"></div>
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

  .album-art-placeholder {
    @apply w-full h-full flex items-center justify-center text-white/80
           bg-linear-to-br from-accent to-accent-focus;
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
