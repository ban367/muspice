/**
 * Auto DJ（再生キューの最後の曲になったら、曲を足して再生を続ける）の選曲
 */
import type { Track } from '#lib/types/models.js';

/** 1回に足す曲数（「次に再生」に、これから再生する曲が少し見えるようにする） */
export const AUTO_DJ_BATCH_SIZE = 3;

/**
 * 足す曲を、元の曲（ライブラリの全曲か、プレイリストの曲）からランダムに選ぶ
 *
 * - ファイルが見つからない曲は選ばない
 * - 再生キューにない曲から選ぶ。元の曲がすべてキューに入っている場合は、キューの最後の曲
 *   （いま再生している曲）だけを避けて、もう一度選ぶ
 * - 選べる曲がなければ、空の配列を返す
 * @param candidates - 元の曲
 * @param queue - いまの再生キュー
 * @param count - 選ぶ曲数
 * @param random - 0以上1未満の乱数を返す関数
 */
export function pickAutoDjTracks(
  candidates: readonly Track[],
  queue: readonly Track[],
  count: number,
  random: () => number = Math.random
): Track[] {
  const playable = candidates.filter((track) => !track.isMissing);
  const queued = new Set(queue.map((track) => track.id));
  let pool = playable.filter((track) => !queued.has(track.id));
  if (pool.length === 0) {
    const lastId = queue.at(-1)?.id;
    pool = playable.filter((track) => track.id !== lastId);
  }

  // 必要な数だけ、ランダムに取り出す（Fisher-Yatesを、先頭の`count`個の分だけ行う）
  const picked: Track[] = [];
  const limit = Math.min(Math.max(0, Math.floor(count)), pool.length);
  for (let index = 0; index < limit; index++) {
    const from = index + Math.floor(random() * (pool.length - index));
    [pool[index], pool[from]] = [pool[from], pool[index]];
    picked.push(pool[index]);
  }
  return picked;
}
