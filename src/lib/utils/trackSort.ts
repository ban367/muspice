/**
 * 曲一覧の並び替え
 *
 * タイトル・アーティスト・アルバムは、並び順に使う値（ソート用のタグ。読み仮名など）があれば
 * その値で並べる（`./nameSort`）。
 */
import type { Track } from '#lib/types/models.js';
import { compareNames } from './nameSort.js';
import type { TrackColumnId } from './trackColumns.js';

/** 並び替えに使える項目（一覧に出せる列のどれでも並べ替えられる） */
export type TrackSortField = TrackColumnId;
export type SortDirection = 'asc' | 'desc';

/** 並び順（項目と向き） */
export interface TrackSort {
  field: TrackSortField;
  direction: SortDirection;
}

type SortKey = string | number;

/** 名前として比べる項目（言語に合わせた比較をする。ほかは、数値・書式のそろった文字列として比べる） */
const NAME_FIELDS: ReadonlySet<TrackSortField> = new Set([
  'title',
  'artist',
  'album',
  'albumArtist',
  'genre',
  'format'
]);

/** 見出しを最初にクリックした時に、大きい方（新しい方）から並べる項目 */
const DESCENDING_FIRST: ReadonlySet<TrackSortField> = new Set([
  'favorite',
  'rating',
  'playCount',
  'skipCount',
  'lastPlayedAt',
  'createdAt'
]);

/** その項目で並べ替え始める時の向き */
export function initialSortDirection(field: TrackSortField): SortDirection {
  return DESCENDING_FIRST.has(field) ? 'desc' : 'asc';
}

function sortKey(track: Track, field: TrackSortField): SortKey {
  switch (field) {
    case 'title':
      return track.sortTags.title || track.title || track.fileName;
    case 'artist':
      return track.sortTags.artist || track.artist || '';
    case 'album':
      return track.sortTags.album || track.album || '';
    case 'albumArtist':
      return track.sortTags.albumArtist || track.albumArtist || '';
    case 'genre':
      return track.genre || '';
    case 'format':
      return track.format;
    case 'favorite':
      return track.isFavorite ? 1 : 0;
    // 日時は書式のそろった文字列のため、そのまま比べられる（値のない曲は、いちばん古い扱い）
    case 'lastPlayedAt':
      return track.lastPlayedAt ?? '';
    case 'createdAt':
      return track.createdAt;
    case 'year':
    case 'trackNumber':
    case 'discNumber':
    case 'rating':
    case 'playCount':
    case 'skipCount':
    case 'duration':
    case 'bitrate':
    case 'sampleRate':
    case 'fileSize':
      return track[field] ?? 0;
  }
}

/** 並び替えた後の順に、元の一覧の中の位置を並べた配列を返す（同じ値の曲は元の順を保つ） */
function sortedOrder(
  keys: readonly SortKey[],
  field: TrackSortField,
  direction: SortDirection
): number[] {
  const useCollator = NAME_FIELDS.has(field);
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
