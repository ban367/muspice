import { describe, expect, it } from 'vitest';
import type { AlbumSummary, Track } from '#lib/types/models.js';
import { albumArtistKey, albumGroupSummary, groupAlbumsByArtist } from './albumArtists.js';

function album(
  name: string,
  artist: string | null,
  overrides: Partial<AlbumSummary> = {}
): AlbumSummary {
  return {
    name,
    sortName: null,
    artist,
    artistSortName: null,
    trackCount: 1,
    totalDuration: 100,
    representativeTrackId: `${artist}:${name}`,
    ...overrides
  };
}

function track(id: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: 'Artist',
    album: 'Album',
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: 100,
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

describe('groupAlbumsByArtist', () => {
  it('アルバムを、まとめたアーティストごとに分ける（アルバムは渡した順のまま）', () => {
    const entries = groupAlbumsByArtist([
      album('Alpha', 'B'),
      album('Beta', 'A'),
      album('Gamma', 'B'),
      // 複数のアーティストの曲のアルバムは、分けずに1件にする
      album('Duets', 'A; B')
    ]);

    expect(entries.map((entry) => [entry.name, entry.albums.map((a) => a.name)])).toEqual([
      ['A', ['Beta']],
      ['A; B', ['Duets']],
      ['B', ['Alpha', 'Gamma']]
    ]);
    // アルバムアートは、最初のアルバムの代表の曲
    expect(entries[2].representativeTrackId).toBe('B:Alpha');
    expect(entries[2].key).toBe(albumArtistKey('B'));
  });

  it('読みの順に並べ、アーティストのないアルバムは最後に置く', () => {
    const entries = groupAlbumsByArtist([
      album('X', null),
      album('Y', '中島みゆき', { artistSortName: 'なかじまみゆき' }),
      // 読みは、アルバムのうち最初に見つかったものを使う
      album('Z1', '椎名林檎'),
      album('Z2', '椎名林檎', { artistSortName: 'しいなりんご' }),
      album('W', 'aiko')
    ]);

    expect(entries.map((entry) => [entry.name, entry.sortName])).toEqual([
      ['aiko', null],
      ['椎名林檎', 'しいなりんご'],
      ['中島みゆき', 'なかじまみゆき'],
      [null, null]
    ]);
    // アーティストのないアルバムと、空の名前を区別する
    expect(albumArtistKey(null)).not.toBe(albumArtistKey(''));
  });
});

describe('albumGroupSummary', () => {
  it('曲を除いた形にし、まとめたアーティストの読みを曲から求める', () => {
    const tracks = [
      track('t1', { albumArtist: 'Various Artists' }),
      track('t2', {
        albumArtist: 'Various Artists',
        sortTags: { title: null, artist: 'あーてぃすと', album: null, albumArtist: 'ゔぁりあす' }
      })
    ];

    expect(
      albumGroupSummary({
        name: 'Album',
        sortName: null,
        artist: 'Various Artists',
        trackCount: 2,
        totalDuration: 200,
        representativeTrackId: 't1',
        tracks
      })
    ).toEqual({
      name: 'Album',
      sortName: null,
      artist: 'Various Artists',
      artistSortName: 'ゔぁりあす',
      trackCount: 2,
      totalDuration: 200,
      representativeTrackId: 't1'
    });
  });
});
