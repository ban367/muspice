/**
 * アルバムの曲を、ディスクごとに分ける（アルバムの詳細・情報パネルで、ディスクの区切りを出す）
 */
import type { Track } from '#lib/types/models.js';

/** 1枚のディスクの曲 */
export interface DiscGroup {
  /** ディスク番号（番号のない曲は1枚目とみなす） */
  discNumber: number;
  tracks: Track[];
}

/**
 * 曲を、ディスク番号の順に分ける（ディスクの中の曲は、渡した順のまま）
 *
 * ディスク番号のない曲は、1枚目とみなす（アルバムの曲の並び順と同じ）。
 * 曲がなければ空の配列を返す。
 */
export function groupByDisc(tracks: readonly Track[]): DiscGroup[] {
  const groups = new Map<number, Track[]>();
  for (const track of tracks) {
    const discNumber = track.discNumber ?? 1;
    const group = groups.get(discNumber);
    if (group) {
      group.push(track);
    } else {
      groups.set(discNumber, [track]);
    }
  }
  return [...groups]
    .sort(([a], [b]) => a - b)
    .map(([discNumber, discTracks]) => ({ discNumber, tracks: discTracks }));
}
