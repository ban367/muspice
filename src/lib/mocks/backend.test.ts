import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { commands } from '#lib/bindings.js';
import { createMockBackend, toCommandName } from './backend';
import { mockPlaylistId, mockTrackId } from './fixtures';

/** バックエンドから送信されたイベント */
let events: { event: string; payload: unknown }[];

beforeEach(() => {
  // Node環境には`window`がないため、`__TAURI_INTERNALS__`の置き場所として空オブジェクトを用意する
  vi.stubGlobal('window', {});
  events = [];
  const backend = createMockBackend({
    emit: (event, payload) => events.push({ event, payload }),
    sleep: async () => {}
  });
  mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('コマンドの対応付け', () => {
  it('bindings.tsの全コマンドのIPC名がハンドラ名に変換できる', async () => {
    const invoked: string[] = [];
    mockIPC((cmd) => {
      invoked.push(cmd);
      return null;
    });

    // 引数は検証しないため、すべて省略して呼び出す
    const allCommands: ((...args: never[]) => Promise<unknown>)[] = Object.values(commands);
    for (const command of allCommands) {
      await command();
    }

    expect(invoked.map(toCommandName)).toEqual(Object.keys(commands));
  });

  it('複数の引数を宣言順のとおり位置引数として受け取る', async () => {
    const trackId = mockTrackId(2);
    await commands.setRating(trackId, 3);

    const tracks = await commands.getAllTracks();
    expect(tracks.find((track) => track.id === trackId)?.rating).toBe(3);
  });

  it('戻り値は複製され、書き換えてもバックエンドの状態に影響しない', async () => {
    const [first] = await commands.getAllTracks();
    first.title = '書き換え';

    const [again] = await commands.getAllTracks();
    expect(again.title).not.toBe('書き換え');
  });
});

describe('エラー', () => {
  it('Rust側と同じ{ code, message }形式でrejectする', async () => {
    await expect(commands.setRating(mockTrackId(1), 6)).rejects.toEqual({
      code: 'VALIDATION',
      message: 'レーティングは0から5の間で指定してください'
    });
    await expect(commands.toggleFavorite('invalid')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.toggleFavorite(mockTrackId(999))).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });
});

describe('トラック', () => {
  it('一覧は追加日時の新しい順に返す', async () => {
    const tracks = await commands.getAllTracks();
    const createdAts = tracks.map((track) => track.createdAt);

    expect(createdAts).toEqual([...createdAts].sort().reverse());
  });

  it('検索は大文字小文字を区別せず部分一致し、空のクエリは空配列を返す', async () => {
    const tracks = await commands.searchTracks('voltage');

    expect(tracks.length).toBeGreaterThan(0);
    expect(tracks.every((track) => track.artist === 'The Voltage')).toBe(true);
    expect(await commands.searchTracks(' ;" ')).toEqual([]);
  });

  it('アーティスト別グループでは、アルバム未設定のトラックを「不明なアルバム」にまとめる', async () => {
    const artists = await commands.getArtistsGrouped();
    const voltage = artists.find((artist) => artist.name === 'The Voltage');

    expect(voltage?.albums.map((album) => album.name)).toContain('不明なアルバム');
    expect(voltage?.trackCount).toBe(
      voltage?.albums.reduce((total, album) => total + album.trackCount, 0)
    );
  });

  it('一括編集は指定したフィールドだけを更新する', async () => {
    const ids = [mockTrackId(1), mockTrackId(2)];
    await commands.updateMultipleTracksMetadata(ids, { genre: 'Rock' });

    const tracks = (await commands.getAllTracks()).filter((track) => ids.includes(track.id));
    expect(tracks.map((track) => track.genre)).toEqual(['Rock', 'Rock']);
    expect(tracks.every((track) => track.artist === 'Aoi Sora')).toBe(true);
  });
});

describe('プレイリスト', () => {
  it('追加・並び替え・削除でpositionを連番に保つ', async () => {
    const playlist = await commands.createPlaylist('テスト');
    await commands.addTrackToPlaylist(playlist.id, mockTrackId(1));
    await commands.addTrackToPlaylist(playlist.id, mockTrackId(2));
    await commands.addTrackToPlaylist(playlist.id, mockTrackId(3));
    await commands.reorderPlaylistTracks(playlist.id, [
      mockTrackId(3),
      mockTrackId(1),
      mockTrackId(2)
    ]);
    await commands.removeTrackFromPlaylist(playlist.id, mockTrackId(1));

    const saved = (await commands.getPlaylists()).find((p) => p.id === playlist.id);
    expect(saved?.tracks.map(({ trackId, position }) => [trackId, position])).toEqual([
      [mockTrackId(3), 0],
      [mockTrackId(2), 1]
    ]);
  });

  it('トラックを削除するとプレイリストからも取り除かれる', async () => {
    await commands.deleteTracksCommand([mockTrackId(7)]);

    const drive = (await commands.getPlaylists()).find((p) => p.id === mockPlaylistId(1));
    expect(drive?.tracks.some((entry) => entry.trackId === mockTrackId(7))).toBe(false);
  });

  it('使用できない文字を含む名前はVALIDATIONエラーになる', async () => {
    await expect(commands.createPlaylist('a/b')).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});

describe('インポート', () => {
  it('進捗イベントを送信し、同じフォルダの再インポートは重複としてスキップする', async () => {
    const first = await commands.importFolder('/Users/demo/Music/New', 'Skip');
    const second = await commands.importFolder('/Users/demo/Music/New', 'Skip');

    expect(first).toMatchObject({ importedCount: 6, skippedCount: 0 });
    expect(second).toMatchObject({ importedCount: 0, skippedCount: 6 });
    expect(events[0]).toEqual({
      event: 'import-progress',
      payload: { current: 1, total: 6, currentFile: 'Mock Track 01.mp3' }
    });
  });

  it('親ディレクトリを含むパスはVALIDATIONエラーになる', async () => {
    await expect(commands.importFolder('/Users/demo/../etc', 'Skip')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
  });
});
