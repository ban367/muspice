import { writable, derived, get, type Writable, type Readable } from 'svelte/store';
import type { Track } from '$lib/types/models';

/**
 * 再生状態を管理するストア
 */

// 現在再生中のトラック
export const currentTrack: Writable<Track | null> = writable(null);

// 再生中かどうか
export const isPlaying: Writable<boolean> = writable(false);

// 現在の再生位置（秒）
export const currentTime: Writable<number> = writable(0);

// トラックの総再生時間（秒）
export const duration: Writable<number> = writable(0);

// 音量（0.0 - 1.0）
export const volume: Writable<number> = writable(1.0);

// 現在の再生キュー（プレイリストまたはライブラリのトラックリスト）
export const playQueue: Writable<Track[]> = writable([]);

// 現在の再生キュー内のインデックス
export const currentTrackIndex: Writable<number> = writable(-1);

// 再生進行状況（0 - 100のパーセンテージ）
export const progress: Readable<number> = derived(
  [currentTime, duration],
  ([$currentTime, $duration]) => {
    if ($duration > 0) {
      return ($currentTime / $duration) * 100;
    }
    return 0;
  }
);

/**
 * 時間を mm:ss 形式にフォーマット
 */
export function formatTime(seconds: number): string {
  if (!seconds || isNaN(seconds)) {
    return '0:00';
  }

  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

/**
 * プレイヤーの状態をリセット
 */
export function resetPlayer(): void {
  currentTrack.set(null);
  isPlaying.set(false);
  currentTime.set(0);
  duration.set(0);
}

/**
 * 再生キューを設定してトラックを再生
 */
export function playTrackFromQueue(tracks: Track[], index: number): void {
  if (index < 0 || index >= tracks.length) {
    console.error('無効なトラックインデックス:', index);
    return;
  }

  playQueue.set(tracks);
  currentTrackIndex.set(index);
  currentTrack.set(tracks[index]);
}

/**
 * 単一のトラックを再生（キューをクリア）
 */
export function playSingleTrack(track: Track): void {
  playQueue.set([track]);
  currentTrackIndex.set(0);
  currentTrack.set(track);
}

/**
 * 次のトラックに進む
 */
export function playNextTrack(): boolean {
  const queue = get(playQueue);
  const currentIndex = get(currentTrackIndex);

  if (queue.length === 0) {
    return false;
  }

  const nextIndex = currentIndex + 1;

  if (nextIndex < queue.length) {
    currentTrackIndex.set(nextIndex);
    currentTrack.set(queue[nextIndex]);
    return true;
  }

  return false;
}

/**
 * 前のトラックに戻る
 */
export function playPreviousTrack(): boolean {
  const queue = get(playQueue);
  const currentIndex = get(currentTrackIndex);

  if (queue.length === 0) {
    return false;
  }

  const previousIndex = currentIndex - 1;

  if (previousIndex >= 0) {
    currentTrackIndex.set(previousIndex);
    currentTrack.set(queue[previousIndex]);
    return true;
  }

  return false;
}

/**
 * 次のトラックがあるかどうか
 */
export const hasNextTrack: Readable<boolean> = derived(
  [playQueue, currentTrackIndex],
  ([$playQueue, $currentTrackIndex]) => {
    return $currentTrackIndex < $playQueue.length - 1;
  }
);

/**
 * 前のトラックがあるかどうか
 */
export const hasPreviousTrack: Readable<boolean> = derived(
  [playQueue, currentTrackIndex],
  ([$playQueue, $currentTrackIndex]) => {
    return $currentTrackIndex > 0;
  }
);
