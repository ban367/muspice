import { describe, expect, it } from 'vitest';
import { albumKey, groupArtist, VARIOUS_ARTISTS } from './albumKey.js';

describe('albumKey', () => {
  it('同じ名前でもアーティストが違うアルバムを区別する', () => {
    expect(albumKey({ name: 'Greatest Hits', artist: 'A' })).not.toBe(
      albumKey({ name: 'Greatest Hits', artist: 'B' })
    );
    expect(albumKey({ name: 'Greatest Hits', artist: 'A' })).toBe(
      albumKey({ name: 'Greatest Hits', artist: 'A' })
    );
  });

  it('アーティストのないアルバムと、名前・アーティストの区切りが紛らわしいアルバムを区別する', () => {
    expect(albumKey({ name: 'X', artist: null })).not.toBe(albumKey({ name: 'X', artist: '' }));
    expect(albumKey({ name: 'b,c', artist: 'a' })).not.toBe(albumKey({ name: 'c', artist: 'a,b' }));
  });
});

describe('groupArtist', () => {
  it('アルバムアーティストがあればそれを、なければ曲のアーティストを返す', () => {
    const track = { compilation: false };
    expect(groupArtist({ ...track, albumArtist: 'Various Artists', artist: 'A' })).toBe(
      'Various Artists'
    );
    expect(groupArtist({ ...track, albumArtist: null, artist: 'A' })).toBe('A');
    expect(groupArtist({ ...track, albumArtist: null, artist: null })).toBeNull();
  });

  it('アルバムアーティストのないコンピレーションは「Various Artists」にまとめる（ADR-093）', () => {
    const compilation = { compilation: true };
    expect(groupArtist({ ...compilation, albumArtist: null, artist: 'A' })).toBe(VARIOUS_ARTISTS);
    expect(groupArtist({ ...compilation, albumArtist: null, artist: null })).toBe(VARIOUS_ARTISTS);
    // アルバムアーティストがあれば、印があってもアルバムアーティストでまとめる
    expect(groupArtist({ ...compilation, albumArtist: 'DJ X', artist: 'A' })).toBe('DJ X');
  });
});
