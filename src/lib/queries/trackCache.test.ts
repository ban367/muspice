import { QueryClient } from '@tanstack/svelte-query';
import { describe, expect, it } from 'vitest';
import type { AlbumGroup, Track } from '#lib/types/models.js';
import { queryKeys } from './keys';
import { patchTrackData, patchTrackInCache } from './trackCache';

function track(id: string, rating = 0): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: null,
    album: null,
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
    rating,
    playCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null }
  };
}

function album(name: string, tracks: Track[]): AlbumGroup {
  return {
    name,
    artist: null,
    trackCount: tracks.length,
    totalDuration: 0,
    representativeTrackId: tracks[0]?.id ?? '',
    tracks
  };
}

describe('patchTrackData', () => {
  it('曲の一覧の中の1曲を書き換える（元のデータは変えない）', () => {
    const tracks = [track('a'), track('b')];

    const patched = patchTrackData(tracks, 'b', { rating: 5 });

    expect(patched.map((t) => t.rating)).toEqual([0, 5]);
    expect(tracks[1].rating).toBe(0);
    // 書き換えていない曲は同じ参照のまま
    expect(patched[0]).toBe(tracks[0]);
  });

  it('アルバムごとの曲の中の1曲を書き換える', () => {
    const albums = [album('X', [track('a')]), album('Y', [track('b'), track('c')])];

    const patched = patchTrackData(albums, 'c', { isFavorite: true });

    expect(patched[1].tracks.map((t) => t.isFavorite)).toEqual([false, true]);
    expect(patched[1].name).toBe('Y');
    // 書き換えていないアルバムは同じ参照のまま
    expect(patched[0]).toBe(albums[0]);
  });

  it('その曲を含まないデータは、同じ参照のまま返す', () => {
    const tracks = [track('a')];
    const albums = [album('X', [track('a')])];
    const empty: Track[] = [];

    expect(patchTrackData(tracks, 'z', { rating: 5 })).toBe(tracks);
    expect(patchTrackData(albums, 'z', { rating: 5 })).toBe(albums);
    expect(patchTrackData(empty, 'z', { rating: 5 })).toBe(empty);
    expect(patchTrackData(undefined, 'z', { rating: 5 })).toBeUndefined();
  });
});

describe('patchTrackInCache', () => {
  it('曲を返すすべてのクエリのキャッシュを書き換え、ほかのクエリは変えない', () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.tracks.list, [track('a'), track('b')]);
    queryClient.setQueryData(queryKeys.tracks.album('X'), [track('b')]);
    queryClient.setQueryData(queryKeys.tracks.artistAlbums('Z'), [album('X', [track('b')])]);
    queryClient.setQueryData(queryKeys.tracks.playlist('p1'), [track('a')]);
    const albumList = [{ name: 'X', trackCount: 1 }];
    queryClient.setQueryData(queryKeys.albums.list, albumList);

    patchTrackInCache(queryClient, 'b', { rating: 4 });

    expect(queryClient.getQueryData<Track[]>(queryKeys.tracks.list)?.[1].rating).toBe(4);
    expect(queryClient.getQueryData<Track[]>(queryKeys.tracks.album('X'))?.[0].rating).toBe(4);
    expect(
      queryClient.getQueryData<AlbumGroup[]>(queryKeys.tracks.artistAlbums('Z'))?.[0].tracks[0]
        .rating
    ).toBe(4);
    expect(queryClient.getQueryData<Track[]>(queryKeys.tracks.playlist('p1'))?.[0].rating).toBe(0);
    expect(queryClient.getQueryData(queryKeys.albums.list)).toBe(albumList);
  });
});
