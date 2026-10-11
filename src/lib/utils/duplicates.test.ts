import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  DUPLICATE_DURATION_TOLERANCE,
  duplicateGroupLookup,
  findDuplicateGroups,
  flattenDuplicateGroups
} from './duplicates.js';

function track(id: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: 'Song',
    artist: 'Artist',
    album: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
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

/** まとまりごとの、曲のID */
const idsOf = (tracks: Track[]) =>
  findDuplicateGroups(tracks).map((g) => g.tracks.map((t) => t.id));

describe('findDuplicateGroups', () => {
  it('タイトルとアーティストが同じで、長さがほぼ同じ曲をまとめる', () => {
    const groups = idsOf([
      track('a'),
      track('b', {
        duration: 200 + DUPLICATE_DURATION_TOLERANCE,
        album: 'Best Of',
        format: 'flac'
      }),
      track('c', { title: 'Other' }),
      track('d', { artist: 'Someone Else' })
    ]);

    expect(groups).toEqual([['a', 'b']]);
  });

  it('大文字と小文字・全角と半角・ひらがなとカタカナの違いを無視する', () => {
    expect(
      idsOf([
        track('a', { title: 'カブトムシ', artist: 'aiko' }),
        track('b', { title: 'ｶﾌﾞﾄﾑｼ', artist: 'ＡＩＫＯ' }),
        track('c', { title: ' かぶとむし ', artist: 'Aiko' })
      ])
    ).toEqual([['a', 'b', 'c']]);
  });

  it('複数のアーティストは、値に分けて比べる', () => {
    expect(
      idsOf([
        track('a', { artist: 'A; B' }),
        track('b', { artist: 'A;B' }),
        track('c', { artist: 'A' }),
        track('d', { artist: 'B; A' })
      ])
    ).toEqual([['a', 'b']]);
  });

  it('長さが離れた曲は、別の曲として扱う', () => {
    const groups = idsOf([
      track('single', { duration: 200 }),
      track('single2', { duration: 202 }),
      track('live', { duration: 320 }),
      track('live2', { duration: 321 }),
      track('remix', { duration: 260 })
    ]);

    expect(groups).toEqual([
      ['single', 'single2'],
      ['live', 'live2']
    ]);
  });

  it('少しずつ長い曲が続いても、最初の曲から離れた曲はまとめない', () => {
    expect(
      idsOf([
        track('a', { duration: 200 }),
        track('b', { duration: 203 }),
        track('c', { duration: 206 }),
        track('d', { duration: 209 })
      ])
    ).toEqual([
      ['a', 'b'],
      ['c', 'd']
    ]);
  });

  it('長さの分からない曲は、長さの分からない曲どうしでまとめる', () => {
    expect(
      idsOf([
        track('a', { duration: null }),
        track('b', { duration: null }),
        track('c', { duration: 200 })
      ])
    ).toEqual([['a', 'b']]);
  });

  it('アーティストのない曲もまとめ、タイトルのない曲はまとめない', () => {
    expect(
      idsOf([
        track('a', { artist: null }),
        track('b', { artist: null }),
        track('c', { title: null }),
        track('d', { title: null }),
        track('e', { title: '  ' })
      ])
    ).toEqual([['a', 'b']]);
  });

  it('まとまりはタイトルの順、まとまりの中は場所の順に並べる', () => {
    const groups = findDuplicateGroups([
      track('z2', { title: 'Zebra', filePath: '/music/b/zebra.mp3' }),
      track('a1', { title: 'Apple', filePath: '/music/b/apple.mp3' }),
      track('z1', { title: 'Zebra', filePath: '/music/a/zebra.mp3' }),
      track('a2', { title: 'Apple', filePath: '/music/a/apple.flac' })
    ]);

    expect(flattenDuplicateGroups(groups).map((t) => t.id)).toEqual(['a2', 'a1', 'z1', 'z2']);
    // まとまりのキーは、まとまりごとに違う
    expect(new Set(groups.map((group) => group.key)).size).toBe(2);
  });

  it('重複がなければ、空を返す', () => {
    expect(findDuplicateGroups([])).toEqual([]);
    expect(findDuplicateGroups([track('a'), track('b', { title: 'Other' })])).toEqual([]);
  });
});

describe('duplicateGroupLookup', () => {
  it('曲から、まとまりのキーを引く（まとまりにない曲は、空の文字列）', () => {
    const tracks = [
      track('a'),
      track('b'),
      track('c', { title: 'Other' }),
      track('d', { title: 'Other' })
    ];
    const groupOf = duplicateGroupLookup(findDuplicateGroups(tracks));

    expect(groupOf(tracks[0])).toBe(groupOf(tracks[1]));
    expect(groupOf(tracks[2])).toBe(groupOf(tracks[3]));
    expect(groupOf(tracks[0])).not.toBe(groupOf(tracks[2]));
    expect(groupOf(track('x'))).toBe('');
  });
});
