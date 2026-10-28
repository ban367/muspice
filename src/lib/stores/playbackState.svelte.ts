/**
 * 再生状態の保存と復元
 *
 * 音量・シャッフル・リピート・再生キュー・再生していた曲を、Rust側のファイル
 * （アプリデータの`playback-state.json`）に保存し、次の起動で復元する。再生位置は保存しない。
 *
 * - 復元（`restorePlaybackState`）: 起動時に1回呼ぶ。キューと再生していた曲を戻すが、再生は始めない
 * - 保存（`watchPlaybackState`）: 再生状態（`./player.svelte.ts`）を監視し、変更を少し待ってから
 *   まとめて保存する。キューは数万曲になることがあるため、キューが変わった時だけ送る
 *
 * キューは曲のIDの並びで保存する（キューの要素のID（エントリID）は保存せず、復元する時に振り直す）。
 */
import { untrack } from 'svelte';
import { commands, type RestoredPlaybackState } from '#lib/bindings.js';
import type { Track } from '#lib/types/models.js';
import { player, restoreQueue } from './player.svelte.js';

/** 変更してから保存するまでの待ち時間（ms。音量のドラッグなど、続けての変更をまとめる） */
const SAVE_DELAY_MS = 300;

export interface RestorePlaybackStateOptions {
  /** 復元を待つ間に、復元が要らなくなったか（再生コントローラーが破棄された）を返す */
  isCancelled: () => boolean;
  /**
   * 再生していた曲を再生状態（`player.currentTrack`）へ戻す直前に呼ばれる
   *
   * 再生コントローラーは`player.currentTrack`の変化で再生を始めるため、ここで「この曲は
   * 再生を始めない」と記録する（戻した後では、再生を始める監視の方が先に動く）。
   */
  onTrackRestoring: (track: Track) => void;
}

/**
 * 保存してある再生状態を、再生状態（`player`）へ復元する
 *
 * 音量・シャッフル・リピートは常に戻す。キューと再生していた曲は、復元を待つ間に再生を
 * 始めていなければ戻す（再生は始めない。再生ボタンで、その曲を頭から再生する）。
 */
export async function restorePlaybackState(options: RestorePlaybackStateOptions): Promise<void> {
  const state = await commands.getPlaybackState();
  if (options.isCancelled()) return;

  player.volume = state.volume ?? 1;
  player.isShuffleEnabled = state.shuffle;
  player.repeatMode = state.repeat;

  // 復元を待つ間に、曲を再生した・キューへ入れた場合は、キューを戻さない
  if (state.queue.length === 0 || player.currentTrack || player.playQueue.length > 0) return;

  // 再生していた曲がない（キューへ入れただけで、何も再生していなかった）場合も、キューは戻す
  const current = state.currentIndex === null ? undefined : state.queue[state.currentIndex];
  if (current) options.onTrackRestoring(current);
  restoreQueue(state.queue, originalQueueOf(state), state.currentIndex);
}

/** シャッフルする前の順のキュー（シャッフルが無効なら、再生する順と同じ） */
function originalQueueOf(state: RestoredPlaybackState): Track[] {
  if (!state.originalTrackIds) return state.queue;
  // シャッフルする前の順は、IDの並びで届く（曲の情報は`queue`にある）
  const byId: Record<string, Track | undefined> = Object.fromEntries(
    state.queue.map((track) => [track.id, track])
  );
  return state.originalTrackIds.map((id) => byId[id]).filter((track) => track !== undefined);
}

/**
 * 再生状態の変更を監視し、保存する
 *
 * 復元（`restorePlaybackState`）が済んでから始める（先に始めると、復元する前の空の状態で
 * 保存してある内容を上書きしてしまう）。
 * @returns 監視をやめる関数（保存を待っている変更があれば、すぐに保存する）
 */
export function watchPlaybackState(): () => void {
  /** キューの版（キューが変わるたびに進める） */
  let queueVersion = 0;
  /**
   * 保存が済んだキューの版
   *
   * `queueVersion`と違う間は、保存のたびにキューも送る。キューを送った保存が失敗した場合も、
   * 次の保存でキューを送り直す（再生していた曲の位置だけを保存すると、保存してあるキューと
   * 食い違う）。
   */
  let savedQueueVersion = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function save(): void {
    timer = null;
    const version = queueVersion;
    const queue =
      version !== savedQueueVersion
        ? {
            trackIds: player.playQueue.map((entry) => entry.track.id),
            // シャッフルが無効の間は、元の順は再生する順と同じため送らない
            originalTrackIds: player.isShuffleEnabled
              ? player.originalQueue.map((entry) => entry.track.id)
              : null
          }
        : null;
    commands
      .savePlaybackState(
        {
          volume: player.volume,
          shuffle: player.isShuffleEnabled,
          repeat: player.repeatMode,
          currentIndex: player.currentTrackIndex >= 0 ? player.currentTrackIndex : null
        },
        queue
      )
      .then(() => {
        if (queue) savedQueueVersion = version;
      })
      .catch((error) => console.error('再生状態の保存に失敗しました:', error));
  }

  function schedule(): void {
    timer ??= setTimeout(save, SAVE_DELAY_MS);
  }

  // 監視を始めた時点の状態は、保存してある内容と同じ（復元した直後）のため保存しない
  let watchingQueue = false;
  let watchingCursor = false;
  const stop = $effect.root(() => {
    $effect(() => {
      // 変更を検知するために読む（配列は、変更のたびに置き換えられる）
      void player.playQueue;
      void player.originalQueue;
      untrack(() => {
        if (watchingQueue) {
          queueVersion++;
          schedule();
        }
        watchingQueue = true;
      });
    });

    $effect(() => {
      void player.currentTrackIndex;
      void player.volume;
      void player.isShuffleEnabled;
      void player.repeatMode;
      untrack(() => {
        if (watchingCursor) schedule();
        watchingCursor = true;
      });
    });
  });

  return () => {
    stop();
    if (timer !== null) {
      clearTimeout(timer);
      save();
    }
  };
}
