import { beforeEach, describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { albumArtDialog, albumArtVersion, refreshAlbumArt } from './albumArt.svelte';

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

describe('albumArtDialog', () => {
  const tracks = [{ id: 'a' }, { id: 'b' }] as Track[];

  beforeEach(() => albumArtDialog.close());

  it('曲を指定して開き、閉じる', () => {
    expect(albumArtDialog.tracks).toBeNull();

    albumArtDialog.open(tracks);
    expect(albumArtDialog.tracks).toBe(tracks);

    albumArtDialog.close();
    expect(albumArtDialog.tracks).toBeNull();
  });

  it('曲がなければ開かない', () => {
    albumArtDialog.open([]);
    expect(albumArtDialog.tracks).toBeNull();
  });
});
