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
    upcomingTracks,
    playQueue,
    currentTrackIndex,
    removeFromQueue,
    clearQueue,
    type RepeatMode
  } from '$lib/stores/player';
  import type { Track, AlbumArt } from '$lib/types/models';
  import { handleError as reportError } from '$lib/stores/error';
  import { incrementPlayCount } from '$lib/queries/tracks';

  let audioElement: HTMLAudioElement;
  let isDraggingProgress = $state(false);
  let isDraggingVolume = $state(false);
  let lastPlayedTrackId = $state<string | null>(null);
  let showQueue = $state(false);
  
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
      const art = await invoke<AlbumArt | null>('get_album_art', { trackId });
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

    // リピートモードに応じて処理
    if ($repeatMode === 'one') {
      // 1曲リピート: 同じトラックを再生
      if (audioElement) {
        audioElement.currentTime = 0;
        audioElement.play().then(() => {
          isPlaying.set(true);
        }).catch(console.error);
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
    if (
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable
    ) {
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
      case 'KeyQ':
        if (!event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          showQueue = !showQueue;
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
  on:timeupdate={handleTimeUpdate}
  on:loadedmetadata={handleLoadedMetadata}
  on:ended={handleEnded}
  on:error={handleError}
></audio>

<!-- プレイヤーUI -->
<div class="player-container">
  {#if $currentTrack}
    <!-- トラック情報 -->
    <div class="track-section">
      <div class="album-art">
        {#if albumArtUrl}
          <img src={albumArtUrl} alt="アルバムアート" />
        {:else}
          <div class="album-art-placeholder">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
            </svg>
          </div>
        {/if}
      </div>
      <div class="track-info">
        <div class="track-title">{$currentTrack.title || $currentTrack.fileName}</div>
        <div class="track-artist">
          {$currentTrack.artist || '不明なアーティスト'}
          {#if $currentTrack.album}
            • {$currentTrack.album}
          {/if}
        </div>
      </div>
    </div>

    <!-- 再生コントロール -->
    <div class="controls-section">
      <div class="control-buttons">
        <button
          class="control-button"
          class:active={$isShuffleEnabled}
          on:click={toggleShuffle}
          title="シャッフル (S)"
          aria-label="シャッフル"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <path d="M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z"/>
          </svg>
        </button>

        <button
          class="control-button"
          on:click={() => playPreviousTrack()}
          disabled={!$hasPreviousTrack}
          title="前へ (Ctrl+←)"
          aria-label="前のトラック"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
          </svg>
        </button>

        <button
          class="control-button play-pause"
          on:click={togglePlayPause}
          title={$isPlaying ? '一時停止 (Space)' : '再生 (Space)'}
          aria-label={$isPlaying ? '一時停止' : '再生'}
        >
          {#if $isPlaying}
            <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="4" width="4" height="16" />
              <rect x="14" y="4" width="4" height="16" />
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
          {/if}
        </button>

        <button
          class="control-button"
          on:click={() => playNextTrack()}
          disabled={!$hasNextTrack}
          title="次へ (Ctrl+→)"
          aria-label="次のトラック"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
          </svg>
        </button>

        <button
          class="control-button"
          class:active={$repeatMode !== 'off'}
          on:click={toggleRepeat}
          title="リピート (R): {$repeatMode === 'off' ? 'オフ' : $repeatMode === 'all' ? '全曲' : '1曲'}"
          aria-label="リピート"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <path d={getRepeatIcon($repeatMode)} />
          </svg>
          {#if $repeatMode === 'one'}
            <span class="repeat-one-indicator">1</span>
          {/if}
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
    </div>

    <!-- 右側コントロール -->
    <div class="extra-controls">
      <button
        class="control-button small"
        class:active={showQueue}
        on:click={() => showQueue = !showQueue}
        title="再生キュー (Q)"
        aria-label="再生キュー"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
          <path d="M4 6h16M4 10h16M4 14h10M4 18h7" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/>
        </svg>
      </button>

      <div class="volume-section">
        <button
          class="control-button small"
          on:click={() => {
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
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
              <path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/>
            </svg>
          {:else if $volume < 0.5}
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"/>
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/>
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
      </div>
    </div>
  {:else}
    <div class="no-track">
      <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="currentColor" class="no-track-icon">
        <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
      </svg>
      <span>トラックを選択して再生</span>
    </div>
  {/if}
</div>

<!-- 再生キューパネル -->
{#if showQueue && $currentTrack}
  <div class="queue-panel">
    <div class="queue-header">
      <h3>再生キュー</h3>
      <div class="queue-actions">
        <button class="btn-clear-queue" on:click={clearQueue}>クリア</button>
        <button class="btn-close-queue" on:click={() => showQueue = false}>✕</button>
      </div>
    </div>
    <div class="queue-now-playing">
      <div class="queue-label">再生中</div>
      <div class="queue-track current">
        <span class="queue-track-title">{$currentTrack.title || $currentTrack.fileName}</span>
        <span class="queue-track-artist">{$currentTrack.artist || '不明なアーティスト'}</span>
      </div>
    </div>
    {#if $upcomingTracks.length > 0}
      <div class="queue-upcoming">
        <div class="queue-label">次に再生 ({$upcomingTracks.length}曲)</div>
        <div class="queue-list">
          {#each $upcomingTracks as track, index (track.id)}
            <div class="queue-track">
              <span class="queue-track-number">{index + 1}</span>
              <div class="queue-track-info">
                <span class="queue-track-title">{track.title || track.fileName}</span>
                <span class="queue-track-artist">{track.artist || '不明なアーティスト'}</span>
              </div>
              <button
                class="btn-remove-from-queue"
                on:click={() => removeFromQueue(track.id)}
                title="キューから削除"
              >
                ✕
              </button>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="queue-empty">キューに他のトラックはありません</div>
    {/if}
  </div>
{/if}

<style>
  .player-container {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    background: linear-gradient(to top, #0a0a12 0%, #12121e 100%);
    color: #ffffff;
    padding: 0.75rem 1.5rem;
    box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.5);
    display: grid;
    grid-template-columns: 1fr 2fr 1fr;
    gap: 1rem;
    align-items: center;
    z-index: 1000;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }

  /* トラック情報セクション */
  .track-section {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-width: 0;
  }

  .album-art {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 0.375rem;
    overflow: hidden;
    flex-shrink: 0;
    background-color: #1e1e2e;
  }

  .album-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .album-art-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: rgba(255, 255, 255, 0.8);
  }

  .album-art-placeholder svg {
    width: 50%;
    height: 50%;
  }

  .track-info {
    min-width: 0;
  }

  .track-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 0.125rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-artist {
    font-size: 0.75rem;
    color: #b3b3b3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* コントロールセクション */
  .controls-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }

  .control-buttons {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 0.75rem;
  }

  .control-button {
    background: none;
    border: none;
    color: #b3b3b3;
    cursor: pointer;
    padding: 0.5rem;
    border-radius: 50%;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }

  .control-button:hover {
    color: #fff;
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
    color: #b3b3b3;
  }

  .control-button.active {
    color: #1db954;
  }

  .control-button.play-pause {
    background-color: #fff;
    color: #000;
    padding: 0.625rem;
  }

  .control-button.play-pause:hover {
    transform: scale(1.05);
    background-color: #fff;
    color: #000;
  }

  .control-button.small {
    padding: 0.375rem;
  }

  .repeat-one-indicator {
    position: absolute;
    bottom: 2px;
    right: 2px;
    font-size: 0.5rem;
    font-weight: bold;
  }

  .progress-section {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    max-width: 600px;
  }

  .time-display {
    font-size: 0.7rem;
    color: #b3b3b3;
    min-width: 35px;
    text-align: center;
  }

  .progress-bar {
    flex: 1;
    height: 4px;
    background: #404040;
    border-radius: 2px;
    position: relative;
    cursor: pointer;
  }

  .progress-bar:hover {
    height: 6px;
  }

  .progress-fill {
    height: 100%;
    background: #fff;
    border-radius: 2px;
    transition: width 0.1s linear;
  }

  .progress-bar:hover .progress-fill {
    background: #1db954;
  }

  .progress-handle {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 12px;
    height: 12px;
    background: #fff;
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.2s;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
  }

  .progress-bar:hover .progress-handle {
    opacity: 1;
  }

  /* 右側コントロール */
  .extra-controls {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.75rem;
  }

  .volume-section {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .volume-bar {
    width: 80px;
    height: 4px;
    background: #404040;
    border-radius: 2px;
    position: relative;
    cursor: pointer;
  }

  .volume-fill {
    height: 100%;
    background: #fff;
    border-radius: 2px;
    transition: width 0.1s linear;
  }

  .volume-bar:hover .volume-fill {
    background: #1db954;
  }

  .volume-handle {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 10px;
    height: 10px;
    background: #fff;
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.2s;
  }

  .volume-bar:hover .volume-handle {
    opacity: 1;
  }

  .no-track {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    color: #666;
    padding: 1rem;
  }

  .no-track-icon {
    opacity: 0.5;
  }

  audio {
    display: none;
  }

  /* 再生キューパネル */
  .queue-panel {
    position: fixed;
    bottom: 5.5rem;
    right: 1rem;
    width: 320px;
    max-height: 400px;
    background: #1e1e2e;
    border: 1px solid #333;
    border-radius: 0.5rem;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
    z-index: 1001;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: slideUp 0.2s ease-out;
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .queue-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid #333;
  }

  .queue-header h3 {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
  }

  .queue-actions {
    display: flex;
    gap: 0.5rem;
  }

  .btn-clear-queue {
    padding: 0.25rem 0.5rem;
    background: transparent;
    border: 1px solid #444;
    border-radius: 0.25rem;
    color: #aaa;
    font-size: 0.75rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-clear-queue:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .btn-close-queue {
    background: transparent;
    border: none;
    color: #666;
    font-size: 1rem;
    cursor: pointer;
    padding: 0.25rem;
  }

  .btn-close-queue:hover {
    color: #fff;
  }

  .queue-now-playing {
    padding: 0.75rem 1rem;
    background: rgba(29, 185, 84, 0.1);
    border-bottom: 1px solid #333;
  }

  .queue-label {
    font-size: 0.625rem;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
    margin-bottom: 0.375rem;
  }

  .queue-track {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    border-radius: 0.25rem;
    transition: background-color 0.2s;
  }

  .queue-track:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .queue-track.current {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.125rem;
    padding: 0;
  }

  .queue-track-number {
    font-size: 0.75rem;
    color: #666;
    width: 1.5rem;
    text-align: center;
  }

  .queue-track-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .queue-track-title {
    font-size: 0.8rem;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .queue-track-artist {
    font-size: 0.7rem;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-remove-from-queue {
    background: transparent;
    border: none;
    color: #666;
    font-size: 0.75rem;
    cursor: pointer;
    padding: 0.25rem;
    opacity: 0;
    transition: all 0.2s;
  }

  .queue-track:hover .btn-remove-from-queue {
    opacity: 1;
  }

  .btn-remove-from-queue:hover {
    color: #ef4444;
  }

  .queue-upcoming {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .queue-upcoming .queue-label {
    padding: 0.75rem 1rem 0.375rem;
  }

  .queue-list {
    flex: 1;
    overflow-y: auto;
    padding: 0 0.5rem 0.5rem;
  }

  .queue-empty {
    padding: 2rem 1rem;
    text-align: center;
    color: #666;
    font-size: 0.875rem;
  }

  /* スクロールバー */
  .queue-list::-webkit-scrollbar {
    width: 6px;
  }

  .queue-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .queue-list::-webkit-scrollbar-thumb {
    background-color: rgba(255, 255, 255, 0.2);
    border-radius: 3px;
  }
</style>
