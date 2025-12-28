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
    hasPreviousTrack
  } from '$lib/stores/player';
  import type { Track } from '$lib/types/models';

  let audioElement: HTMLAudioElement;
  let isDraggingProgress = false;
  let isDraggingVolume = false;

  // 現在のトラックが変更されたときに再生を開始
  $: if ($currentTrack && audioElement) {
    loadAndPlayTrack($currentTrack);
  }

  // 音量が変更されたときにオーディオ要素に反映
  $: if (audioElement) {
    audioElement.volume = $volume;
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

      // オーディオソースを設定
      audioElement.src = assetUrl;
      audioElement.load();

      // 再生を開始
      await audioElement.play();
      isPlaying.set(true);

      // 現在再生中のトラックをバックエンドに通知
      await invoke('set_current_track', { trackId: track.id });
    } catch (error) {
      console.error('トラックの再生に失敗しました:', error);
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
   * 停止
   */
  function stop() {
    if (!audioElement) return;

    audioElement.pause();
    audioElement.currentTime = 0;
    isPlaying.set(false);
    currentTime.set(0);
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

    // 次のトラックがあれば自動再生
    const hasNext = playNextTrack();
    if (!hasNext) {
      // キューの最後に到達した場合は停止
      resetPlayer();
    }
  }

  function handleError() {
    console.error('オーディオの再生エラーが発生しました');
    isPlaying.set(false);
  }

  /**
   * グローバルキーボードショートカットを処理
   */
  function handleGlobalKeydown(event: KeyboardEvent) {
    // 入力フィールドにフォーカスがある場合は無視
    const target = event.target as HTMLElement;
    if (
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable
    ) {
      return;
    }

    switch (event.code) {
      case 'Space':
        // スペースキー: 再生/一時停止
        event.preventDefault();
        togglePlayPause();
        break;
      case 'ArrowLeft':
        // Ctrl + 左矢印: 前のトラック
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          playPreviousTrack();
        }
        break;
      case 'ArrowRight':
        // Ctrl + 右矢印: 次のトラック
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          playNextTrack();
        }
        break;
      case 'ArrowUp':
        // Ctrl + 上矢印: 音量アップ
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          volume.set(Math.min(1, $volume + 0.1));
        }
        break;
      case 'ArrowDown':
        // Ctrl + 下矢印: 音量ダウン
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          volume.set(Math.max(0, $volume - 0.1));
        }
        break;
      case 'KeyM':
        // M キー: ミュート切り替え
        if (!event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          if ($volume > 0) {
            // 現在の音量を保存してミュート
            previousVolume = $volume;
            volume.set(0);
          } else {
            // 前の音量に戻す
            volume.set(previousVolume || 1);
          }
        }
        break;
    }
  }

  // ミュート切り替え用の前の音量を保持
  let previousVolume = 1;

  onMount(() => {
    // グローバルなマウスイベントリスナーを追加
    window.addEventListener('mousemove', onDragProgress);
    window.addEventListener('mouseup', stopDraggingProgress);
    window.addEventListener('mousemove', onDragVolume);
    window.addEventListener('mouseup', stopDraggingVolume);
    // グローバルキーボードショートカット
    window.addEventListener('keydown', handleGlobalKeydown);
  });

  onDestroy(() => {
    // イベントリスナーをクリーンアップ
    window.removeEventListener('mousemove', onDragProgress);
    window.removeEventListener('mouseup', stopDraggingProgress);
    window.removeEventListener('mousemove', onDragVolume);
    window.removeEventListener('mouseup', stopDraggingVolume);
    window.removeEventListener('keydown', handleGlobalKeydown);

    // プレイヤーをリセット
    resetPlayer();
  });
</script>

<!-- 非表示のオーディオ要素 -->
<audio
  bind:this={audioElement}
  on:timeupdate={handleTimeUpdate}
  on:loadedmetadata={handleLoadedMetadata}
  on:ended={handleEnded}
  on:error={handleError}
></audio>

<!-- プレイヤーUI -->
<div class="player-container">
  {#if $currentTrack}
    <!-- トラック情報 -->
    <div class="track-info">
      <div class="track-title">{$currentTrack.title || $currentTrack.fileName}</div>
      <div class="track-artist">
        {$currentTrack.artist || '不明なアーティスト'}
        {#if $currentTrack.album}
          - {$currentTrack.album}
        {/if}
      </div>
    </div>

    <!-- 再生コントロール -->
    <div class="controls">
      <button
        class="control-button"
        on:click={() => playPreviousTrack()}
        disabled={!$hasPreviousTrack}
        title="前へ"
        aria-label="前のトラック"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
        </svg>
      </button>

      <button class="control-button" on:click={stop} title="停止" aria-label="停止">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <rect x="6" y="6" width="12" height="12" />
        </svg>
      </button>

      <button
        class="control-button play-pause"
        on:click={togglePlayPause}
        title={$isPlaying ? '一時停止' : '再生'}
        aria-label={$isPlaying ? '一時停止' : '再生'}
      >
        {#if $isPlaying}
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="32"
            height="32"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <rect x="6" y="4" width="4" height="16" />
            <rect x="14" y="4" width="4" height="16" />
          </svg>
        {:else}
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="32"
            height="32"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M8 5v14l11-7z" />
          </svg>
        {/if}
      </button>

      <button
        class="control-button"
        on:click={() => playNextTrack()}
        disabled={!$hasNextTrack}
        title="次へ"
        aria-label="次のトラック"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
        </svg>
      </button>
    </div>

    <!-- 進行バー -->
    <div class="progress-section">
      <span class="time-display">{formatTime($currentTime)}</span>
      <div
        id="progress-bar"
        class="progress-bar"
        role="slider"
        aria-label="再生位置"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={$progress}
        tabindex="0"
        on:click={seekToPosition}
        on:keydown={(e) => {
          if (e.key === 'ArrowLeft') {
            audioElement.currentTime = Math.max(0, audioElement.currentTime - 5);
          } else if (e.key === 'ArrowRight') {
            audioElement.currentTime = Math.min($duration, audioElement.currentTime + 5);
          }
        }}
        on:mousedown={startDraggingProgress}
      >
        <div class="progress-fill" style="width: {$progress}%"></div>
        <div class="progress-handle" style="left: {$progress}%"></div>
      </div>
      <span class="time-display">{formatTime($duration)}</span>
    </div>

    <!-- 音量コントロール -->
    <div class="volume-section">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="currentColor"
      >
        <path
          d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"
        />
      </svg>
      <div
        id="volume-bar"
        class="volume-bar"
        role="slider"
        aria-label="音量"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={$volume * 100}
        tabindex="0"
        on:click={changeVolume}
        on:keydown={(e) => {
          if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
            volume.set(Math.max(0, $volume - 0.1));
          } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
            volume.set(Math.min(1, $volume + 0.1));
          }
        }}
        on:mousedown={startDraggingVolume}
      >
        <div class="volume-fill" style="width: {$volume * 100}%"></div>
        <div class="volume-handle" style="left: {$volume * 100}%"></div>
      </div>
      <span class="volume-display">{Math.round($volume * 100)}%</span>
    </div>
  {:else}
    <div class="no-track">トラックが選択されていません</div>
  {/if}
</div>

<style>
  .player-container {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    background: #1a1a1a;
    color: #ffffff;
    padding: 1rem;
    box-shadow: 0 -2px 10px rgba(0, 0, 0, 0.3);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    z-index: 1000;
  }

  .track-info {
    text-align: center;
  }

  .track-title {
    font-size: 1.1rem;
    font-weight: 600;
    margin-bottom: 0.25rem;
  }

  .track-artist {
    font-size: 0.9rem;
    color: #b3b3b3;
  }

  .controls {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 1rem;
  }

  .control-button {
    background: none;
    border: none;
    color: #ffffff;
    cursor: pointer;
    padding: 0.5rem;
    border-radius: 50%;
    transition:
      background-color 0.2s,
      transform 0.1s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .control-button:hover {
    background-color: rgba(255, 255, 255, 0.1);
  }

  .control-button:active {
    transform: scale(0.95);
  }

  .control-button:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .control-button:disabled:hover {
    background-color: transparent;
  }

  .control-button.play-pause {
    background-color: #1db954;
    padding: 0.75rem;
  }

  .control-button.play-pause:hover {
    background-color: #1ed760;
  }

  .progress-section {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .time-display {
    font-size: 0.85rem;
    color: #b3b3b3;
    min-width: 40px;
    text-align: center;
  }

  .progress-bar {
    flex: 1;
    height: 6px;
    background: #404040;
    border-radius: 3px;
    position: relative;
    cursor: pointer;
  }

  .progress-fill {
    height: 100%;
    background: #1db954;
    border-radius: 3px;
    transition: width 0.1s linear;
  }

  .progress-handle {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 12px;
    height: 12px;
    background: #ffffff;
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.2s;
  }

  .progress-bar:hover .progress-handle {
    opacity: 1;
  }

  .volume-section {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: center;
  }

  .volume-bar {
    width: 100px;
    height: 4px;
    background: #404040;
    border-radius: 2px;
    position: relative;
    cursor: pointer;
  }

  .volume-fill {
    height: 100%;
    background: #1db954;
    border-radius: 2px;
    transition: width 0.1s linear;
  }

  .volume-handle {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 10px;
    height: 10px;
    background: #ffffff;
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.2s;
  }

  .volume-bar:hover .volume-handle {
    opacity: 1;
  }

  .volume-display {
    font-size: 0.85rem;
    color: #b3b3b3;
    min-width: 40px;
  }

  .no-track {
    text-align: center;
    color: #b3b3b3;
    padding: 1rem;
  }

  audio {
    display: none;
  }
</style>
