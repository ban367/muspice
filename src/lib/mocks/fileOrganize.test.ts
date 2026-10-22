import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { parseOrganizeTemplate, planOrganize } from './fileOrganize';

const LIBRARY = '/music';
const DEFAULT = '%albumartist%/%album%/%track% %title%';

function track(id: string, filePath: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath,
    fileName: filePath.slice(filePath.lastIndexOf('/') + 1),
    title: 'Song',
    artist: 'Artist',
    album: 'Album',
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: 1,
    discNumber: null,
    duration: 200,
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
    sortTags: { title: null, artist: null, album: null, albumArtist: null },
    isMissing: false,
    ...overrides
  };
}

function plan(tracks: Track[], template = DEFAULT) {
  return planOrganize(tracks, [LIBRARY], parseOrganizeTemplate(template), tracks);
}

/** 1曲を整理した後の場所（ライブラリフォルダからの場所） */
const newPath = (template: string, overrides: Partial<Track> = {}, path = '/music/in/a.mp3') =>
  plan([track('t1', path, overrides)], template).entries[0].newPath;

describe('parseOrganizeTemplate', () => {
  it('書式の誤りは、VALIDATIONのエラーにする', () => {
    const invalid = [
      '  ',
      'a'.repeat(256),
      '%composer% - %title%',
      '%title',
      '[%track% [%disc%]]%title%',
      '%track%] %title%',
      '[%track% %title%',
      '%album%/track',
      '%artist%/%album%/'
    ];
    for (const template of invalid) {
      expect(() => parseOrganizeTemplate(template), template).toThrow(
        expect.objectContaining({ code: 'VALIDATION' })
      );
    }
  });

  it('「/」がなければ、今のフォルダのままファイル名だけを変える', () => {
    expect(parseOrganizeTemplate('%artist% - %title%').inPlace).toBe(true);
    expect(parseOrganizeTemplate('/%artist% - %title%').inPlace).toBe(false);
    expect(parseOrganizeTemplate('%Artist%\\%TITLE%').folders).toHaveLength(1);
  });
});

describe('planOrganize', () => {
  it('タグから、整理後の場所を決める（値のない項目は、既定の名前か、何も書かない）', () => {
    expect(newPath(DEFAULT)).toBe('Artist/Album/01 Song.mp3');
    expect(newPath(DEFAULT, { albumArtist: 'Various' })).toBe('Various/Album/01 Song.mp3');
    expect(newPath(DEFAULT, { title: null, album: ' ', trackNumber: null })).toBe(
      'Artist/Unknown Album/a.mp3'
    );
    expect(newPath('%genre%/%year%/%disc%-%track% %filename%', { year: 1999, discNumber: 2 })).toBe(
      'Unknown Genre/1999/2-01 a.mp3'
    );
    expect(newPath('100%% %title%')).toBe('in/100% Song.mp3');
  });

  it('値の中の「/」は階層にせず、使えない文字と前後のドット・空白を除く', () => {
    expect(newPath(DEFAULT, { artist: 'AC/DC', album: '..', title: 'What? .' })).toBe(
      'AC_DC/_/01 What_.mp3'
    );
  });

  it('[ ] の中の項目に値がなければ省き、空になった階層も省く', () => {
    const template = '%albumartist%/[%year% ]%album%/[Disc %disc%]/[%disc%-][%track% ]%title%';
    expect(newPath(template, { trackNumber: null })).toBe('Artist/Album/Song.mp3');
    expect(newPath(template, { year: 2001, discNumber: 2, trackNumber: 7 })).toBe(
      'Artist/2001 Album/Disc 2/2-07 Song.mp3'
    );
  });

  it('「/」のない書式は、今のフォルダの中で名前を変える', () => {
    const result = plan([track('t1', '/music/in/a.mp3')], '%artist% - %title%');
    expect(result.entries[0].newPath).toBe('in/Artist - Song.mp3');
    expect(result.moves).toEqual([
      { trackId: 't1', to: '/music/in/Artist - Song.mp3', copy: false }
    ]);
  });

  it('動かせない曲には、理由を付ける', () => {
    const result = plan([
      track('organized', '/music/Artist/Album/01 Song.mp3'),
      track('outside', '/elsewhere/b.mp3'),
      track('missing', '/music/in/c.mp3', { isMissing: true }),
      // 移動先は、すでに`organized`がある場所
      track('blocked', '/music/in/d.mp3')
    ]);

    expect(result.entries.map((entry) => entry.status)).toEqual([
      'unchanged',
      'outsideLibrary',
      'missing',
      'targetExists'
    ]);
    expect(result.entries[1]).toMatchObject({ currentPath: '/elsewhere/b.mp3', newPath: null });
    expect(result.moves).toEqual([]);
  });

  it('移動先がライブラリの見つからない曲の場所なら、ファイルがある場合と理由を分ける', () => {
    const result = plan([
      track('gone', '/music/Artist/Album/01 Song.mp3', { isMissing: true }),
      track('blocked', '/music/in/d.mp3')
    ]);

    expect(result.entries[1].status).toBe('targetRegistered');
    expect(result.moves).toEqual([]);
  });

  it('同じ移動先になる曲は、どれも動かさない（大文字と小文字の違いも同じとみなす）', () => {
    const result = plan(
      [
        track('a', '/music/a/one.mp3'),
        track('b', '/music/b/two.mp3', { title: 'song' }),
        track('c', '/music/b/three.mp3', { title: 'Other' })
      ],
      '%artist%/%title%'
    );

    expect(result.entries.map((entry) => entry.status)).toEqual([
      'sameTarget',
      'sameTarget',
      'move'
    ]);
    expect(result.moves).toEqual([{ trackId: 'c', to: '/music/Artist/Other.mp3', copy: false }]);
  });

  it('名前の大文字と小文字だけを変える曲は、動かせる', () => {
    const result = plan([track('t1', '/music/artist/song.mp3')], '%title%');
    expect(result.entries[0].status).toBe('move');
  });

  it('音楽フォルダへ集める時は、外の曲も音楽フォルダへ移し、別のドライブの曲はコピーにする', () => {
    const result = planOrganize(
      [
        track('t1', '/music/in/a.mp3'),
        track('t2', '/Volumes/Backup/b.mp3', { title: 'Second' }),
        track('t3', '/elsewhere/c.mp3', { title: 'Third' })
      ],
      ['/music', '/Volumes/Backup'],
      parseOrganizeTemplate('%title%'),
      [],
      '/music'
    );

    expect(result.entries.map((entry) => [entry.currentPath, entry.newPath])).toEqual([
      ['in/a.mp3', 'Song.mp3'],
      ['b.mp3', 'Second.mp3'],
      ['/elsewhere/c.mp3', 'Third.mp3']
    ]);
    expect(result.moves).toEqual([
      { trackId: 't1', to: '/music/Song.mp3', copy: false },
      { trackId: 't2', to: '/music/Second.mp3', copy: true },
      { trackId: 't3', to: '/music/Third.mp3', copy: false }
    ]);
  });
});
