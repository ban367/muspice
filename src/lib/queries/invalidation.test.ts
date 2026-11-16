import { QueryClient } from '@tanstack/svelte-query';
import { describe, expect, it } from 'vitest';
import { queryKeys } from './keys.js';
import { invalidatePlaylistQueries } from './playlists.js';
import { invalidateLibraryQueries } from './tracks.js';
import { applyTracksEdited } from './trackEditor.js';
import { albumArtVersion } from '#lib/stores/albumArt.svelte.js';

/** キャッシュにデータを入れたクエリクライアント（取り直しは起きない。無効化の印だけを見る） */
function clientWith(keys: readonly (readonly unknown[])[]): QueryClient {
  const client = new QueryClient();
  for (const key of keys) client.setQueryData(key, []);
  return client;
}

function isInvalidated(client: QueryClient, key: readonly unknown[]): boolean | undefined {
  return client.getQueryState(key)?.isInvalidated;
}

describe('invalidatePlaylistQueries', () => {
  const keys = [
    queryKeys.playlists,
    queryKeys.tracks.playlist('a'),
    queryKeys.tracks.playlist('ab'),
    queryKeys.tracks.playlist('smart')
  ];

  it('曲を変えたプレイリストの曲だけを取り直す（開いている自動プレイリストは取り直さない）', () => {
    const client = clientWith(keys);

    invalidatePlaylistQueries(client, 'a');

    expect(isInvalidated(client, queryKeys.playlists)).toBe(true);
    expect(isInvalidated(client, queryKeys.tracks.playlist('a'))).toBe(true);
    // 前方一致にならない（IDの先頭が同じ別のプレイリスト）
    expect(isInvalidated(client, queryKeys.tracks.playlist('ab'))).toBe(false);
    expect(isInvalidated(client, queryKeys.tracks.playlist('smart'))).toBe(false);
  });

  it('一覧だけを変えた時（移動・削除・名前）は、プレイリストの曲を取り直さない', () => {
    const client = clientWith(keys);

    invalidatePlaylistQueries(client, null);

    expect(isInvalidated(client, queryKeys.playlists)).toBe(true);
    for (const id of ['a', 'ab', 'smart']) {
      expect(isInvalidated(client, queryKeys.tracks.playlist(id))).toBe(false);
    }
  });
});

describe('invalidateLibraryQueries', () => {
  it('曲・プレイリストと、ライブラリフォルダ・音楽フォルダを無効化する', () => {
    const client = clientWith([
      queryKeys.tracks.list,
      queryKeys.tracks.playlist('a'),
      queryKeys.albums.list,
      queryKeys.playlists,
      queryKeys.libraryFolders,
      queryKeys.musicFolder,
      queryKeys.settings
    ]);

    invalidateLibraryQueries(client);

    for (const key of [
      queryKeys.tracks.list,
      queryKeys.tracks.playlist('a'),
      queryKeys.albums.list,
      queryKeys.playlists,
      queryKeys.libraryFolders,
      queryKeys.musicFolder
    ]) {
      expect(isInvalidated(client, key), JSON.stringify(key)).toBe(true);
    }
    // ライブラリと関係のないクエリは、そのまま
    expect(isInvalidated(client, queryKeys.settings)).toBe(false);
  });
});

describe('applyTracksEdited', () => {
  it('曲の一覧とタグを取り直し、アルバムアートを書き換えた曲だけ画像を読み直させる', () => {
    const client = clientWith([
      queryKeys.tracks.list,
      queryKeys.albums.list,
      queryKeys.trackLyrics.track('a'),
      queryKeys.albumArtInfo.track('a'),
      queryKeys.playlists
    ]);
    client.setQueryData(queryKeys.trackTags.track('a'), {});

    applyTracksEdited(client, { trackIds: ['edited-tags'], albumArt: false });

    expect(isInvalidated(client, queryKeys.tracks.list)).toBe(true);
    expect(isInvalidated(client, queryKeys.albums.list)).toBe(true);
    expect(isInvalidated(client, queryKeys.trackLyrics.track('a'))).toBe(true);
    // ファイルから読んだタグは捨てる（次に開く時に読み直す）
    expect(client.getQueryState(queryKeys.trackTags.track('a'))).toBeUndefined();
    // プレイリスト（入っている曲）は変わらない
    expect(isInvalidated(client, queryKeys.playlists)).toBe(false);
    expect(isInvalidated(client, queryKeys.albumArtInfo.track('a'))).toBe(false);
    expect(albumArtVersion('edited-tags')).toBe(0);

    applyTracksEdited(client, { trackIds: ['edited-art'], albumArt: true });

    expect(isInvalidated(client, queryKeys.albumArtInfo.track('a'))).toBe(true);
    expect(albumArtVersion('edited-art')).toBe(1);
  });
});
