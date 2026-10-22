/**
 * 重複した曲の検出
 *
 * タイトルとアーティストが同じで、長さがほぼ同じ曲を、同じ曲とみなす（ADR-052）。
 *
 * - タイトル・アーティストは、検索と同じ規則でそろえて比べる（大文字と小文字・全角と半角・
 *   ひらがなとカタカナの違いを無視する。`./searchText`）
 * - 複数のアーティストは、値に分けてから比べる（`A; B`と`A;B`は同じ）
 * - 長さは、数秒の違いまでを同じとみなす（形式・エンコードの違いで、少しずれるため）
 *
 * 取得済みの全曲の一覧から計算する（フォルダ別・年代別の画面と同じ）。
 */
import type { Track } from '#lib/types/models.js';
import { splitValues } from './multiValue.js';
import { normalizeSearchText } from './searchText.js';

/**
 * 同じ曲とみなす、長さの違いの上限（秒）
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export const DUPLICATE_DURATION_TOLERANCE = 3;

/** 重複した曲のまとまり */
export interface DuplicateGroup {
  /** まとまりを区別する値（一覧のキーに使う） */
  key: string;
  /** 同じ曲とみなした曲（2曲以上。場所の順） */
  tracks: Track[];
}

/** 比べるための、タイトルとアーティスト（タイトルのない曲は、比べない） */
function identityOf(track: Track): string | null {
  const title = normalizeSearchText(track.title ?? '').trim();
  if (title === '') return null;
  const artists = splitValues(track.artist).map((artist) => normalizeSearchText(artist));
  // 項目の区切りには、値に入らない文字を使う（項目をまたいだ一致を起こさない）
  return [title, ...artists].join('\n');
}

const byPath = (a: Track, b: Track) =>
  a.filePath < b.filePath ? -1 : a.filePath > b.filePath ? 1 : 0;

/**
 * タイトルとアーティストが同じ曲を、長さの近いものどうしに分ける
 *
 * 長さの短い順に並べ、まとまりの最初の曲との違いが上限までの曲を、同じまとまりにする
 * （少しずつ長い曲が続いても、離れた長さの曲はつながらない）。長さの分からない曲は、
 * 長さの分からない曲どうしでまとめる。
 */
function splitByDuration(tracks: Track[]): Track[][] {
  const unknown = tracks.filter((track) => track.duration === null);
  const known = tracks
    .filter((track) => track.duration !== null)
    .sort((a, b) => (a.duration ?? 0) - (b.duration ?? 0));

  const clusters: Track[][] = unknown.length > 0 ? [unknown] : [];
  let current: Track[] = [];
  for (const track of known) {
    const first = current[0]?.duration ?? null;
    if (first !== null && (track.duration ?? 0) - first > DUPLICATE_DURATION_TOLERANCE) {
      clusters.push(current);
      current = [];
    }
    current.push(track);
  }
  if (current.length > 0) clusters.push(current);
  return clusters;
}

/**
 * 重複した曲を探す
 *
 * @returns 同じ曲とみなした曲のまとまり（2曲以上のものだけ。タイトル・アーティスト・長さの順）
 */
export function findDuplicateGroups(tracks: readonly Track[]): DuplicateGroup[] {
  const byIdentity: Record<string, Track[]> = Object.create(null);
  for (const track of tracks) {
    const identity = identityOf(track);
    if (identity === null) continue;
    (byIdentity[identity] ??= []).push(track);
  }

  const groups: DuplicateGroup[] = [];
  for (const identity of Object.keys(byIdentity).sort()) {
    const candidates = byIdentity[identity];
    if (candidates.length < 2) continue;
    for (const cluster of splitByDuration(candidates)) {
      if (cluster.length < 2) continue;
      const first = cluster[0].duration;
      groups.push({ key: `${identity}\n${first ?? ''}`, tracks: cluster.sort(byPath) });
    }
  }
  return groups;
}

/** まとまりを、一覧に出す順の曲にする */
export function flattenDuplicateGroups(groups: readonly DuplicateGroup[]): Track[] {
  return groups.flatMap((group) => group.tracks);
}

/** 曲のIDから、その曲のまとまりのキーを引く関数を作る（まとまりにない曲は、空の文字列） */
export function duplicateGroupLookup(groups: readonly DuplicateGroup[]): (track: Track) => string {
  const keys: Record<string, string> = Object.create(null);
  for (const group of groups) {
    for (const track of group.tracks) keys[track.id] = group.key;
  }
  return (track) => keys[track.id] ?? '';
}
