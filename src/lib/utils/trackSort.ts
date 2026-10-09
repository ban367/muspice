/**
 * 曲一覧の並び替え
 *
 * タイトル・アーティスト・アルバムは、並び順に使う値（ソート用のタグ。読み仮名など）があれば
 * その値で並べる（`./nameSort`）。
 */
import type { Track } from '#lib/types/models.js';
import { compareNames } from './nameSort.js';

export type TrackSortField = 'title' | 'artist' | 'album' | 'duration' | 'createdAt' | 'playCount';
export type SortDirection = 'asc' | 'desc';

type SortKey = string | number;

function sortKey(track: Track, field: TrackSortField): SortKey {
  switch (field) {
    case 'title':
      return track.sortTags.title || track.title || track.fileName;
    case 'artist':
      return track.sortTags.artist || track.artist || '';
    case 'album':
      return track.sortTags.album || track.album || '';
    case 'duration':
      return track.duration || 0;
    case 'createdAt':
      return track.createdAt;
    case 'playCount':
      return track.playCount;
  }
}

/** 並び替えた後の順に、元の一覧の中の位置を並べた配列を返す（同じ値の曲は元の順を保つ） */
function sortedOrder(
  keys: readonly SortKey[],
  field: TrackSortField,
  direction: SortDirection
): number[] {
  // 日時は書式のそろった文字列のため、言語に合わせた比較ではなく単純な比較で並べる
  const useCollator = field === 'title' || field === 'artist' || field === 'album';
  const sign = direction === 'asc' ? 1 : -1;

  return keys
    .map((_, index) => index)
    .sort((a, b) => {
      const keyA = keys[a];
      const keyB = keys[b];
      const comparison = useCollator
        ? compareNames(keyA as string, keyB as string)
        : keyA < keyB
          ? -1
          : keyA > keyB
            ? 1
            : 0;
      return comparison !== 0 ? sign * comparison : a - b;
    });
}

/**
 * 曲を並び替える関数（元の配列は変えない）
 * @param tracks - 並び替える曲
 * @param field - 並び替えに使う項目（タイトルがない曲はファイル名で並べる。タイトル・アーティスト・
 *   アルバムは、並び順に使う値があればその値で並べる）
 * @param direction - 昇順・降順
 */
export type TrackSorter = (
  tracks: readonly Track[],
  field: TrackSortField,
  direction: SortDirection
) => Track[];

/**
 * 直前の並び替えの結果を覚えておく、曲の並び替えの関数を作る
 *
 * 評価の変更などで一覧が作り直されても、曲の並びと並び替えに使う値が直前と同じなら、
 * 比較をやり直さず直前の順を使う（数万曲の文字列の比較は、操作のたびに行うには重い）。
 */
export function createTrackSorter(): TrackSorter {
  let last: {
    field: TrackSortField;
    direction: SortDirection;
    ids: string[];
    keys: SortKey[];
    order: number[];
  } | null = null;

  return (tracks, field, direction) => {
    const keys = tracks.map((track) => sortKey(track, field));
    const previous = last;
    const isSameInput =
      previous !== null &&
      previous.field === field &&
      previous.direction === direction &&
      previous.ids.length === tracks.length &&
      tracks.every(
        (track, index) => track.id === previous.ids[index] && keys[index] === previous.keys[index]
      );

    const current =
      isSameInput && previous !== null
        ? previous
        : {
            field,
            direction,
            ids: tracks.map((track) => track.id),
            keys,
            order: sortedOrder(keys, field, direction)
          };
    last = current;
    return current.order.map((index) => tracks[index]);
  };
}
