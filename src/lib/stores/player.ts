import { writable, derived, get, type Writable, type Readable } from 'svelte/store';
import type { Track } from '$lib/types/models';

/**
 * 再生状態を管理するストア
 */

// リピートモード
export type RepeatMode = 'off' | 'all' | 'one';

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

// 元の再生キュー（シャッフル前の順序を保持）
export const originalQueue: Writable<Track[]> = writable([]);

// 現在の再生キュー内のインデックス
export const currentTrackIndex: Writable<number> = writable(-1);

// シャッフルモード
export const isShuffleEnabled: Writable<boolean> = writable(false);

// リピートモード
export const repeatMode: Writable<RepeatMode> = writable('off');

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
  playQueue.set([]);
  originalQueue.set([]);
  currentTrackIndex.set(-1);
}

/**
 * 配列をシャッフル（Fisher-Yates アルゴリズム）
 */
function shuffleArray<T>(array: T[]): T[] {
  const shuffled = [...array];
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  return shuffled;
}

/**
 * シャッフルモードを切り替え
 */
export function toggleShuffle(): void {
  const shuffle = get(isShuffleEnabled);
  const queue = get(playQueue);
  const current = get(currentTrack);

  if (!shuffle) {
    // シャッフルを有効にする
    originalQueue.set([...queue]);

    if (current && queue.length > 0) {
      // 現在のトラックを除いてシャッフル
      const otherTracks = queue.filter((t) => t.id !== current.id);
      const shuffledOthers = shuffleArray(otherTracks);
      const newQueue = [current, ...shuffledOthers];
      playQueue.set(newQueue);
      currentTrackIndex.set(0);
    }
  } else {
    // シャッフルを無効にする（元の順序に戻す）
    const original = get(originalQueue);
    if (original.length > 0 && current) {
      const originalIndex = original.findIndex((t) => t.id === current.id);
      playQueue.set(original);
      currentTrackIndex.set(originalIndex !== -1 ? originalIndex : 0);
    }
  }

  isShuffleEnabled.set(!shuffle);
}

/**
 * リピートモードを切り替え
 */
export function toggleRepeat(): void {
  const current = get(repeatMode);
  const modes: RepeatMode[] = ['off', 'all', 'one'];
  const currentIndex = modes.indexOf(current);
  const nextIndex = (currentIndex + 1) % modes.length;
  repeatMode.set(modes[nextIndex]);
}

/**
 * 再生キューを設定してトラックを再生
 */
export function playTrackFromQueue(tracks: Track[], index: number): void {
  if (index < 0 || index >= tracks.length) {
    console.error('無効なトラックインデックス:', index);
    return;
  }

  const shuffle = get(isShuffleEnabled);

  if (shuffle) {
    // シャッフルモードの場合、選択したトラックを先頭にしてシャッフル
    const selectedTrack = tracks[index];
    const otherTracks = tracks.filter((_, i) => i !== index);
    const shuffledOthers = shuffleArray(otherTracks);
    const newQueue = [selectedTrack, ...shuffledOthers];

    originalQueue.set([...tracks]);
    playQueue.set(newQueue);
    currentTrackIndex.set(0);
    currentTrack.set(selectedTrack);
  } else {
    originalQueue.set([...tracks]);
    playQueue.set([...tracks]);
    currentTrackIndex.set(index);
    currentTrack.set(tracks[index]);
  }
}

/**
 * 単一のトラックを再生（キューをクリア）
 */
export function playSingleTrack(track: Track): void {
  originalQueue.set([track]);
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
  const repeat = get(repeatMode);

  if (queue.length === 0) {
    return false;
  }

  // 1曲リピートの場合は同じトラックを再生
  if (repeat === 'one') {
    currentTrack.set(queue[currentIndex]);
    return true;
  }

  const nextIndex = currentIndex + 1;

  if (nextIndex < queue.length) {
    currentTrackIndex.set(nextIndex);
    currentTrack.set(queue[nextIndex]);
    return true;
  }

  // 全曲リピートの場合は最初に戻る
  if (repeat === 'all') {
    currentTrackIndex.set(0);
    currentTrack.set(queue[0]);
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
  const repeat = get(repeatMode);
  const time = get(currentTime);

  if (queue.length === 0) {
    return false;
  }

  // 3秒以上再生している場合は最初に戻る
  if (time > 3) {
    currentTime.set(0);
    return true;
  }

  // 1曲リピートの場合は同じトラックを再生
  if (repeat === 'one') {
    currentTrack.set(queue[currentIndex]);
    return true;
  }

  const previousIndex = currentIndex - 1;

  if (previousIndex >= 0) {
    currentTrackIndex.set(previousIndex);
    currentTrack.set(queue[previousIndex]);
    return true;
  }

  // 全曲リピートの場合は最後に移動
  if (repeat === 'all') {
    const lastIndex = queue.length - 1;
    currentTrackIndex.set(lastIndex);
    currentTrack.set(queue[lastIndex]);
    return true;
  }

  return false;
}

/**
 * キューから特定のトラックを削除
 */
export function removeFromQueue(trackId: string): void {
  const queue = get(playQueue);
  const original = get(originalQueue);
  const currentIndex = get(currentTrackIndex);
  const current = get(currentTrack);

  const newQueue = queue.filter((t) => t.id !== trackId);
  const newOriginal = original.filter((t) => t.id !== trackId);

  playQueue.set(newQueue);
  originalQueue.set(newOriginal);

  // 現在再生中のトラックが削除された場合
  if (current?.id === trackId) {
    if (newQueue.length > 0) {
      const newIndex = Math.min(currentIndex, newQueue.length - 1);
      currentTrackIndex.set(newIndex);
      currentTrack.set(newQueue[newIndex]);
    } else {
      resetPlayer();
    }
  } else {
    // インデックスを調整
    const newIndex = newQueue.findIndex((t) => t.id === current?.id);
    if (newIndex !== -1) {
      currentTrackIndex.set(newIndex);
    }
  }
}

/**
 * キューをクリア（現在再生中のトラックは残す）
 */
export function clearQueue(): void {
  const current = get(currentTrack);

  if (current) {
    playQueue.set([current]);
    originalQueue.set([current]);
    currentTrackIndex.set(0);
  } else {
    resetPlayer();
  }
}

/**
 * 次のトラックがあるかどうか
 */
export const hasNextTrack: Readable<boolean> = derived(
  [playQueue, currentTrackIndex, repeatMode],
  ([$playQueue, $currentTrackIndex, $repeatMode]) => {
    if ($repeatMode === 'all' || $repeatMode === 'one') {
      return $playQueue.length > 0;
    }
    return $currentTrackIndex < $playQueue.length - 1;
  }
);

/**
 * 前のトラックがあるかどうか
 */
export const hasPreviousTrack: Readable<boolean> = derived(
  [playQueue, currentTrackIndex, repeatMode],
  ([$playQueue, $currentTrackIndex, $repeatMode]) => {
    if ($repeatMode === 'all' || $repeatMode === 'one') {
      return $playQueue.length > 0;
    }
    return $currentTrackIndex > 0;
  }
);

/**
 * 残りのキュー（現在のトラック以降）
 */
export const upcomingTracks: Readable<Track[]> = derived(
  [playQueue, currentTrackIndex],
  ([$playQueue, $currentTrackIndex]) => {
    if ($currentTrackIndex < 0 || $playQueue.length === 0) {
      return [];
    }
    return $playQueue.slice($currentTrackIndex + 1);
  }
);
