import { describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { createTrackSorter } from './trackSort.js';

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
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    ...overrides
  };
}

const ids = (tracks: Track[]) => tracks.map((t) => t.id);

describe('createTrackSorter（並び替え）', () => {
  const sortTracks = createTrackSorter();

  it('タイトルで並べる（タイトルがない曲はファイル名を使う）', () => {
    const tracks = [
      track('a', { title: 'りんご' }),
      track('b', { title: null, fileName: 'あんず.mp3' }),
      track('c', { title: 'みかん' })
    ];

    expect(ids(sortTracks(tracks, 'title', 'asc'))).toEqual(['b', 'c', 'a']);
    expect(ids(sortTracks(tracks, 'title', 'desc'))).toEqual(['a', 'c', 'b']);
  });

  it('アーティスト・アルバムがない曲は、空の文字列として先頭に並べる', () => {
    const tracks = [
      track('a', { artist: 'B', album: 'Y' }),
      track('b', { artist: null, album: 'X' }),
      track('c', { artist: 'A', album: null })
    ];

    expect(ids(sortTracks(tracks, 'artist', 'asc'))).toEqual(['b', 'c', 'a']);
    expect(ids(sortTracks(tracks, 'album', 'asc'))).toEqual(['c', 'b', 'a']);
  });

  it('長さは数値として並べる（長さがない曲は0）', () => {
    const tracks = [
      track('a', { duration: 100 }),
      track('b', { duration: null }),
      track('c', { duration: 9 })
    ];

    expect(ids(sortTracks(tracks, 'duration', 'asc'))).toEqual(['b', 'c', 'a']);
  });

  it('追加日時で並べる', () => {
    const tracks = [
      track('a', { createdAt: '2026-01-02T00:00:00.000Z' }),
      track('b', { createdAt: '2026-01-03T00:00:00.000Z' }),
      track('c', { createdAt: '2026-01-01T00:00:00.000Z' })
    ];

    expect(ids(sortTracks(tracks, 'createdAt', 'desc'))).toEqual(['b', 'a', 'c']);
  });

  it('同じ値の曲は、昇順・降順とも元の順を保つ', () => {
    const tracks = [
      track('a', { artist: 'X' }),
      track('b', { artist: 'X' }),
      track('c', { artist: 'A' })
    ];

    expect(ids(sortTracks(tracks, 'artist', 'asc'))).toEqual(['c', 'a', 'b']);
    expect(ids(sortTracks(tracks, 'artist', 'desc'))).toEqual(['a', 'b', 'c']);
  });

  it('元の配列は変えない', () => {
    const tracks = [track('b'), track('a')];

    sortTracks(tracks, 'title', 'asc');

    expect(ids(tracks)).toEqual(['b', 'a']);
  });
});

describe('createTrackSorter（直前の結果の再利用）', () => {
  it('並び替えに使う値が同じなら、比較をやり直さず新しい曲の内容を返す', () => {
    const sort = createTrackSorter();
    const tracks = [track('b', { title: 'B' }), track('a', { title: 'A' })];
    expect(ids(sort(tracks, 'title', 'asc'))).toEqual(['a', 'b']);

    // 評価の変更で一覧が作り直された場合
    const compare = vi.spyOn(Intl.Collator.prototype, 'compare', 'get');
    const rated = [{ ...tracks[0], rating: 5 }, tracks[1]];
    const sorted = sort(rated, 'title', 'asc');
    expect(compare).not.toHaveBeenCalled();
    compare.mockRestore();

    expect(ids(sorted)).toEqual(['a', 'b']);
    expect(sorted[1].rating).toBe(5);
  });

  it('並び替えに使う値・項目・向き・曲が変わったら、並べ直す', () => {
    const sort = createTrackSorter();
    const tracks = [track('b', { title: 'B' }), track('a', { title: 'A' })];
    expect(ids(sort(tracks, 'title', 'asc'))).toEqual(['a', 'b']);

    // 並び替えに使う値が変わった
    const renamed = [{ ...tracks[0], title: '0' }, tracks[1]];
    expect(ids(sort(renamed, 'title', 'asc'))).toEqual(['b', 'a']);

    // 向きが変わった
    expect(ids(sort(renamed, 'title', 'desc'))).toEqual(['a', 'b']);

    // 曲が増えた
    const added = [...renamed, track('c', { title: '1' })];
    expect(ids(sort(added, 'title', 'desc'))).toEqual(['a', 'c', 'b']);

    // 同じ数の別の曲に変わった
    const replaced = [track('x', { title: 'Z' }), track('y', { title: 'Y' })];
    expect(ids(sort(replaced, 'title', 'asc'))).toEqual(['y', 'x']);
  });
});
