import { describe, expect, it } from 'vitest';
import { albumArtVersion, refreshAlbumArt } from './albumArt.svelte';

describe('albumArtVersion', () => {
  it('書き換えていない曲は0、読み直させるたびに増える', () => {
    expect(albumArtVersion('version-a')).toBe(0);

    refreshAlbumArt(['version-a']);
    refreshAlbumArt(new Set(['version-a', 'version-b']));

    expect(albumArtVersion('version-a')).toBe(2);
    expect(albumArtVersion('version-b')).toBe(1);
    expect(albumArtVersion('version-c')).toBe(0);
  });
});
