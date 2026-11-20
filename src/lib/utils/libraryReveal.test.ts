import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Track } from '#lib/types/models.js';

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => `/${path}` }));

import { goto } from '$app/navigation';
import { ui } from '#lib/stores/ui.svelte.js';
import {
  albumOf,
  artistsOf,
  goToAlbum,
  goToArtist,
  matchesReveal,
  revealAlbumArtistKey,
  revealAlbumKey
} from './libraryReveal.js';
import { albumArtistKey } from './albumArtists.js';
import { albumKey } from './albumKey.js';

function track(overrides: Partial<Track> = {}): Track {
  return {
    album: 'Album',
    albumArtist: null,
    artist: 'Artist',
    compilation: false,
    ...overrides
  } as Track;
}

beforeEach(() => {
  vi.clearAllMocks();
  ui.revealGroup = null;
  ui.browseSearchQuery = '';
  ui.isNowPlayingOpen = false;
});

describe('albumOf', () => {
  it('アルバムは、アルバム名とまとめたアーティスト（アルバムの一覧と同じ）で決める', () => {
    expect(albumOf(track())).toEqual({ kind: 'album', name: 'Album', artist: 'Artist' });
    expect(albumOf(track({ albumArtist: 'Band', artist: 'Band feat. Guest' }))).toEqual({
      kind: 'album',
      name: 'Album',
      artist: 'Band'
    });
    // コンピレーションの印があり、アルバムアーティストのない曲は、Various Artistsのアルバム
    expect(albumOf(track({ compilation: true }))?.artist).toBe('Various Artists');
    // アーティストのない曲のアルバム
    expect(albumOf(track({ artist: null }))?.artist).toBeNull();
  });

  it('アルバムのない曲は、移る先がない', () => {
    expect(albumOf(track({ album: null }))).toBeNull();
    expect(albumOf(track({ album: '' }))).toBeNull();
  });
});

describe('artistsOf', () => {
  it('アルバムアーティストがあれば、それだけ', () => {
    expect(artistsOf(track({ albumArtist: 'Band', artist: 'A; B' }))).toEqual(['Band']);
  });

  it('アルバムアーティストのないコンピレーションの曲は、Various Artists', () => {
    expect(artistsOf(track({ compilation: true, artist: 'Someone' }))).toEqual(['Various Artists']);
  });

  it('ほかの曲は、曲のアーティストのそれぞれの値（ない曲は空）', () => {
    expect(artistsOf(track({ artist: 'A; B ; A' }))).toEqual(['A', 'B']);
    expect(artistsOf(track({ artist: null }))).toEqual([]);
  });
});

describe('matchesReveal', () => {
  const album = { kind: 'album', name: 'Greatest Hits', artist: 'A' } as const;

  it('アルバムは、名前とまとめたアーティストの両方で見分ける', () => {
    expect(matchesReveal(album, { name: 'Greatest Hits', artist: 'A' })).toBe(true);
    expect(matchesReveal(album, { name: 'Greatest Hits', artist: 'B' })).toBe(false);
    expect(matchesReveal(album, { name: 'Other', artist: 'A' })).toBe(false);
    // アーティストの一覧の項目（アーティストを持たない）には合わない
    expect(matchesReveal(album, { name: 'Greatest Hits' })).toBe(false);
  });

  it('アーティストは、名前で見分ける', () => {
    expect(matchesReveal({ kind: 'artist', name: 'A' }, { name: 'A' })).toBe(true);
    expect(matchesReveal({ kind: 'artist', name: 'A' }, { name: 'B' })).toBe(false);
  });

  it('グリッド・リスト表示のキーは、それぞれの一覧のキーと同じ', () => {
    expect(revealAlbumKey(album)).toBe(albumKey({ name: 'Greatest Hits', artist: 'A' }));
    expect(revealAlbumArtistKey(album)).toBe(albumArtistKey('A'));
    expect(revealAlbumArtistKey({ ...album, artist: null })).toBe(albumArtistKey(null));
  });
});

describe('goToAlbum / goToArtist', () => {
  it('アルバムの画面を開き、選ぶアルバムを置く（検索とNow Playingの画面は閉じる）', async () => {
    ui.browseSearchQuery = 'abc';
    ui.isNowPlayingOpen = true;
    await goToAlbum(track({ albumArtist: 'Band' }));
    expect(ui.revealGroup).toEqual({ kind: 'album', name: 'Album', artist: 'Band' });
    expect(ui.browseSearchQuery).toBe('');
    expect(ui.isNowPlayingOpen).toBe(false);
    expect(goto).toHaveBeenCalledWith('/library/albums');
  });

  it('アルバムのない曲では、何もしない', async () => {
    await goToAlbum(track({ album: null }));
    expect(ui.revealGroup).toBeNull();
    expect(goto).not.toHaveBeenCalled();
  });

  it('アーティストの画面を開き、選ぶアーティストを置く', async () => {
    await goToArtist('B');
    expect(ui.revealGroup).toEqual({ kind: 'artist', name: 'B' });
    expect(goto).toHaveBeenCalledWith('/library/artists');
  });
});
