import { describe, expect, it } from 'vitest';
import { albumKey } from './albumKey.js';

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
