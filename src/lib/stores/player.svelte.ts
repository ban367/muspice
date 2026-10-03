import type { Track } from '#lib/types/models.js';

/**
 * 再生状態
 *
 * `player`の各プロパティを直接読み書きする（例: `player.volume = 0.5`）。
 * キューの変更（再生するトラックの決定）は、下のキュー操作の関数を使う。
 * audio要素との同期は再生コントローラー（`./playback.svelte.ts`）が担う。
 */

// リピートモード
export type RepeatMode = 'off' | 'all' | 'one';

class Player {
  /** 現在再生中のトラック */
  currentTrack = $state.raw<Track | null>(null);

  /** 再生中かどうか（audio要素のplay/pauseイベントから更新する） */
  isPlaying = $state(false);

  /** 現在の再生位置（秒） */
  currentTime = $state(0);

  /** トラックの総再生時間（秒） */
  duration = $state(0);

  /** 音量（0.0 - 1.0） */
  volume = $state(1.0);

  /** 現在の再生キュー（プレイリストまたはライブラリのトラックリスト）。配列ごと置き換える */
  playQueue = $state.raw<Track[]>([]);

  /** 元の再生キュー（シャッフル前の順序を保持）。配列ごと置き換える */
  originalQueue = $state.raw<Track[]>([]);

  /** 現在の再生キュー内のインデックス */
  currentTrackIndex = $state(-1);

  /** シャッフルモード */
  isShuffleEnabled = $state(false);

  /** リピートモード */
  repeatMode = $state<RepeatMode>('off');

  /** 再生進行状況（0 - 100のパーセンテージ） */
  readonly progress = $derived(this.duration > 0 ? (this.currentTime / this.duration) * 100 : 0);

  /** 次のトラックがあるかどうか */
  readonly hasNextTrack = $derived(
    this.repeatMode === 'all' || this.repeatMode === 'one'
      ? this.playQueue.length > 0
      : this.currentTrackIndex < this.playQueue.length - 1
  );

  /** 前のトラックがあるかどうか */
  readonly hasPreviousTrack = $derived(
    this.repeatMode === 'all' || this.repeatMode === 'one'
      ? this.playQueue.length > 0
      : this.currentTrackIndex > 0
  );

  /** 残りのキュー（現在のトラック以降） */
  readonly upcomingTracks = $derived(
    this.currentTrackIndex < 0 || this.playQueue.length === 0
      ? []
      : this.playQueue.slice(this.currentTrackIndex + 1)
  );
}

export const player = new Player();

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
  player.currentTrack = null;
  player.isPlaying = false;
  player.currentTime = 0;
  player.duration = 0;
  player.playQueue = [];
  player.originalQueue = [];
  player.currentTrackIndex = -1;
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
  const shuffle = player.isShuffleEnabled;
  const queue = player.playQueue;
  const current = player.currentTrack;

  if (!shuffle) {
    // シャッフルを有効にする
    player.originalQueue = [...queue];

    if (current && queue.length > 0) {
      // 現在のトラックを除いてシャッフル
      const otherTracks = queue.filter((t) => t.id !== current.id);
      const shuffledOthers = shuffleArray(otherTracks);
      const newQueue = [current, ...shuffledOthers];
      player.playQueue = newQueue;
      player.currentTrackIndex = 0;
    }
  } else {
    // シャッフルを無効にする（元の順序に戻す）
    const original = player.originalQueue;
    if (original.length > 0 && current) {
      const originalIndex = original.findIndex((t) => t.id === current.id);
      player.playQueue = original;
      player.currentTrackIndex = originalIndex !== -1 ? originalIndex : 0;
    }
  }

  player.isShuffleEnabled = !shuffle;
}

/**
 * リピートモードを切り替え
 */
export function toggleRepeat(): void {
  const current = player.repeatMode;
  const modes: RepeatMode[] = ['off', 'all', 'one'];
  const currentIndex = modes.indexOf(current);
  const nextIndex = (currentIndex + 1) % modes.length;
  player.repeatMode = modes[nextIndex];
}

/**
 * 再生キューを設定してトラックを再生
 */
export function playTrackFromQueue(tracks: Track[], index: number): void {
  if (index < 0 || index >= tracks.length) {
    console.error('無効なトラックインデックス:', index);
    return;
  }

  const shuffle = player.isShuffleEnabled;

  if (shuffle) {
    // シャッフルモードの場合、選択したトラックを先頭にしてシャッフル
    const selectedTrack = tracks[index];
    const otherTracks = tracks.filter((_, i) => i !== index);
    const shuffledOthers = shuffleArray(otherTracks);
    const newQueue = [selectedTrack, ...shuffledOthers];

    player.originalQueue = [...tracks];
    player.playQueue = newQueue;
    player.currentTrackIndex = 0;
    player.currentTrack = selectedTrack;
  } else {
    player.originalQueue = [...tracks];
    player.playQueue = [...tracks];
    player.currentTrackIndex = index;
    player.currentTrack = tracks[index];
  }
}

/**
 * トラック一覧をシャッフル再生
 *
 * シャッフルモードを有効にして、ランダムに選んだトラックから再生する（残りはFisher-Yatesで
 * 並べ替えるため、並び全体が均等にランダムになる）。元の順序は保持し、シャッフルを解除すると戻る。
 */
export function playShuffled(tracks: Track[]): void {
  if (tracks.length === 0) return;
  player.isShuffleEnabled = true;
  playTrackFromQueue(tracks, Math.floor(Math.random() * tracks.length));
}

/**
 * 単一のトラックを再生（キューをクリア）
 */
export function playSingleTrack(track: Track): void {
  player.originalQueue = [track];
  player.playQueue = [track];
  player.currentTrackIndex = 0;
  player.currentTrack = track;
}

/**
 * 次のトラックに進む
 */
export function playNextTrack(): boolean {
  const queue = player.playQueue;
  const currentIndex = player.currentTrackIndex;
  const repeat = player.repeatMode;

  if (queue.length === 0) {
    return false;
  }

  // 1曲リピートの場合は同じトラックを再生
  if (repeat === 'one') {
    player.currentTrack = queue[currentIndex];
    return true;
  }

  const nextIndex = currentIndex + 1;

  if (nextIndex < queue.length) {
    player.currentTrackIndex = nextIndex;
    player.currentTrack = queue[nextIndex];
    return true;
  }

  // 全曲リピートの場合は最初に戻る
  if (repeat === 'all') {
    player.currentTrackIndex = 0;
    player.currentTrack = queue[0];
    return true;
  }

  return false;
}

/**
 * 前のトラックに戻る
 */
export function playPreviousTrack(): boolean {
  const queue = player.playQueue;
  const currentIndex = player.currentTrackIndex;
  const repeat = player.repeatMode;
  const time = player.currentTime;

  if (queue.length === 0) {
    return false;
  }

  // 3秒以上再生している場合は最初に戻る
  if (time > 3) {
    player.currentTime = 0;
    return true;
  }

  // 1曲リピートの場合は同じトラックを再生
  if (repeat === 'one') {
    player.currentTrack = queue[currentIndex];
    return true;
  }

  const previousIndex = currentIndex - 1;

  if (previousIndex >= 0) {
    player.currentTrackIndex = previousIndex;
    player.currentTrack = queue[previousIndex];
    return true;
  }

  // 全曲リピートの場合は最後に移動
  if (repeat === 'all') {
    const lastIndex = queue.length - 1;
    player.currentTrackIndex = lastIndex;
    player.currentTrack = queue[lastIndex];
    return true;
  }

  return false;
}

/**
 * キューから特定のトラックを削除
 */
export function removeFromQueue(trackId: string): void {
  const queue = player.playQueue;
  const original = player.originalQueue;
  const currentIndex = player.currentTrackIndex;
  const current = player.currentTrack;

  const newQueue = queue.filter((t) => t.id !== trackId);
  const newOriginal = original.filter((t) => t.id !== trackId);

  player.playQueue = newQueue;
  player.originalQueue = newOriginal;

  // 現在再生中のトラックが削除された場合
  if (current?.id === trackId) {
    if (newQueue.length > 0) {
      const newIndex = Math.min(currentIndex, newQueue.length - 1);
      player.currentTrackIndex = newIndex;
      player.currentTrack = newQueue[newIndex];
    } else {
      resetPlayer();
    }
  } else {
    // インデックスを調整
    const newIndex = newQueue.findIndex((t) => t.id === current?.id);
    if (newIndex !== -1) {
      player.currentTrackIndex = newIndex;
    }
  }
}

/**
 * キューをクリア（現在再生中のトラックは残す）
 */
export function clearQueue(): void {
  const current = player.currentTrack;

  if (current) {
    player.playQueue = [current];
    player.originalQueue = [current];
    player.currentTrackIndex = 0;
  } else {
    resetPlayer();
  }
}
