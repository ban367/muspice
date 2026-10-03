/**
 * 再生コントローラー
 *
 * audio要素の操作（読み込み・再生・シーク・音量）と再生状態ストア（`./player`）の同期、
 * トラック終了時のキュー遷移、再生回数の記録、イコライザの接続をまとめて扱う。
 * Playerコンポーネントは表示と操作の受付だけを行い、再生の制御はここに委ねる。
 *
 * 次・前のトラックの決定はストアのキュー操作（`playNextTrack` / `playPreviousTrack`）が担う。
 * ストアは「再生中の曲をもう一度」の場合（1曲リピート、3秒以上再生中の「前へ」、
 * 1曲だけのキューの全曲リピート）に同じトラックを再設定するが、トラックIDが変わらないため
 * それだけでは再生し直されない。ここでその場合を検出して頭から再生し直す。
 */
import { get } from 'svelte/store';
import { convertFileSrc } from '@tauri-apps/api/core';
import { commands } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import { handleError } from './error.svelte.js';
import {
  cleanupEqualizer,
  initializeEqualizer,
  isEqualizerInitialized,
  resumeAudioContext
} from './equalizer.svelte.js';
import {
  currentTime,
  currentTrack,
  duration,
  isPlaying,
  playNextTrack,
  playPreviousTrack,
  resetPlayer,
  volume
} from './player';
import type { Track } from '#lib/types/models.js';

/** `MediaError.code`の値（Node環境のテストでも参照できるよう定数で持つ） */
const MEDIA_ERR_ABORTED = 1;
const MEDIA_ERR_NETWORK = 2;
const MEDIA_ERR_DECODE = 3;
const MEDIA_ERR_SRC_NOT_SUPPORTED = 4;

export interface PlaybackController {
  /** 再生/一時停止を切り替える */
  togglePlayPause(): Promise<void>;
  /** 指定位置（秒）へシークする */
  seek(time: number): void;
  /** 現在位置から相対的にシークする（秒） */
  seekBy(delta: number): void;
  /** 次のトラックへ進む */
  next(): void;
  /** 前のトラックへ戻る（3秒以上再生中なら再生中のトラックの頭へ） */
  previous(): void;
  /** シークバーをドラッグ中は、再生位置の通知でドラッグ位置を上書きしないようにする */
  setScrubbing(scrubbing: boolean): void;
  /** イベント購読とイコライザを解放し、再生状態をリセットする */
  destroy(): void;
}

/**
 * audio要素のエラーをユーザー向けメッセージにする
 * @returns 表示不要なエラー（再生の中断）の場合はnull
 */
export function mediaErrorMessage(error: Pick<MediaError, 'code'> | null): string | null {
  switch (error?.code) {
    case MEDIA_ERR_ABORTED:
      return null;
    case MEDIA_ERR_NETWORK:
      return 'ネットワークエラーが発生しました';
    case MEDIA_ERR_DECODE:
      return 'デコードエラー: ファイルが破損しているか未対応の形式です';
    case MEDIA_ERR_SRC_NOT_SUPPORTED:
      return '未対応のフォーマットか、ファイルが見つかりません';
    default:
      return '再生エラーが発生しました';
  }
}

/** トラック切り替えなどで再生が中断されたことによる拒否か */
function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

/**
 * audio要素を制御する再生コントローラーを作成する
 *
 * 作成した時点から`currentTrack`を購読し、トラックが変わるたびに読み込んで再生する。
 */
export function createPlaybackController(audio: HTMLAudioElement): PlaybackController {
  /** 最後に読み込みを始めたトラックのID（同じトラックの再設定で読み込み直さないため） */
  let loadedTrackId: string | null = null;
  let scrubbing = false;

  /** 再生を開始する（中断による拒否は無視し、それ以外はログに残す） */
  async function play(): Promise<void> {
    try {
      await audio.play();
    } catch (error) {
      if (!isAbortError(error)) {
        console.error('再生の開始に失敗しました:', error);
      }
    }
  }

  /** トラックを読み込んで再生する */
  async function load(track: Track): Promise<void> {
    // 前のトラックは切り替えを始めた時点で止める
    audio.pause();

    try {
      const filePath = await commands.getTrackFilePath(track.id);

      // パスの取得中に別のトラックへ切り替わっていたら何もしない
      if (get(currentTrack)?.id !== track.id) return;

      audio.src = convertFileSrc(filePath);

      // ユーザー操作の後にAudioContextを再開する（自動再生ポリシー対応）
      await resumeAudioContext();

      try {
        await audio.play();
      } catch (error) {
        if (isAbortError(error)) return;
        throw error;
      }

      // 現在再生中のトラックをバックエンドに通知し、再生回数を記録する
      await commands.setCurrentTrack(track.id);
      void incrementPlayCount(track.id);
    } catch (error) {
      handleError(error, 'トラックの再生に失敗しました');
      isPlaying.set(false);
    }
  }

  /** 再生中のトラックを頭から再生し直す */
  function restart(): void {
    audio.currentTime = 0;
    currentTime.set(0);
    void play();
  }

  /**
   * キューを移動する
   *
   * 移動先が再生中と同じトラックだった場合は、トラックIDが変わらず読み込みが
   * 走らないため、ここで頭から再生し直す。
   * @returns キューを移動できたか（ストアのキュー操作の戻り値）
   */
  function moveInQueue(move: () => boolean): boolean {
    const playingId = get(currentTrack)?.id;
    const moved = move();
    if (moved && playingId !== undefined && get(currentTrack)?.id === playingId) {
      restart();
    }
    return moved;
  }

  function seek(time: number): void {
    const max = Number.isFinite(audio.duration) ? audio.duration : time;
    const clamped = Math.max(0, Math.min(time, max));
    audio.currentTime = clamped;
    currentTime.set(clamped);
  }

  // ---------- audio要素のイベント ----------

  const listeners: Array<[keyof HTMLMediaElementEventMap, () => void]> = [
    ['play', () => isPlaying.set(true)],
    ['pause', () => isPlaying.set(false)],
    [
      'timeupdate',
      () => {
        if (!scrubbing) currentTime.set(audio.currentTime);
      }
    ],
    ['loadedmetadata', () => duration.set(audio.duration)],
    [
      'ended',
      () => {
        currentTime.set(0);
        // 次がなければ（リピートなしでキューの最後）再生を終える
        if (!moveInQueue(playNextTrack)) {
          resetPlayer();
        }
      }
    ],
    [
      'error',
      () => {
        const message = mediaErrorMessage(audio.error);
        if (message === null) return;
        console.error('オーディオの再生エラーが発生しました', {
          error: audio.error,
          src: audio.src
        });
        handleError(message);
        isPlaying.set(false);
      }
    ]
  ];
  for (const [type, listener] of listeners) {
    audio.addEventListener(type, listener);
  }

  // ---------- ストアの購読 ----------

  const unsubscribeTrack = currentTrack.subscribe((track) => {
    if (!track) {
      // キューが空になったら再生を止める
      loadedTrackId = null;
      audio.pause();
      return;
    }
    if (track.id === loadedTrackId) return;
    loadedTrackId = track.id;
    void load(track);
  });

  const unsubscribeVolume = volume.subscribe((value) => {
    audio.volume = value;
  });

  if (!isEqualizerInitialized()) {
    initializeEqualizer(audio).catch((error) => {
      console.error('イコライザの初期化に失敗しました:', error);
    });
  }

  return {
    async togglePlayPause() {
      if (!get(currentTrack)) return;
      if (audio.paused) {
        await play();
      } else {
        audio.pause();
      }
    },
    seek,
    seekBy(delta) {
      seek(audio.currentTime + delta);
    },
    next() {
      moveInQueue(playNextTrack);
    },
    previous() {
      moveInQueue(playPreviousTrack);
    },
    setScrubbing(value) {
      scrubbing = value;
    },
    destroy() {
      unsubscribeTrack();
      unsubscribeVolume();
      for (const [type, listener] of listeners) {
        audio.removeEventListener(type, listener);
      }
      resetPlayer();
      cleanupEqualizer().catch((error) => {
        console.error('イコライザのクリーンアップに失敗しました:', error);
      });
    }
  };
}
