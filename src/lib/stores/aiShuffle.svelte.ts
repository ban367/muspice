/**
 * AIシャッフル（曲が始まるたびに、設定の選び方に合う曲を選び、再生中の曲の次に足す。ADR-104）
 *
 * 曲を選ぶのはRust側（`pickAiShuffleTrack`。選び方は設定の`aiShuffleMode`）。ここでは、再生中の曲が
 * 変わるたびに選ばせ、キューへ足す（`queueAiShuffleTrack`）。曲が始まった時点で足すため、次の曲の
 * 先読み（ギャップレス再生・クロスフェード）も、足した曲に対して行われる。
 *
 * 有効・無効の切り替えと、シャッフルとの関係は`./player.svelte.ts`の`setAiShuffleEnabled`。
 */
import { untrack } from 'svelte';
import { AI_SHUFFLE_EXCLUSION_WINDOW, commands } from '#lib/bindings.js';
import type { QueueEntry } from '#lib/utils/playQueue.js';
import {
  aiShuffleExcludedTrackIds,
  aiShuffleTarget,
  player,
  queueAiShuffleTrack
} from './player.svelte.js';

/**
 * AIシャッフルを始める（有効な間、再生中の曲が変わるたびに曲を足す）
 *
 * 再生していない間（起動して復元しただけの曲など）は足さず、再生を始めた時に足す。
 * @returns 止める関数
 */
export function watchAiShuffle(): () => void {
  /** 最後に始めた選曲の番号（選んでいる間に次の選曲を始めたら、前の結果は使わない） */
  let latestRequest = 0;

  async function fill(entry: QueueEntry): Promise<void> {
    const request = ++latestRequest;
    try {
      const track = await commands.pickAiShuffleTrack(
        entry.track.id,
        aiShuffleExcludedTrackIds(AI_SHUFFLE_EXCLUSION_WINDOW)
      );
      // 選んでいる間に状況が変わっていたら、`queueAiShuffleTrack`が足さずに戻る
      if (request === latestRequest && track) queueAiShuffleTrack(track, entry.entryId);
    } catch (error) {
      // 足せなくても、再生は続ける（キューのその後ろの曲へ進む）
      console.error('Failed to pick a track for AI shuffle:', error);
    }
  }

  // コンポーネントの外でも動かし、戻り値の関数で止めるため$effect.rootで作る
  return $effect.root(() => {
    $effect(() => {
      const entry = player.isPlaying ? aiShuffleTarget() : null;
      if (entry) untrack(() => void fill(entry));
    });
  });
}
