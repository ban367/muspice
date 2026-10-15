/**
 * 再生キューの要素
 *
 * 再生キューには同じ曲を何度でも入れられるため、要素ごとに一意なID（エントリID）を振り、
 * 曲のIDではなくエントリIDで見分ける（一覧のキー・削除・並べ替え・シャッフルの切り替え）。
 * エントリIDは保存しない（保存・復元は曲のIDの並びで行い、復元する時に振り直す）。
 */
import type { Track } from '#lib/types/models.js';

/** 再生キューの要素 */
export interface QueueEntry {
  /** 要素ごとに一意なID（キューに入れるたびに新しく振る） */
  readonly entryId: number;
  /** 曲（キューに入れた時点の内容） */
  readonly track: Track;
}

/** 最後に振ったエントリID */
let lastEntryId = 0;

/** 曲を、キューの要素にする（要素ごとに新しいエントリIDを振る） */
export function toQueueEntries(tracks: readonly Track[]): QueueEntry[] {
  return tracks.map((track) => ({ entryId: ++lastEntryId, track }));
}

/**
 * 曲の並びを、キューの要素へ対応付ける（曲のIDの並びで保存してあった、シャッフルする前の順を
 * 復元する時に使う）
 *
 * 同じ曲が複数ある場合は、前から順に対応付ける。対応する要素が残っていない曲は、新しい要素にする。
 * @param tracks - 対応付ける曲の並び
 * @param entries - 対応付け先の要素（どの要素も、1回までしか使わない）
 */
export function matchQueueEntries(
  tracks: readonly Track[],
  entries: readonly QueueEntry[]
): QueueEntry[] {
  // 曲ごとに、要素を後ろから積む（前の要素から順に取り出せる）
  const unused = new Map<string, QueueEntry[]>();
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index];
    const stack = unused.get(entry.track.id);
    if (stack) stack.push(entry);
    else unused.set(entry.track.id, [entry]);
  }
  return tracks.map((track) => unused.get(track.id)?.pop() ?? toQueueEntries([track])[0]);
}
