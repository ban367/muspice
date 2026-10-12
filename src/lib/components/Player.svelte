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
  import { useQueryClient } from '@tanstack/svelte-query';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import { watchAutoDj } from '#lib/stores/autoDj.svelte.js';
  import PlaybackAidsMenu from './PlaybackAidsMenu.svelte';
  import { miniPlayer } from '#lib/stores/miniPlayer.svelte.js';
  import { useFavoriteTracksQuery } from '#lib/queries/tracks.js';
  import { events, type PlaybackControl } from '#lib/bindings.js';
  import { onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { ui } from '#lib/stores/ui.svelte.js';
  import AlbumArt from './AlbumArt.svelte';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import MarqueeText from './MarqueeText.svelte';
  import FavoriteButton from './library/FavoriteButton.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  let progressBar = $state<HTMLElement>();
  let volumeBar = $state<HTMLElement>();
  let isDraggingProgress = $state(false);
  let isDraggingVolume = $state(false);

  // 再生の制御（エンジンの操作・キュー遷移・リピート・イコライザの設定の送信）はコントローラーに委ねる
  let playback: PlaybackController | null = null;

  // 再生の設定（設定ウィンドウで変えると`SettingsChanged`で更新される。音量の正規化・クロスフェード・
  // 出力デバイスは、保存した時点でRust側が再生エンジンへ伝える）
  const settingsQuery = useSettingsQuery();
  // 再生回数・スキップ回数を記録した時に、曲の一覧・再生履歴のキャッシュへ反映する
  const queryClient = useQueryClient();

  onMount(() => {
    const controller = createPlaybackController({
      gapless: () => settingsQuery.data?.gaplessPlayback ?? true,
      crossfadeSeconds: () => settingsQuery.data?.crossfadeSeconds ?? 0,
      queryClient
    });
    playback = controller;
    // Auto DJ（再生キューの最後の曲になったら、曲を足す）
    const stopAutoDj = watchAutoDj({
      enabled: () => settingsQuery.data?.autoDj ?? false,
      playlistId: () => settingsQuery.data?.autoDjPlaylistId ?? null,
      queryClient
    });

    // メニューバーの「再生」メニューと、OSのメディアキー・コントロールセンターなどからの操作
    const listening = events.playbackControl.listen((event) =>
      handlePlaybackControl(event.payload)
    );
    listening.catch((error) => console.error('再生の操作を購読できません:', error));

    return () => {
      void listening.then((unlisten) => unlisten()).catch(() => {});
      stopAutoDj();
      controller.destroy();
      playback = null;
    };
  });

  // 再生中トラックのアルバムアート
  const currentArtUrl = $derived(albumArtUrl(player.currentTrack?.id));

  // 再生中の曲がお気に入りかどうか。再生キューの曲は、キューに入れた時点の内容のため、
  // お気に入りの一覧（変更のたびに取り直される）から調べる
  const favoritesQuery = useFavoriteTracksQuery();
  const favoriteIds = $derived(new Set((favoritesQuery.data ?? []).map((track) => track.id)));

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

  /** キー操作・メニューで、音量を1段階変える大きさ */
  const VOLUME_STEP = 0.1;

  /**
   * 音量を、今の値から変える（0〜1に収める）
   */
  function changeVolume(delta: number) {
    player.volume = Math.max(0, Math.min(1, player.volume + delta));
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
   * メニューバーの「再生」メニューと、OSのメディアキー・コントロールセンターなどからの操作を行う
   * （ウィンドウの中のキー操作と同じ動き）
   */
  function handlePlaybackControl(control: PlaybackControl) {
    switch (control.type) {
      case 'toggle':
        void playback?.togglePlayPause();
        break;
      case 'play':
        if (!player.isPlaying) void playback?.togglePlayPause();
        break;
      case 'pause':
        if (player.isPlaying) void playback?.togglePlayPause();
        break;
      case 'next':
        playback?.next();
        break;
      case 'previous':
        playback?.previous();
        break;
      case 'seek':
        if (control.position !== null) playback?.seek(control.position);
        break;
      case 'volumeUp':
        changeVolume(VOLUME_STEP);
        break;
      case 'volumeDown':
        changeVolume(-VOLUME_STEP);
        break;
      case 'toggleMute':
        toggleMute();
        break;
      case 'toggleShuffle':
        toggleShuffle();
        break;
      case 'toggleRepeat':
        toggleRepeat();
        break;
    }
  }

  /**
   * 再生中の曲を、一覧の中で表示する（その行までスクロールして選ぶ）
   *
   * 開いている画面の一覧にその曲があればそこで、なければ全曲の一覧へ移動して表示する。
   */
  async function revealCurrentTrack() {
    const track = player.currentTrack;
    if (!track) return;
    // Now Playingの画面を開いていたら、一覧が見えるように閉じる
    ui.isNowPlayingOpen = false;
    ui.revealTrackId = track.id;
    // 開いている画面の一覧（`TrackList`）が、その曲を見つければ受け取る
    await tick();
    if (ui.revealTrackId !== null && page.url.pathname !== '/library/songs') {
      await goto(resolve('library/songs'));
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
          changeVolume(VOLUME_STEP);
        }
        break;
      case 'ArrowDown':
        if (withModifier) {
          event.preventDefault();
          changeVolume(-VOLUME_STEP);
        }
        break;
      case 'KeyL':
        if (withModifier) {
          event.preventDefault();
          void revealCurrentTrack();
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

<!-- プレイヤーUI -->
<div class="player-container">
  {#if player.currentTrack}
    <!-- トラック情報 -->
    <div class="flex items-center gap-3 min-w-0">
      <!-- アルバムアートのクリックで、Now Playingの画面（大きなアルバムアートと歌詞）を開閉する -->
      <button
        type="button"
        class="album-art"
        onclick={() => (ui.isNowPlayingOpen = !ui.isNowPlayingOpen)}
        title={ui.isNowPlayingOpen ? m.nowPlaying.close : m.nowPlaying.open}
        aria-label={ui.isNowPlayingOpen ? m.nowPlaying.close : m.nowPlaying.open}
        aria-pressed={ui.isNowPlayingOpen}
      >
        <AlbumArt src={currentArtUrl} alt={m.common.albumArt} placeholderType="music" />
      </button>
      <!-- 曲の情報のクリックで、再生中の曲を一覧で表示する -->
      <button
        type="button"
        class="track-info"
        onclick={revealCurrentTrack}
        title={m.player.revealCurrentTrack}
      >
        <div class="min-w-0">
          <MarqueeText
            text={player.currentTrack.title || player.currentTrack.fileName}
            class="text-sm font-semibold mb-0.5"
          />
          <MarqueeText
            text={`${player.currentTrack.artist || m.common.unknownArtist}${player.currentTrack.album ? ' • ' + player.currentTrack.album : ''}`}
            class="text-xs text-text-secondary"
          />
        </div>
      </button>
      <FavoriteButton
        trackId={player.currentTrack.id}
        isFavorite={favoriteIds.has(player.currentTrack.id)}
        size="medium"
      />
    </div>

    <!-- 再生コントロール -->
    <div class="flex flex-col items-center gap-2">
      <div class="flex justify-center items-center gap-3">
        <button
          class="control-button"
          class:active={player.isShuffleEnabled}
          onclick={toggleShuffle}
          title={m.player.shuffleTitle}
          aria-label={m.player.shuffle}
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
          title={m.player.previousTitle}
          aria-label={m.player.previous}
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
          title={player.isPlaying ? m.player.pauseTitle : m.player.playTitle}
          aria-label={player.isPlaying ? m.player.pause : m.player.play}
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
          title={m.player.nextTitle}
          aria-label={m.player.next}
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
          title={m.player.repeatTitle(m.player.repeatModes[player.repeatMode])}
          aria-label={m.player.repeat}
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
          aria-label={m.player.seek}
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
      <PlaybackAidsMenu />
      <button
        class="control-button small"
        onclick={() => miniPlayer.toggle()}
        title={m.miniPlayer.enter}
        aria-label={m.miniPlayer.enter}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1zm8 7h7v5h-7z"
          />
        </svg>
      </button>
      <div class="flex items-center gap-2">
        <button
          class="control-button small"
          onclick={toggleMute}
          title={m.player.muteTitle}
          aria-label={m.player.mute}
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
          aria-label={m.player.volume}
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={player.volume * 100}
          tabindex="0"
          onclick={(e) => setVolumeAt(e.clientX)}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
              changeVolume(-VOLUME_STEP);
            } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
              changeVolume(VOLUME_STEP);
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
      <span>{m.player.noTrack}</span>
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
  /* 曲の情報（クリックで、再生中の曲を一覧で表示する） */
  .track-info {
    @apply flex items-center gap-3 min-w-0 p-0 bg-transparent border-none text-left text-inherit cursor-pointer rounded-md;
  }

  .album-art {
    @apply w-14 h-14 p-0 border-none rounded-md overflow-hidden shrink-0 bg-base-300 cursor-pointer transition-opacity;
  }

  .album-art:hover {
    @apply opacity-80;
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

  /* 再生/一時停止ボタン（背景と反対の色: ダークでは白地に黒、ライトでは黒地に白） */
  .play-pause-button {
    @apply bg-text-primary text-base-100 p-2.5 rounded-full cursor-pointer
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
