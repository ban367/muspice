import type { Track } from '#lib/types/models.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { matchQueueEntries, toQueueEntries, type QueueEntry } from '#lib/utils/playQueue.js';
import { showWarning } from './error.svelte.js';

/**
 * 再生状態
 *
 * `player`の各プロパティを直接読み書きする（例: `player.volume = 0.5`）。
 * キューの変更（再生するトラックの決定）は、下のキュー操作の関数を使う。
 * 再生エンジンとの同期は再生コントローラー（`./playback.svelte.ts`）が担う。
 *
 * キューは、利用者が消す操作（キューからの削除・クリア・「キューを置き換えて再生」）でだけ
 * 曲が外れる。曲を再生する操作（一覧のダブルクリック・「再生」）は、再生中の曲の次に曲を入れて
 * 再生する（`playNow`）。キューの最後まで再生しても、キューと最後の曲は残る。
 *
 * キューには同じ曲を何度でも入れられるため、キューの曲は要素（`QueueEntry`）ごとのID
 * （エントリID）か位置で扱う（曲のIDでは、同じ曲の別の要素と見分けられない）。
 */

// リピートモード
export type RepeatMode = 'off' | 'all' | 'one';

class Player {
  /** 現在再生中のトラック */
  currentTrack = $state.raw<Track | null>(null);

  /** 再生中かどうか（再生コントローラーが、再生エンジンの操作の結果と通知から更新する） */
  isPlaying = $state(false);

  /** 現在の再生位置（秒） */
  currentTime = $state(0);

  /** トラックの総再生時間（秒） */
  duration = $state(0);

  /** 音量（0.0 - 1.0） */
  volume = $state(1.0);

  /** 現在の再生キュー（再生する順）。配列ごと置き換える */
  playQueue = $state.raw<QueueEntry[]>([]);

  /**
   * シャッフルする前の順の再生キュー。配列ごと置き換える
   *
   * シャッフル中は、`playQueue`と同じ要素をシャッフルする前の順に並べる（キューへの追加・削除は
   * 両方に行う）。シャッフルを解除すると、この順に戻す。
   */
  originalQueue = $state.raw<QueueEntry[]>([]);

  /** 現在の再生キュー内のインデックス */
  currentTrackIndex = $state(-1);

  /**
   * 「この曲を再生する」操作（一覧・キューから曲を選ぶ）の回数
   *
   * 再生コントローラーが、再生を始めずに置いてあった曲（起動して復元した曲など）を選び直された
   * ことに気付くために読む（同じ曲を選び直しても、`currentTrack`のIDは変わらないため）。
   */
  playRequestCount = $state(0);

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

  /**
   * 「次に再生」のキュー（再生中のトラックより後ろの要素）
   *
   * 何も再生していない間（何も再生していない時に、キューへ追加した曲）は、キューのすべて
   * （「次へ」・再生で、先頭から再生する）。
   */
  readonly upcomingEntries = $derived(
    this.currentTrackIndex < 0 ? this.playQueue : this.playQueue.slice(this.currentTrackIndex + 1)
  );
}

export const player = new Player();

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
 * 保存してあった再生キューを復元する（前回の終了時の状態。再生は始めない）
 *
 * キューは曲のIDの並びで保存してあるため、要素（エントリID）はここで振り直す。シャッフルする前の
 * 順は、同じ曲の要素へ前から順に対応付ける。
 * @param queue - 再生する順のキュー
 * @param originalQueue - シャッフルする前の順のキュー（シャッフルが無効なら`queue`と同じ）
 * @param index - `queue`の中の、再生していたトラックの位置（nullは、何も再生していなかった。
 *   キューへ入れただけの曲は、すべて「次に再生」に戻る）
 */
export function restoreQueue(
  queue: readonly Track[],
  originalQueue: readonly Track[],
  index: number | null
): void {
  if (queue.length === 0) return;
  if (index !== null && (!Number.isInteger(index) || index < 0 || index >= queue.length)) return;
  const current = index === null ? null : queue[index];
  const entries = toQueueEntries(queue);
  player.originalQueue = matchQueueEntries(originalQueue, entries);
  player.playQueue = entries;
  player.currentTrackIndex = index ?? -1;
  player.currentTrack = current;
  player.isPlaying = false;
  player.currentTime = 0;
  player.duration = current?.duration ?? 0;
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

/** キューの中の、要素の位置（ない場合は-1） */
function indexOfEntry(queue: readonly QueueEntry[], entryId: number): number {
  return queue.findIndex((entry) => entry.entryId === entryId);
}

/**
 * キューの`position`の位置に要素を入れた、新しいキューを返す
 *
 * 件数が多い場合があるため、引数の数に上限のあるspliceへ展開せずに組み立てる。
 */
function insertEntries(
  queue: readonly QueueEntry[],
  position: number,
  entries: readonly QueueEntry[]
): QueueEntry[] {
  return [...queue.slice(0, position), ...entries, ...queue.slice(position)];
}

/**
 * シャッフルモードを切り替え
 */
export function toggleShuffle(): void {
  const shuffle = player.isShuffleEnabled;
  const queue = player.playQueue;
  const currentIndex = player.currentTrackIndex;
  const current: QueueEntry | undefined = queue[currentIndex];

  if (!shuffle) {
    // シャッフルを有効にする
    player.originalQueue = [...queue];

    if (current) {
      // 再生中の要素を先頭にして、残りを並べ替える（同じ曲の別の要素も残す）
      const others = queue.filter((_, index) => index !== currentIndex);
      player.playQueue = [current, ...shuffleArray(others)];
      player.currentTrackIndex = 0;
    } else {
      // 何も再生していない（キューへ追加しただけ）: キュー全体を並べ替える
      player.playQueue = shuffleArray(queue);
    }
  } else {
    // シャッフルを無効にする（元の順序に戻す）
    const original = player.originalQueue;
    if (original.length > 0 && !current) {
      player.playQueue = original;
    } else if (original.length > 0 && current) {
      const originalIndex = indexOfEntry(original, current.entryId);
      if (originalIndex !== -1) {
        player.playQueue = original;
        player.currentTrackIndex = originalIndex;
      } else {
        // 元の順に再生中の要素がない（古い版で保存したキューなど）: 再生中の位置へ入れ、
        // 「次へ」でその後ろから続ける
        const position = Math.min(currentIndex, original.length);
        player.playQueue = insertEntries(original, position, [current]);
        player.originalQueue = player.playQueue;
        player.currentTrackIndex = position;
      }
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

/** ファイルが見つかるトラックだけを返す（見つからない曲は再生できないため、キューに入れない） */
function playableTracks(tracks: readonly Track[]): Track[] {
  return tracks.filter((track) => !track.isMissing);
}

/**
 * 再生キューを置き換えて、トラックを再生（「キューを置き換えて再生」）
 *
 * それまでのキューは外れる。ファイルが見つからない曲はキューに入れない。選んだ曲のファイルが
 * 見つからない場合は、再生せずに通知する（キューも変えない）。
 * @param allTracks - キューにするトラック（表示順）
 * @param selectedIndex - その中の、再生するトラックの位置
 */
export function replaceQueueAndPlay(allTracks: readonly Track[], selectedIndex = 0): void {
  if (selectedIndex < 0 || selectedIndex >= allTracks.length) {
    console.error('無効なトラックインデックス:', selectedIndex);
    return;
  }
  const selected = allTracks[selectedIndex];
  if (selected.isMissing) {
    showWarning(m.notices.trackFileMissing);
    return;
  }

  const tracks = playableTracks(allTracks);
  // 見つからない曲を除いた後の、選んだ曲の位置（同じ曲が複数あっても、選んだ位置のもの）
  const index = playableTracks(allTracks.slice(0, selectedIndex)).length;
  const entries = toQueueEntries(tracks);
  const shuffle = player.isShuffleEnabled;

  player.originalQueue = entries;
  if (shuffle) {
    // シャッフルモードの場合、選択したトラックを先頭にしてシャッフル
    const others = entries.filter((_, i) => i !== index);
    player.playQueue = [entries[index], ...shuffleArray(others)];
    player.currentTrackIndex = 0;
  } else {
    player.playQueue = entries;
    player.currentTrackIndex = index;
  }
  player.currentTrack = selected;
  player.playRequestCount++;
}

/**
 * 再生中の要素の次に要素を入れる（何も再生していなければ、キューの先頭）
 *
 * シャッフルする前の順でも、再生中の要素の次に入れる（シャッフルを解除しても残るように。
 * 元の順に再生中の要素がない場合は最後）。
 * @param entries - シャッフルする前の順に入れる要素（渡された順）
 * @param ordered - 再生する順に入れる要素（`entries`と同じ要素を、再生する順に並べたもの）
 * @returns 再生する順のキューの中の、入れた要素の先頭の位置
 */
function insertNext(
  entries: readonly QueueEntry[],
  ordered: readonly QueueEntry[] = entries
): number {
  const queue = player.playQueue;
  const position = player.currentTrackIndex + 1;
  const current: QueueEntry | undefined = queue[player.currentTrackIndex];
  const original = player.originalQueue;
  let originalPosition = 0;
  if (current) {
    const originalIndex = indexOfEntry(original, current.entryId);
    originalPosition = originalIndex === -1 ? original.length : originalIndex + 1;
  }

  player.playQueue = insertEntries(queue, position, ordered);
  player.originalQueue = insertEntries(original, originalPosition, entries);
  return position;
}

/**
 * 再生中の曲の次にトラックを入れて、そのまま再生する（「次に追加して再生」。一覧のダブルクリック・
 * 「再生」のボタン）
 *
 * キューのほかの曲は残す（入れた曲を再生し終えたら、それまでの「次に再生」の曲へ続く）。何も
 * 再生していなければ、キューの先頭に入れて再生する。ファイルが見つからない曲は入れない（どれも
 * 見つからない場合は、再生せずに通知する）。
 * シャッフル中は、最初の曲から再生し、残りをランダムな順に入れる。
 * @param tracks - 入れるトラック（表示順。最初の曲から再生する）
 * @param options.shuffle - すべてをランダムな順に入れる（シャッフル再生。シャッフルモードは変えない）
 */
export function playNow(tracks: readonly Track[], options: { shuffle?: boolean } = {}): void {
  const playable = playableTracks(tracks);
  if (playable.length === 0) {
    if (tracks.length > 0) showWarning(m.notices.trackFileMissing);
    return;
  }

  const entries = toQueueEntries(playable);
  let ordered = entries;
  if (options.shuffle) {
    ordered = shuffleArray(entries);
  } else if (player.isShuffleEnabled) {
    ordered = [entries[0], ...shuffleArray(entries.slice(1))];
  }
  const position = insertNext(entries, ordered);
  player.currentTrackIndex = position;
  player.currentTrack = ordered[0].track;
  player.playRequestCount++;
}

/**
 * トラック一覧をシャッフル再生
 *
 * ランダムな順で再生中の曲の次に入れ、そのまま再生する（Fisher-Yatesで並べ替えるため、並び全体が
 * 均等にランダムになる）。キューのほかの曲は残し、シャッフルモードも変えない。
 */
export function playShuffled(tracks: readonly Track[]): void {
  playNow(tracks, { shuffle: true });
}

/**
 * 次へ進んだ場合のキュー内の位置（進めない場合は-1）
 */
function nextTrackIndex(): number {
  const queue = player.playQueue;
  const currentIndex = player.currentTrackIndex;

  if (queue.length === 0) {
    return -1;
  }

  // 1曲リピートの場合は同じトラックを再生
  if (player.repeatMode === 'one') {
    return currentIndex >= 0 && currentIndex < queue.length ? currentIndex : -1;
  }

  const nextIndex = currentIndex + 1;
  if (nextIndex < queue.length) {
    return nextIndex;
  }

  // 全曲リピートの場合は最初に戻る
  return player.repeatMode === 'all' ? 0 : -1;
}

/**
 * 次に再生するトラック（`playNextTrack`で進む先）を、状態を変えずに返す
 *
 * ギャップレス再生で次の曲を先読みするために使う。1曲リピートでは再生中のトラックを返す。
 * @returns 次がない（リピートなしでキューの最後）場合はnull
 */
export function peekNextTrack(): Track | null {
  const index = nextTrackIndex();
  return index < 0 ? null : player.playQueue[index].track;
}

/**
 * 次のトラックに進む
 */
export function playNextTrack(): boolean {
  const index = nextTrackIndex();
  if (index < 0) {
    return false;
  }
  player.currentTrackIndex = index;
  player.currentTrack = player.playQueue[index].track;
  return true;
}

/**
 * キュー内の位置を指定して、そのトラックから再生する
 *
 * キューの並びは変えず、再生位置だけを移す（指定した位置より前のトラックもキューに残る）。
 * @param index - `player.playQueue`内の位置
 * @returns 再生位置を移せたか（範囲外の場合はfalse）
 */
export function playQueueIndex(index: number): boolean {
  const queue = player.playQueue;
  if (!Number.isInteger(index) || index < 0 || index >= queue.length) {
    return false;
  }
  player.currentTrackIndex = index;
  player.currentTrack = queue[index].track;
  player.playRequestCount++;
  return true;
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
    player.currentTrack = queue[currentIndex]?.track ?? null;
    return true;
  }

  const previousIndex = currentIndex - 1;

  if (previousIndex >= 0) {
    player.currentTrackIndex = previousIndex;
    player.currentTrack = queue[previousIndex].track;
    return true;
  }

  // 全曲リピートの場合は最後に移動
  if (repeat === 'all') {
    const lastIndex = queue.length - 1;
    player.currentTrackIndex = lastIndex;
    player.currentTrack = queue[lastIndex].track;
    return true;
  }

  return false;
}

/**
 * キューの最後にトラックを追加（「キューの末尾に追加」。ファイルが見つからない曲は入れない）
 *
 * シャッフルする前の順の最後にも入れる（シャッフルを解除しても残るように）。
 */
export function addToQueue(tracks: readonly Track[]): void {
  const entries = toQueueEntries(playableTracks(tracks));
  if (entries.length === 0) return;
  player.playQueue = [...player.playQueue, ...entries];
  player.originalQueue = [...player.originalQueue, ...entries];
}

/**
 * 再生中のトラックの次にトラックを追加（「次に追加」。ファイルが見つからない曲は入れない）
 *
 * 何も再生していなければ、キューの先頭に入れる。シャッフル中も、渡した順のまま入れる。
 */
export function addNextInQueue(tracks: readonly Track[]): void {
  const entries = toQueueEntries(playableTracks(tracks));
  if (entries.length === 0) return;
  insertNext(entries);
}

/**
 * 「次に再生」の曲を、別の曲の位置へ動かす（再生中の曲と、それより前の曲は動かさない）
 *
 * 動かした曲が、落とした曲の位置に来る（下へ動かす時は落とした曲の後ろ、上へ動かす時は前）。
 * 位置ではなく要素で指定するため、ドラッグしている間に曲が進んでも、ドラッグした曲を動かす。
 * シャッフルする前の順（`originalQueue`）は変えない。
 * @param entryId - 動かす曲の要素のID
 * @param targetEntryId - 落とした曲の要素のID
 */
export function moveUpcomingEntry(entryId: number, targetEntryId: number): void {
  const queue = player.playQueue;
  const start = player.currentTrackIndex + 1;
  const from = indexOfEntry(queue, entryId);
  const to = indexOfEntry(queue, targetEntryId);
  // 「次に再生」にない要素（再生し終えた・再生中・キューから消えた）は動かさない
  // （何も再生していない間は、キューのすべてが「次に再生」）
  if (from < start || to < start || from === to) return;

  const next = [...queue];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  player.playQueue = next;
}

/**
 * キューから要素を削除（同じ曲の別の要素は残す）
 *
 * 再生中の要素を削除した場合は、同じ位置の次の曲（最後の曲なら、新しい最後の曲）へ移る。
 * @param entryId - 削除する要素のID
 */
export function removeFromQueue(entryId: number): void {
  const queue = player.playQueue;
  const index = indexOfEntry(queue, entryId);
  if (index === -1) return;
  const currentIndex = player.currentTrackIndex;

  const newQueue = queue.filter((_, i) => i !== index);
  player.playQueue = newQueue;
  player.originalQueue = player.originalQueue.filter((entry) => entry.entryId !== entryId);

  if (index === currentIndex) {
    if (newQueue.length > 0) {
      const newIndex = Math.min(currentIndex, newQueue.length - 1);
      player.currentTrackIndex = newIndex;
      player.currentTrack = newQueue[newIndex].track;
    } else {
      resetPlayer();
    }
  } else if (index < currentIndex) {
    // 再生中の要素より前を削除した: 位置を詰める
    player.currentTrackIndex = currentIndex - 1;
  }
}

/**
 * キューをクリア（再生中のトラックも外す。再生コントローラーが、再生するトラックがなくなったのを見て
 * 再生を止める）
 */
export function clearQueue(): void {
  resetPlayer();
}
