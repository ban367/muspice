import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  EMPTY_FILTERS,
  EMPTY_SELECTION,
  applyTrackFilters,
  browseTracks,
  clickBrowserItem,
  countActiveFilters,
  hasBrowserSelection,
  type BrowserItem
} from './trackFilter.js';

const noSortTags = { title: null, artist: null, album: null, albumArtist: null };

function track(id: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: null,
    album: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: null,
    fileSize: 1,
    format: 'mp3',
    bitrate: null,
    sampleRate: null,
    isFavorite: false,
    rating: 0,
    playCount: 0,
    skipCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    sortTags: noSortTags,
    isMissing: false,
    ...overrides
  };
}

const ids = (tracks: readonly Track[]) => tracks.map((t) => t.id);
const names = (items: BrowserItem[]) => items.map((item) => `${item.name}(${item.count})`);

describe('applyTrackFilters', () => {
  const tracks = [
    track('a', { rating: 5, year: 2020, isFavorite: true }),
    track('b', { rating: 3, year: 1999 }),
    track('c', { rating: 0, year: null, isFavorite: true }),
    track('d', { rating: 4, year: 2024 })
  ];

  it('条件がなければ、渡した配列をそのまま返す', () => {
    expect(applyTrackFilters(tracks, EMPTY_FILTERS)).toBe(tracks);
    expect(countActiveFilters(EMPTY_FILTERS)).toBe(0);
  });

  it('評価の下限で絞り込む', () => {
    expect(ids(applyTrackFilters(tracks, { ...EMPTY_FILTERS, minRating: 4 }))).toEqual(['a', 'd']);
    expect(ids(applyTrackFilters(tracks, { ...EMPTY_FILTERS, minRating: 1 }))).toEqual([
      'a',
      'b',
      'd'
    ]);
  });

  it('年の範囲で絞り込む（端の年を含む。年のない曲は含めない）', () => {
    const range = (yearFrom: number | null, yearTo: number | null) =>
      ids(applyTrackFilters(tracks, { ...EMPTY_FILTERS, yearFrom, yearTo }));

    expect(range(1999, 2020)).toEqual(['a', 'b']);
    expect(range(2020, null)).toEqual(['a', 'd']);
    expect(range(null, 1999)).toEqual(['b']);
    expect(range(2021, 2023)).toEqual([]);
  });

  it('お気に入りだけにする', () => {
    expect(ids(applyTrackFilters(tracks, { ...EMPTY_FILTERS, favoritesOnly: true }))).toEqual([
      'a',
      'c'
    ]);
  });

  it('すべての条件に合う曲だけにする', () => {
    const filters = { minRating: 4, yearFrom: 2000, yearTo: null, favoritesOnly: true };

    expect(ids(applyTrackFilters(tracks, filters))).toEqual(['a']);
    expect(countActiveFilters(filters)).toBe(3);
    // 年の範囲は、片方だけでも両方でも1つと数える
    expect(countActiveFilters({ ...EMPTY_FILTERS, yearFrom: 2000, yearTo: 2010 })).toBe(1);
  });
});

describe('browseTracks', () => {
  const tracks = [
    track('r1', { genre: 'Rock', artist: 'ZARD', album: '揺れる想い' }),
    track('r2', { genre: 'Rock', artist: 'ZARD', album: '揺れる想い' }),
    track('r3', { genre: 'Rock', artist: 'B', album: 'Greatest Hits' }),
    track('j1', { genre: 'Jazz', artist: 'A', album: 'Greatest Hits' }),
    track('j2', {
      genre: 'Jazz',
      artist: 'Guest',
      albumArtist: 'Various Artists',
      album: 'Compilation'
    }),
    track('n1', { genre: null, artist: null, album: null })
  ];

  it('何も選んでいなければ、すべての項目と、すべての曲を返す', () => {
    const result = browseTracks(tracks, EMPTY_SELECTION);

    // 値のない項目は、最後に並べる
    expect(names(result.genres)).toEqual(['Jazz(2)', 'Rock(3)', 'null(1)']);
    // アーティストは、アルバムアーティスト（なければ曲のアーティスト）でまとめる
    expect(names(result.artists)).toEqual([
      'A(1)',
      'B(1)',
      'Various Artists(1)',
      'ZARD(2)',
      'null(1)'
    ]);
    // 同じ名前のアルバムでも、アーティストが違えば別の項目
    expect(result.albums.map((album) => `${album.name}/${album.artist}(${album.count})`)).toEqual([
      'Compilation/Various Artists(1)',
      'Greatest Hits/A(1)',
      'Greatest Hits/B(1)',
      '揺れる想い/ZARD(2)',
      'null/null(1)'
    ]);
    // 同じ名前の項目には、見分けるための目印を付ける
    expect(result.albums.filter((album) => album.hasSameName).map((album) => album.artist)).toEqual(
      ['A', 'B']
    );
    expect(result.tracks).toHaveLength(6);
    expect(hasBrowserSelection(result.selection)).toBe(false);
  });

  it('ジャンルを選ぶと、アーティスト・アルバムの列と曲が絞られる', () => {
    const result = browseTracks(tracks, { ...EMPTY_SELECTION, genres: ['Rock'] });

    // ジャンルの列は、絞り込まれない
    expect(result.genres).toHaveLength(3);
    expect(names(result.artists)).toEqual(['B(1)', 'ZARD(2)']);
    expect(names(result.albums)).toEqual(['Greatest Hits(1)', '揺れる想い(2)']);
    expect(ids(result.tracks)).toEqual(['r1', 'r2', 'r3']);
  });

  it('ジャンル・アーティスト・アルバムの順に絞り込む', () => {
    const artists = browseTracks(tracks, {
      genres: ['Rock'],
      artists: ['ZARD'],
      albums: []
    });
    expect(names(artists.albums)).toEqual(['揺れる想い(2)']);
    expect(ids(artists.tracks)).toEqual(['r1', 'r2']);

    const albumKeyOfB = browseTracks(tracks, EMPTY_SELECTION).albums.find(
      (album) => album.artist === 'B'
    )!.key;
    const albums = browseTracks(tracks, { genres: [], artists: [], albums: [albumKeyOfB] });
    expect(ids(albums.tracks)).toEqual(['r3']);
  });

  it('1つの列で複数の項目を選べる', () => {
    const result = browseTracks(tracks, { ...EMPTY_SELECTION, artists: ['A', 'B'] });

    expect(ids(result.tracks)).toEqual(['r3', 'j1']);
    expect(names(result.albums)).toEqual(['Greatest Hits(1)', 'Greatest Hits(1)']);
  });

  it('値のない項目（不明なジャンルなど）も選べる', () => {
    const unknownGenre = browseTracks(tracks, EMPTY_SELECTION).genres.at(-1)!;
    expect(unknownGenre.name).toBeNull();

    const result = browseTracks(tracks, { ...EMPTY_SELECTION, genres: [unknownGenre.key] });
    expect(ids(result.tracks)).toEqual(['n1']);
  });

  it('上の列の絞り込みでなくなった項目は、選択から外す', () => {
    const result = browseTracks(tracks, { genres: ['Jazz'], artists: ['ZARD', 'A'], albums: [] });

    // ZARDは、Jazzの曲にいない
    expect(result.selection).toEqual({ genres: ['Jazz'], artists: ['A'], albums: [] });
    expect(ids(result.tracks)).toEqual(['j1']);
    // 一覧にないジャンルを選んでいても、絞り込まない
    expect(browseTracks(tracks, { ...EMPTY_SELECTION, genres: ['Pop'] }).tracks).toHaveLength(6);
  });

  it('アーティスト・アルバムは、読み（並び順に使う値）の順に並べる', () => {
    const list = [
      track('n', {
        artist: '中島みゆき',
        album: '歌集',
        sortTags: { ...noSortTags, artist: 'なかじまみゆき', album: 'かしゅう' }
      }),
      track('s', {
        artist: '椎名林檎',
        album: '無罪モラトリアム',
        sortTags: { ...noSortTags, artist: 'しいなりんご', album: 'むざい' }
      }),
      track('a', { artist: 'aiko', album: '桜の木の下' })
    ];

    const result = browseTracks(list, EMPTY_SELECTION);

    expect(result.artists.map((artist) => artist.name)).toEqual(['aiko', '椎名林檎', '中島みゆき']);
    expect(result.albums.map((album) => album.name)).toEqual([
      '歌集',
      '無罪モラトリアム',
      '桜の木の下'
    ]);
  });
});

describe('clickBrowserItem', () => {
  const items: BrowserItem[] = ['a', 'b', 'c', 'd'].map((key) => ({ key, name: key, count: 1 }));
  const plain = { toggleKey: false, shiftKey: false };

  it('そのままクリックすると、その項目だけを選ぶ', () => {
    expect(clickBrowserItem(items, ['a', 'b'], 'c', plain)).toEqual(['c']);
  });

  it('Cmd / Ctrl を押しながらクリックすると、選択を切り替える', () => {
    const toggle = { toggleKey: true, shiftKey: false };

    expect(clickBrowserItem(items, ['a'], 'c', toggle)).toEqual(['a', 'c']);
    expect(clickBrowserItem(items, ['a', 'c'], 'a', toggle)).toEqual(['c']);
  });

  it('Shift を押しながらクリックすると、最後に選んだ項目からの範囲を選ぶ', () => {
    const shift = { toggleKey: false, shiftKey: true };

    expect(clickBrowserItem(items, ['b'], 'd', shift).sort()).toEqual(['b', 'c', 'd']);
    expect(clickBrowserItem(items, ['c'], 'a', shift).sort()).toEqual(['a', 'b', 'c']);
    // 範囲の起点は、最後に選んだ項目のまま（続けて範囲を選び直せる）
    expect(clickBrowserItem(items, ['b'], 'd', shift).at(-1)).toBe('b');
    // 何も選んでいなければ、その項目だけを選ぶ
    expect(clickBrowserItem(items, [], 'c', shift)).toEqual(['c']);
  });
});
