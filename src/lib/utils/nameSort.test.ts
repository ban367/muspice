import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { compareNames, sortByName, sortTracksByArtistAndAlbum } from './nameSort.js';

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

const names = (items: { name: string }[]) => items.map((item) => item.name);
const ids = (tracks: Track[]) => tracks.map((t) => t.id);

describe('compareNames', () => {
  it('ひらがなとカタカナ・大文字と小文字を区別せず、五十音順に比べる', () => {
    expect(compareNames('あいこ', 'アムロ')).toBeLessThan(0);
    expect(compareNames('アムロ', 'あゆ')).toBeLessThan(0);
    expect(compareNames('がくと', 'かとう')).toBeLessThan(0);
    expect(compareNames('aiko', 'ZARD')).toBeLessThan(0);
    expect(compareNames('zard', 'Aiko')).toBeGreaterThan(0);
  });
});

describe('sortByName', () => {
  it('並び順に使う値があればその値で、なければ名前で並べる', () => {
    const artists = [
      { name: '中島みゆき', sortName: 'なかじまみゆき' },
      { name: 'ZARD', sortName: null },
      { name: '椎名林檎', sortName: 'シイナリンゴ' },
      { name: 'aiko' },
      { name: '安室奈美恵', sortName: 'あむろなみえ' }
    ];

    expect(names(sortByName(artists))).toEqual([
      'aiko',
      'ZARD',
      '安室奈美恵',
      '椎名林檎',
      '中島みゆき'
    ]);
    // 元の配列は変えない
    expect(artists[0].name).toBe('中島みゆき');
  });

  it('同じ並びのものは、元の順を保つ', () => {
    const albums = [
      { name: 'Greatest Hits', artist: 'B' },
      { name: 'Greatest Hits', artist: 'A' },
      { name: 'Alpha', artist: 'C' }
    ];

    expect(sortByName(albums).map((album) => album.artist)).toEqual(['C', 'B', 'A']);
  });

  it('空の並び順の値は、ないものとして扱う', () => {
    const items = [
      { name: 'b', sortName: '' },
      { name: 'a', sortName: null }
    ];

    expect(names(sortByName(items))).toEqual(['a', 'b']);
  });
});

describe('sortTracksByArtistAndAlbum', () => {
  it('アーティスト → アルバムの読みの順に並べ、アルバムの中の順は保つ', () => {
    const tracks = [
      // 元の並び（名前の文字コード順）: 中島みゆき → 椎名林檎
      track('n1', { artist: '中島みゆき', album: '歌集' }),
      track('n2', { artist: '中島みゆき', album: '歌集' }),
      track('s1', {
        artist: '椎名林檎',
        album: '勝訴ストリップ',
        sortTags: { ...noSortTags, artist: 'しいなりんご', album: 'しょうそすとりっぷ' }
      }),
      track('s2', {
        artist: '椎名林檎',
        album: '加爾基 精液 栗ノ花',
        sortTags: { ...noSortTags, artist: 'しいなりんご', album: 'かるき' }
      }),
      track('s3', {
        artist: '椎名林檎',
        album: '加爾基 精液 栗ノ花',
        sortTags: { ...noSortTags, artist: 'しいなりんご', album: 'かるき' }
      })
    ];
    tracks[0].sortTags = { ...noSortTags, artist: 'なかじまみゆき' };
    tracks[1].sortTags = { ...noSortTags, artist: 'なかじまみゆき' };

    expect(ids(sortTracksByArtistAndAlbum(tracks))).toEqual(['s2', 's3', 's1', 'n1', 'n2']);
    // 元の配列は変えない
    expect(ids(tracks)).toEqual(['n1', 'n2', 's1', 's2', 's3']);
  });

  it('一部の曲にだけ読みがあっても、アーティスト・アルバムの曲は分かれない', () => {
    const tracks = [
      track('a1', { artist: 'あ', album: 'アルバムA' }),
      // 読みのない曲と、読みのある曲が混ざったアーティスト
      track('k1', { artist: '椎名林檎', album: '無罪モラトリアム', trackNumber: 1 }),
      track('k2', {
        artist: '椎名林檎',
        album: '無罪モラトリアム',
        trackNumber: 2,
        sortTags: { ...noSortTags, artist: 'しいなりんご', album: 'むざいもらとりあむ' }
      }),
      track('z1', { artist: 'ん', album: 'アルバムZ' })
    ];

    expect(ids(sortTracksByArtistAndAlbum(tracks))).toEqual(['a1', 'k1', 'k2', 'z1']);
  });

  it('アルバムアーティストでまとめる曲は、アルバムアーティストの読みで並べる', () => {
    const tracks = [
      // 曲のアーティストの読みは、まとめるアーティスト（Various Artists）の並びに使わない
      track('v1', {
        artist: '椎名林檎',
        albumArtist: 'Various Artists',
        album: 'Compilation',
        sortTags: { ...noSortTags, artist: 'しいなりんご' }
      }),
      track('t1', {
        artist: '東京事変',
        albumArtist: '東京事変',
        album: '教育',
        sortTags: { ...noSortTags, albumArtist: 'とうきょうじへん' }
      }),
      // アルバムアーティストの読みがなければ、同じ名前の曲のアーティストの読みを使う
      track('u1', {
        artist: '宇多田ヒカル',
        albumArtist: '宇多田ヒカル',
        album: 'First Love',
        sortTags: { ...noSortTags, artist: 'うただひかる' }
      })
    ];

    expect(ids(sortTracksByArtistAndAlbum(tracks))).toEqual(['v1', 'u1', 't1']);
  });
});
