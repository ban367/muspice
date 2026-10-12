/**
 * 名前の並び順
 *
 * アーティスト・アルバム・曲の名前は、並び順に使う値（ソート用のタグ。読み仮名など）があれば
 * その値で、なければ表示用の名前で並べる。比較は言語に合わせて行う（ひらがなとカタカナ・
 * 大文字と小文字を区別せず、五十音順に並ぶ）。
 */
import type { Track } from '#lib/types/models.js';

// 文字列の比較は件数が多いと重いため、比較のたびに作らず1つを使い回す
const collator = new Intl.Collator('ja');

/** 名前（または、並び順に使う値）を比べる */
export function compareNames(a: string, b: string): number {
  return collator.compare(a, b);
}

/** 並び順に使う値を持つもの（アルバム・アーティストなど） */
interface Named {
  name: string;
  /** 並び順に使う値（なければ`name`で並べる） */
  sortName?: string | null;
}

/**
 * 一覧を、名前の順に並べる（元の配列は変えない。同じ並びのものは、元の順を保つ）
 */
export function sortByName<T extends Named>(items: readonly T[]): T[] {
  return [...items].sort((a, b) => compareNames(a.sortName || a.name, b.sortName || b.name));
}

/** 曲をまとめるアーティスト（アルバムアーティスト。なければ曲のアーティスト） */
function groupArtist(track: Track): string {
  return track.albumArtist ?? track.artist ?? '';
}

/**
 * 曲をまとめるアーティストの、並び順に使う値（なければnull）
 *
 * アルバムアーティストでまとめる曲は、アルバムアーティストの読み（なければ、同じ名前の
 * 曲のアーティストの読み）。曲のアーティストでまとめる曲は、曲のアーティストの読み。
 */
function groupArtistSortName(track: Track): string | null {
  const { sortTags } = track;
  if (track.albumArtist === null) return sortTags.artist;
  return sortTags.albumArtist ?? (track.artist === track.albumArtist ? sortTags.artist : null);
}

/**
 * 曲を、アーティスト → アルバムの順に並べ直す（元の配列は変えない）
 *
 * 同じアーティスト・同じアルバムの曲は、元の順（アルバムの中の並び）を保つ。
 * 並び順に使う値は、そのアーティスト・アルバムの曲のうち最初に見つかったものを使う
 * （一部の曲にだけ読みがあっても、アーティスト・アルバムの曲が分かれない）。
 */
export function sortTracksByArtistAndAlbum(tracks: readonly Track[]): Track[] {
  const artistKeys = new Map<string, string>();
  const albumKeys = new Map<string, string>();
  const albumId = (track: Track) => JSON.stringify([groupArtist(track), track.album]);

  for (const track of tracks) {
    const artist = groupArtist(track);
    const artistSort = groupArtistSortName(track);
    // 読みは、最初に見つかったものを覚える（読みのないアーティスト・アルバムは、名前で並べる）
    if (artistSort && artistKeys.get(artist) === undefined) artistKeys.set(artist, artistSort);
    const album = albumId(track);
    if (track.sortTags.album && albumKeys.get(album) === undefined) {
      albumKeys.set(album, track.sortTags.album);
    }
  }
  const artistKey = (track: Track) => artistKeys.get(groupArtist(track)) ?? groupArtist(track);
  const albumKey = (track: Track) => albumKeys.get(albumId(track)) ?? track.album ?? '';

  return [...tracks].sort(
    (a, b) =>
      compareNames(artistKey(a), artistKey(b)) ||
      // 読みが同じ別のアーティストの曲が混ざらないよう、名前でも比べる
      compareNames(groupArtist(a), groupArtist(b)) ||
      compareNames(albumKey(a), albumKey(b)) ||
      compareNames(a.album ?? '', b.album ?? '')
  );
}
