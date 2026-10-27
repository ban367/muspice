/**
 * アルバムアーティスト（アルバムの画面のリスト表示の左の一覧）
 *
 * アルバムを、まとめたアーティスト（`AlbumSummary`の`artist`。アルバムアーティスト、なければ曲の
 * アーティスト）ごとに分ける。アーティストの画面と違い、複数のアーティストの曲のアルバム（`A; B`）を
 * 分けないため、どのアルバムも1つのアルバムアーティストの下に1回だけ入る（Rustの`find_album_artist_albums`と同じ）。
 */
import type { AlbumGroup, AlbumSummary } from '#lib/types/models.js';
import { compareNames, groupArtistSortName } from './nameSort.js';

/** アルバムアーティストの一覧の1件 */
export interface AlbumArtistEntry {
  /** 一覧の中で見分けるキー（アーティストのないアルバムもあるため、名前ではなくこれで見分ける） */
  key: string;
  /** アルバムをまとめたアーティスト（アルバムアーティストも曲のアーティストもないアルバムはnull） */
  name: string | null;
  /** 並び順に使う値（アルバムのうち最初に見つかったもの。なければnullで、`name`で並べる） */
  sortName: string | null;
  /** アルバム（渡したアルバムの一覧の順） */
  albums: AlbumSummary[];
  /** アルバムアートに使う曲（最初のアルバムの代表の曲） */
  representativeTrackId: string;
}

/** アルバムの画面の絞り込みの対象（アルバム名・アーティスト。グリッドとリスト表示で同じ） */
export function albumSearchFields(album: Pick<AlbumSummary, 'name' | 'artist'>): (string | null)[] {
  return [album.name, album.artist];
}

/** アルバムアーティストを見分けるキー（アーティストのないアルバムと、空の名前を区別する） */
export function albumArtistKey(artist: string | null): string {
  return JSON.stringify(artist);
}

/**
 * アルバムを、アルバムアーティストごとに分ける（名前の順。アーティストのないアルバムは最後）
 */
export function groupAlbumsByArtist(albums: readonly AlbumSummary[]): AlbumArtistEntry[] {
  const entries = new Map<string, AlbumArtistEntry>();
  for (const album of albums) {
    const key = albumArtistKey(album.artist);
    const entry = entries.get(key);
    if (entry) {
      entry.albums.push(album);
      entry.sortName ??= album.artistSortName;
    } else {
      entries.set(key, {
        key,
        name: album.artist,
        sortName: album.artistSortName,
        albums: [album],
        representativeTrackId: album.representativeTrackId
      });
    }
  }
  return [...entries.values()].sort((a, b) => {
    if (a.name === null || b.name === null)
      return Number(a.name === null) - Number(b.name === null);
    // 読みが同じ別のアーティストは、名前で並べる
    return compareNames(a.sortName || a.name, b.sortName || b.name) || compareNames(a.name, b.name);
  });
}

/**
 * アルバムとその曲を、アルバムの一覧の1件の形にする（情報パネル・メニューに渡す）
 *
 * まとめたアーティストの読みは、アルバムの一覧と同じく、アルバムの曲のうち最初に見つかったもの。
 */
export function albumGroupSummary(album: AlbumGroup): AlbumSummary {
  const { tracks, ...summary } = album;
  return {
    ...summary,
    artistSortName: tracks.map(groupArtistSortName).find((value) => value !== null) ?? null
  };
}
