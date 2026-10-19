import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { groupByDisc } from './albumDiscs.js';

function track(id: string, discNumber: number | null): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: null,
    album: 'Album',
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber,
    duration: 60,
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
    isMissing: false
  };
}

/** 「ディスク番号: 曲のID」の形にする（比べやすくする） */
const summarize = (tracks: readonly Track[]) =>
  groupByDisc(tracks).map(
    (disc) => `${disc.discNumber}: ${disc.tracks.map((t) => t.id).join(',')}`
  );

describe('groupByDisc', () => {
  it('ディスク番号の順に分け、ディスクの中は渡した順のままにする', () => {
    const tracks = [track('a', 2), track('b', 1), track('c', 2), track('d', 1)];
    expect(summarize(tracks)).toEqual(['1: b,d', '2: a,c']);
  });

  it('ディスク番号のない曲は1枚目とみなす', () => {
    const tracks = [track('a', null), track('b', 2), track('c', 1)];
    expect(summarize(tracks)).toEqual(['1: a,c', '2: b']);
  });

  it('1枚だけのアルバムは、1つにまとめる', () => {
    expect(summarize([track('a', null), track('b', null)])).toEqual(['1: a,b']);
    expect(summarize([track('a', 3)])).toEqual(['3: a']);
  });

  it('曲がなければ空にする', () => {
    expect(groupByDisc([])).toEqual([]);
  });
});
