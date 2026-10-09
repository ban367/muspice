import { describe, expect, it, vi } from 'vitest';
import { refreshAlbumArt } from '#lib/stores/albumArt.svelte.js';
import { albumArtUrl } from './albumArt';

const convertFileSrc = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc }));

describe('albumArtUrl', () => {
  it('トラックIDから、albumartプロトコルのURLを作る', () => {
    convertFileSrc.mockImplementation((path, protocol) => `${protocol}://localhost/${path}`);

    expect(albumArtUrl('track-a')).toBe('albumart://localhost/track-a');
    expect(albumArtUrl(null)).toBeNull();
    expect(albumArtUrl(undefined)).toBeNull();
    expect(albumArtUrl('')).toBeNull();
  });

  it('アルバムアートを書き換えた曲だけ、URLに版を付けて読み直させる', () => {
    convertFileSrc.mockImplementation((path, protocol) => `${protocol}://localhost/${path}`);

    refreshAlbumArt(['track-b', 'track-c']);
    refreshAlbumArt(['track-b']);

    expect(albumArtUrl('track-b')).toBe('albumart://localhost/track-b?v=2');
    expect(albumArtUrl('track-c')).toBe('albumart://localhost/track-c?v=1');
    expect(albumArtUrl('track-d')).toBe('albumart://localhost/track-d');
  });

  it('data URL（ブラウザ確認用のモック）には、版を付けない', () => {
    convertFileSrc.mockImplementation(() => 'data:image/svg+xml;base64,AAAA');

    refreshAlbumArt(['track-e']);

    expect(albumArtUrl('track-e')).toBe('data:image/svg+xml;base64,AAAA');
  });
});
