/**
 * ライブラリの絞り込み（フィルタと、カラムブラウザ）
 *
 * 曲の一覧を、条件（評価・年・お気に入り）と、カラムブラウザの選択（ジャンル → アーティスト →
 * アルバム）で絞り込む。取得済みの曲の一覧に対して計算する（ファイル・データベースは読まない）。
 *
 * 絞り込みの順は、条件 → ジャンル → アーティスト → アルバム。カラムブラウザの各列には、
 * その列より前の絞り込みに合う曲の値だけを出す。
 */
import type { Track } from '#lib/types/models.js';
import { albumKey } from './albumKey.js';
import { compareNames } from './nameSort.js';

// ---------- フィルタ（評価・年・お気に入り） ----------

export interface TrackFilters {
  /** 評価の下限（星の数。0は絞り込まない） */
  minRating: number;
  /** 年の範囲（どちらもその年を含む。nullは指定なし） */
  yearFrom: number | null;
  yearTo: number | null;
  /** お気に入りの曲だけにする */
  favoritesOnly: boolean;
}

export const EMPTY_FILTERS: TrackFilters = {
  minRating: 0,
  yearFrom: null,
  yearTo: null,
  favoritesOnly: false
};

/** 有効な条件の数（ボタンの印に出す。年の範囲は、片方だけでも1つと数える） */
export function countActiveFilters(filters: TrackFilters): number {
  return (
    (filters.minRating > 0 ? 1 : 0) +
    (filters.yearFrom !== null || filters.yearTo !== null ? 1 : 0) +
    (filters.favoritesOnly ? 1 : 0)
  );
}

/**
 * 条件に合う曲だけにする（条件がなければ、渡した配列をそのまま返す）
 *
 * 年の範囲を指定した場合、年のない曲は含めない。
 */
export function applyTrackFilters(tracks: readonly Track[], filters: TrackFilters): Track[] {
  if (countActiveFilters(filters) === 0) return tracks as Track[];
  const { minRating, yearFrom, yearTo, favoritesOnly } = filters;
  const hasYearRange = yearFrom !== null || yearTo !== null;

  return tracks.filter((track) => {
    if (track.rating < minRating) return false;
    if (favoritesOnly && !track.isFavorite) return false;
    if (hasYearRange) {
      if (track.year === null) return false;
      if (yearFrom !== null && track.year < yearFrom) return false;
      if (yearTo !== null && track.year > yearTo) return false;
    }
    return true;
  });
}

// ---------- カラムブラウザ（ジャンル → アーティスト → アルバム） ----------

/** カラムブラウザの列 */
export type BrowserColumn = 'genres' | 'artists' | 'albums';
export const BROWSER_COLUMNS: readonly BrowserColumn[] = ['genres', 'artists', 'albums'];

/** カラムブラウザで選んでいる項目（列ごとのキー。空は「すべて」） */
export type BrowserSelection = Record<BrowserColumn, string[]>;

export const EMPTY_SELECTION: BrowserSelection = { genres: [], artists: [], albums: [] };

/** カラムブラウザの1項目 */
export interface BrowserItem {
  /** 選択に使うキー（値のない項目は、列ごとに決まった形になる） */
  key: string;
  /** 表示する名前（値のない項目はnull。「不明なジャンル」などは表示する側で出す） */
  name: string | null;
  /** アルバムの、まとめたアーティスト（同じ名前のアルバムを見分けるために出す） */
  artist?: string | null;
  /** 同じ名前の項目が、一覧にほかにもあるか（アルバムの列で、アーティストも出す目印） */
  hasSameName?: boolean;
  /** その項目の曲数 */
  count: number;
}

export interface BrowserResult {
  genres: BrowserItem[];
  artists: BrowserItem[];
  albums: BrowserItem[];
  /** 今の一覧にある項目だけにした選択（上の列の絞り込みでなくなった項目は外す） */
  selection: BrowserSelection;
  /** 選択で絞り込んだ曲 */
  tracks: Track[];
}

/** 値のないジャンル・アーティストのキー（空の文字列は、タグの値としては入らない） */
const UNKNOWN_KEY = '';

/** 曲をまとめるアーティスト（アルバムアーティスト。なければ曲のアーティスト） */
const groupArtist = (track: Track) => track.albumArtist ?? track.artist;

/** まとめたアーティストの、並び順に使う値（`nameSort`の`sortTracksByArtistAndAlbum`と同じ） */
const groupArtistSortName = (track: Track) =>
  track.albumArtist === null
    ? track.sortTags.artist
    : (track.sortTags.albumArtist ??
      (track.artist === track.albumArtist ? track.sortTags.artist : null));

/** 集計の途中の項目（並び順に使う値は、最初に見つかったものを使う） */
interface Bucket extends BrowserItem {
  sortName: string | null;
}

/** 曲を、キーごとに数える */
function collect(
  tracks: readonly Track[],
  describe: (track: Track) => Omit<Bucket, 'count'>
): BrowserItem[] {
  const buckets = new Map<string, Bucket>();
  for (const track of tracks) {
    const item = describe(track);
    const bucket = buckets.get(item.key);
    if (bucket) {
      bucket.count++;
      bucket.sortName ??= item.sortName;
    } else {
      buckets.set(item.key, { ...item, count: 1 });
    }
  }

  return [...buckets.values()]
    .sort((a, b) => {
      // 値のない項目は、最後に並べる
      if ((a.name === null) !== (b.name === null)) return a.name === null ? 1 : -1;
      return (
        compareNames(a.sortName || a.name || '', b.sortName || b.name || '') ||
        compareNames(a.artist ?? '', b.artist ?? '')
      );
    })
    .map(({ sortName: _sortName, ...item }) => item);
}

/** 同じ名前の項目が複数ある場合に、目印を付ける（別のアーティストの、同じ名前のアルバム） */
function markSameNames(items: BrowserItem[]): BrowserItem[] {
  const counts = new Map<string | null, number>();
  for (const item of items) counts.set(item.name, (counts.get(item.name) ?? 0) + 1);
  return items.map((item) =>
    (counts.get(item.name) ?? 0) > 1 ? { ...item, hasSameName: true } : item
  );
}

const genreKey = (track: Track) => track.genre ?? UNKNOWN_KEY;
const artistKey = (track: Track) => groupArtist(track) ?? UNKNOWN_KEY;
const albumKeyOf = (track: Track) =>
  albumKey({ name: track.album ?? '', artist: groupArtist(track) });

/** 選んだ項目に合う曲だけにする（何も選んでいなければ、すべて） */
function narrow(
  tracks: readonly Track[],
  selected: readonly string[],
  keyOf: (track: Track) => string
): readonly Track[] {
  if (selected.length === 0) return tracks;
  const keys = new Set(selected);
  return tracks.filter((track) => keys.has(keyOf(track)));
}

/** 今の一覧にある項目だけを、選択に残す */
function keepExisting(selected: readonly string[], items: readonly BrowserItem[]): string[] {
  if (selected.length === 0) return [];
  const keys = new Set(items.map((item) => item.key));
  return selected.filter((key) => keys.has(key));
}

/**
 * カラムブラウザの各列の項目と、選択で絞り込んだ曲を求める
 *
 * - ジャンルの列: 渡した曲のジャンル
 * - アーティストの列: 選んだジャンルの曲の、アーティスト（アルバムアーティスト。なければ曲のアーティスト）
 * - アルバムの列: 選んだジャンル・アーティストの曲の、アルバム（同じ名前でも、アーティストが違えば別）
 */
export function browseTracks(tracks: readonly Track[], selection: BrowserSelection): BrowserResult {
  const genres = collect(tracks, (track) => ({
    key: genreKey(track),
    name: track.genre,
    sortName: null
  }));
  const selectedGenres = keepExisting(selection.genres, genres);
  const inGenres = narrow(tracks, selectedGenres, genreKey);

  const artists = collect(inGenres, (track) => ({
    key: artistKey(track),
    name: groupArtist(track),
    sortName: groupArtistSortName(track)
  }));
  const selectedArtists = keepExisting(selection.artists, artists);
  const inArtists = narrow(inGenres, selectedArtists, artistKey);

  const albums = markSameNames(
    collect(inArtists, (track) => ({
      key: albumKeyOf(track),
      name: track.album,
      artist: groupArtist(track),
      sortName: track.sortTags.album
    }))
  );
  const selectedAlbums = keepExisting(selection.albums, albums);
  const inAlbums = narrow(inArtists, selectedAlbums, albumKeyOf);

  return {
    genres,
    artists,
    albums,
    selection: { genres: selectedGenres, artists: selectedArtists, albums: selectedAlbums },
    tracks: inAlbums as Track[]
  };
}

/** カラムブラウザで、何かを選んでいるか */
export function hasBrowserSelection(selection: BrowserSelection): boolean {
  return BROWSER_COLUMNS.some((column) => selection[column].length > 0);
}

/**
 * 項目のクリックによる選択（その列の、新しい選択を返す）
 *
 * - そのままクリック: その項目だけを選ぶ
 * - Cmd / Ctrl を押しながら: その項目の選択を切り替える（複数選べる）
 * - Shift を押しながら: 最後に選んだ項目から、その項目までを選ぶ
 * @param items - 列の項目（表示順）
 * @param selected - その列の今の選択（最後の要素が、最後に選んだ項目）
 */
export function clickBrowserItem(
  items: readonly BrowserItem[],
  selected: readonly string[],
  key: string,
  modifiers: { toggleKey: boolean; shiftKey: boolean }
): string[] {
  if (modifiers.shiftKey && selected.length > 0) {
    const keys = items.map((item) => item.key);
    const from = keys.indexOf(selected[selected.length - 1]);
    const to = keys.indexOf(key);
    if (from >= 0 && to >= 0) {
      const range = keys.slice(Math.min(from, to), Math.max(from, to) + 1);
      // 最後に選んだ項目（範囲の起点）を、最後の要素のままにする
      const anchor = keys[from];
      return [...range.filter((k) => k !== anchor), anchor];
    }
  }
  if (modifiers.toggleKey) {
    return selected.includes(key) ? selected.filter((k) => k !== key) : [...selected, key];
  }
  return [key];
}
