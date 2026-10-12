/**
 * キャッシュにある曲の書き換え
 *
 * 評価・お気に入りのように1曲の一部の項目だけが変わる操作では、全曲の一覧を取り直さず、
 * キャッシュの中のその曲だけを書き換える（全曲の一覧は数万曲になるため、操作のたびに
 * 取り直すには重い）。
 */

import type { QueryClient } from '@tanstack/svelte-query';
import type { AlbumGroup, Track } from '#lib/types/models.js';
import { queryKeys } from './keys';

/** `queryKeys.tracks`のクエリが返すデータ（曲の一覧、またはアルバムごとの曲） */
export type TrackQueryData = Track[] | AlbumGroup[];

function patchTrackList(
  tracks: Track[],
  trackIds: ReadonlySet<string>,
  patch: Partial<Track>
): Track[] {
  let patched: Track[] | null = null;
  tracks.forEach((track, index) => {
    if (!trackIds.has(track.id)) return;
    patched ??= [...tracks];
    patched[index] = { ...track, ...patch };
  });
  return patched ?? tracks;
}

/**
 * 曲を返すクエリのデータの中の曲を書き換える
 *
 * 対象の曲を含まないデータは、同じ参照のまま返す（表示の更新を起こさない）。
 * @param data - クエリのデータ（未取得ならundefined）
 * @param trackIds - 書き換える曲のID
 * @param patch - 書き換える項目
 */
export function patchTracksData<T extends TrackQueryData | undefined>(
  data: T,
  trackIds: ReadonlySet<string>,
  patch: Partial<Track>
): T {
  if (!data || data.length === 0 || trackIds.size === 0) return data;

  if ('tracks' in data[0]) {
    const albums = data as AlbumGroup[];
    let changed = false;
    const patched = albums.map((album) => {
      const tracks = patchTrackList(album.tracks, trackIds, patch);
      if (tracks === album.tracks) return album;
      changed = true;
      return { ...album, tracks };
    });
    return (changed ? patched : data) as T;
  }

  return patchTrackList(data as Track[], trackIds, patch) as T;
}

/**
 * 曲を返すクエリのデータの中の1曲を書き換える（`patchTracksData`の1曲版）
 */
export function patchTrackData<T extends TrackQueryData | undefined>(
  data: T,
  trackId: string,
  patch: Partial<Track>
): T {
  return patchTracksData(data, new Set([trackId]), patch);
}

/**
 * キャッシュにあるすべての曲の一覧の中の曲を書き換える
 * @param trackIds - 書き換える曲のID
 * @param patch - 書き換える項目
 */
export function patchTracksInCache(
  queryClient: QueryClient,
  trackIds: Iterable<string>,
  patch: Partial<Track>
): void {
  const ids = new Set(trackIds);
  queryClient.setQueriesData<TrackQueryData>({ queryKey: queryKeys.tracks.all }, (data) =>
    patchTracksData(data, ids, patch)
  );
}

/**
 * キャッシュにあるすべての曲の一覧の中の1曲を書き換える
 * @param trackId - 書き換える曲のID
 * @param patch - 書き換える項目
 */
export function patchTrackInCache(
  queryClient: QueryClient,
  trackId: string,
  patch: Partial<Track>
): void {
  patchTracksInCache(queryClient, [trackId], patch);
}
