import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { commands } from '#lib/bindings.js';
import { createMockBackend, toCommandName, type MockBackend } from './backend';
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

describe('大量のトラック', () => {
  it('extraTrackCountで指定した数のトラックを、フィクスチャに加えて上限なく返す', async () => {
    const fixtureCount = (await commands.getAllTracks()).length;
    const backend = createMockBackend({ emit: () => {}, extraTrackCount: 2500 });
    mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));

    const tracks = await commands.getAllTracks();
    expect(tracks).toHaveLength(fixtureCount + 2500);
    expect(new Set(tracks.map((track) => track.id)).size).toBe(tracks.length);
    // 一覧の先頭には、フィクスチャが並ぶ
    expect(tracks.slice(0, fixtureCount).some((track) => track.title?.startsWith('Bulk'))).toBe(
      false
    );
    expect(tracks[fixtureCount].title).toBe('Bulk Track 000001');

    const albums = await commands.getAlbums();
    expect(albums.reduce((total, album) => total + album.trackCount, 0)).toBe(
      tracks.filter((track) => track.album !== null).length
    );
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

  it('アルバムの一覧は曲を持たず、曲数・長さ・代表の曲がアルバムの曲と一致する', async () => {
    const albums = await commands.getAlbums();
    const names = albums.map((album) => album.name);

    expect(names).toEqual([...names].sort((a, b) => (a.toLowerCase() < b.toLowerCase() ? -1 : 1)));
    for (const album of albums) {
      const tracks = await commands.getAlbumTracks(album.name, album.artist);
      expect(album.trackCount).toBe(tracks.length);
      expect(album.totalDuration).toBe(tracks.reduce((sum, t) => sum + (t.duration ?? 0), 0));
      expect(album.representativeTrackId).toBe(tracks[0].id);
      // アルバムのアーティストは、アルバムアーティスト（なければ曲のアーティスト）
      expect(tracks.every((t) => (t.albumArtist ?? t.artist) === album.artist)).toBe(true);
      expect('tracks' in album).toBe(false);
    }
    // すべてのアルバムの曲を合わせると、アルバムのある曲の数になる
    const albumTrackCount = (await commands.getAllTracks()).filter((t) => t.album !== null).length;
    expect(albums.reduce((total, album) => total + album.trackCount, 0)).toBe(albumTrackCount);
  });

  it('アルバムの曲はトラック番号の順に返し、見つからないアルバムは空配列を返す', async () => {
    const numbers = (await commands.getAlbumTracks('Blue Horizon', 'Aoi Sora')).map(
      (t) => t.trackNumber
    );

    expect(numbers).toEqual([1, 2, 3, 4]);
    expect(await commands.getAlbumTracks('ないアルバム', 'Aoi Sora')).toEqual([]);
    expect(await commands.getAlbumTracks('Blue Horizon', '別のアーティスト')).toEqual([]);
  });

  it('アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる', async () => {
    const albums = await commands.getAlbums();
    const find = (name: string) => albums.filter((album) => album.name === name);

    // コンピレーションは、曲ごとのアーティストが違っても1つのアルバム
    expect(find('City Nights Collection')).toMatchObject([
      { artist: 'Various Artists', trackCount: 3 }
    ]);
    // フィーチャリングの曲は、アルバムアーティストのアルバムに入る
    expect(find('Field Notes')).toMatchObject([{ artist: 'Aoi Sora', trackCount: 3 }]);
    // 別のアーティストの同じ名前のアルバムは、別のアルバム
    expect(find('Sketches').map((album) => album.artist)).toEqual(['Kenji Mori', 'The Voltage']);

    const compilation = await commands.getAlbumTracks('City Nights Collection', 'Various Artists');
    expect(compilation.map((t) => t.artist)).toEqual(['ネオン通り', 'Aoi Sora', 'Mika Hayashi']);
  });

  it('アーティストはアルバムアーティストでまとめ、コンピレーションの参加アーティストごとに分けない', async () => {
    const artists = await commands.getArtists();
    const names = artists.map((artist) => artist.name);

    expect(names).toContain('Various Artists');
    expect(names).not.toContain('Aoi Sora feat. Mika Hayashi');

    // 曲ごとのアーティストが違う曲も、アルバムアーティストのアルバムに入る
    const aoi = await commands.getArtistAlbums('Aoi Sora');
    const fieldNotes = aoi.find((album) => album.name === 'Field Notes');
    expect(fieldNotes?.tracks.map((t) => t.artist)).toEqual([
      'Aoi Sora',
      'Aoi Sora',
      'Aoi Sora feat. Mika Hayashi'
    ]);
    // コンピレーションの曲は、参加アーティストのアルバムには入らない
    expect(aoi.map((album) => album.name)).not.toContain('City Nights Collection');
    expect(await commands.searchTracks('various artists')).toHaveLength(3);
  });

  it('アーティストのアルバムでは、アルバム未設定のトラックを「不明なアルバム」にまとめる', async () => {
    const artist = (await commands.getArtists()).find((a) => a.name === 'The Voltage');
    const albums = await commands.getArtistAlbums('The Voltage');

    expect(albums.map((album) => album.name)).toContain('不明なアルバム');
    expect(artist?.albumCount).toBe(albums.length);
    expect(artist?.trackCount).toBe(albums.reduce((total, album) => total + album.trackCount, 0));
    expect(artist?.representativeTrackId).toBe(albums[0].representativeTrackId);
  });

  it('ジャンルの一覧は、曲数と代表の曲がジャンルの曲と一致する', async () => {
    const genres = await commands.getGenres();

    expect(genres.length).toBeGreaterThan(0);
    for (const genre of genres) {
      const tracks = await commands.getGenreTracks(genre.name);
      expect(genre.trackCount).toBe(tracks.length);
      expect(genre.representativeTrackId).toBe(tracks[0].id);
      expect(tracks.every((track) => track.genre === genre.name)).toBe(true);
    }
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
    const added = await commands.addTracksToPlaylist(playlist.id, [
      mockTrackId(1),
      mockTrackId(2),
      mockTrackId(3)
    ]);
    expect(added).toBe(3);
    // すでに入っている曲は飛ばし、追加した数だけを返す
    expect(await commands.addTracksToPlaylist(playlist.id, [mockTrackId(2), mockTrackId(3)])).toBe(
      0
    );
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

  it('プレイリストの曲を、プレイリストの中の並び順で返す', async () => {
    const tracks = await commands.getPlaylistTracks(mockPlaylistId(1));
    expect(tracks.map((track) => track.id)).toEqual([7, 8, 1, 10, 12].map(mockTrackId));
    expect(tracks[0].title).toBe('夜明けのシグナル');

    await commands.reorderPlaylistTracks(mockPlaylistId(1), [12, 7, 8, 1, 10].map(mockTrackId));
    const reordered = await commands.getPlaylistTracks(mockPlaylistId(1));
    expect(reordered.map((track) => track.id)).toEqual([12, 7, 8, 1, 10].map(mockTrackId));

    // 見つからないプレイリストは空配列（不正なIDはVALIDATION）
    expect(await commands.getPlaylistTracks(mockPlaylistId(999))).toEqual([]);
    await expect(commands.getPlaylistTracks('invalid')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
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

describe('ライブラリフォルダ', () => {
  it('インポートしたフォルダを登録し、入れ子のフォルダはまとめる', async () => {
    await commands.importFolder('/Users/demo/Imports/A', 'Skip');
    await commands.importFolder('/Users/demo/Imports/A/Live', 'Skip');
    let paths = (await commands.getLibraryFolders()).folders.map((f) => f.path);
    expect(paths).toContain('/Users/demo/Imports/A');
    expect(paths).not.toContain('/Users/demo/Imports/A/Live');

    await commands.importFolder('/Users/demo/Imports', 'Skip');
    paths = (await commands.getLibraryFolders()).folders.map((f) => f.path);
    expect(paths).toContain('/Users/demo/Imports');
    expect(paths).not.toContain('/Users/demo/Imports/A');
  });

  it('末尾の区切り文字を除いて登録し、ルートのフォルダもRustと同じ接頭辞で数える', async () => {
    await commands.importFolder('/Users/demo/Imports/B/', 'Skip');
    let folders = (await commands.getLibraryFolders()).folders;
    const imported = folders.find((f) => f.path === '/Users/demo/Imports/B');
    expect(imported?.trackCount).toBeGreaterThan(0);
    // インポートしたら、設定ウィンドウの一覧も読み直せるよう変更を通知する
    expect(events.map((e) => e.event)).toContain('library-changed');

    await commands.importFolder('/', 'Skip');
    folders = (await commands.getLibraryFolders()).folders;
    // ルートは区切り文字を残し、すべての曲を含む
    expect(folders.map((f) => f.path)).toEqual(['/']);
    expect(folders[0].trackCount).toBe((await commands.getAllTracks()).length);
  });

  it('トラック数を数え、どのフォルダにも属さない曲を区別する', async () => {
    const before = await commands.getLibraryFolders();
    const music = before.folders.find((f) => f.path === '/Users/demo/Music');
    expect(music?.trackCount).toBeGreaterThan(0);
    expect(before.unregisteredTrackCount).toBe(0);

    await commands.removeLibraryFolder(music!.id, false);
    const after = await commands.getLibraryFolders();
    expect(after.unregisteredTrackCount).toBe(music!.trackCount);
  });

  it('フォルダ内の曲もライブラリから外すと、変更を通知する', async () => {
    const { folders } = await commands.getLibraryFolders();
    const music = folders.find((f) => f.path === '/Users/demo/Music')!;

    await expect(commands.removeLibraryFolder(music.id, true)).resolves.toBe(music.trackCount);
    expect(await commands.getAllTracks()).toHaveLength(0);
    expect(events.map((e) => e.event)).toContain('library-changed');
  });

  it('見つからないフォルダの再スキャンはNOT_FOUNDエラーになる', async () => {
    const { folders } = await commands.getLibraryFolders();
    const missing = folders.find((f) => !f.exists)!;

    await expect(commands.rescanLibraryFolder(missing.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });
});

describe('転送先デバイス', () => {
  /** 接続されているデバイス（プレイリスト「ドライブ用」を同期する） */
  async function connectedDevice() {
    return (await commands.getSyncDevices()).find((d) => d.connected)!;
  }

  it('名前順に返し、接続されていないデバイスは容量を返さない', async () => {
    const devices = await commands.getSyncDevices();
    expect(devices.map((d) => d.name)).toEqual(['SDカード', 'Walkman']);

    const [connected, disconnected] = devices;
    expect(connected.freeBytes).toBeGreaterThan(0);
    expect(connected.totalBytes).toBeGreaterThan(connected.freeBytes!);
    expect(disconnected).toMatchObject({ connected: false, freeBytes: null, totalBytes: null });
  });

  it('フォルダを登録し、ライブラリフォルダと重なるフォルダは拒否する', async () => {
    const device = await commands.registerSyncDevice('/Volumes/NEW_SD/', '  新しいSD ');
    expect(device).toMatchObject({
      name: '新しいSD',
      path: '/Volumes/NEW_SD',
      connected: true,
      syncAll: false,
      playlistIds: [],
      removeUnselected: true
    });
    expect((await commands.getSyncDevices()).map((d) => d.id)).toContain(device.id);

    await expect(commands.registerSyncDevice('/Users/demo/Music/Sub', 'SD')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.registerSyncDevice('/Users', 'SD')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.registerSyncDevice('/Volumes/OTHER', '   ')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
  });

  it('設定を更新し、見つからないプレイリストは無視する', async () => {
    const device = await connectedDevice();
    const updated = await commands.updateSyncDevice(device.id, {
      name: '車',
      syncAll: true,
      playlistIds: [mockPlaylistId(2), mockPlaylistId(99)],
      removeUnselected: false
    });

    expect(updated).toMatchObject({
      name: '車',
      syncAll: true,
      playlistIds: [mockPlaylistId(2)],
      removeUnselected: false
    });
  });

  it('差分を調べる（コピー済みの曲はコピーせず、対象から外れた曲は削除する）', async () => {
    const device = await connectedDevice();
    const plan = await commands.planDeviceSync(device.id);

    // プレイリストの5曲のうち2曲がコピー済みで、対象から外れた1曲がデバイスにある
    expect(plan).toMatchObject({
      copyCount: 3,
      deleteCount: 1,
      unchangedCount: 2,
      playlistCount: 1,
      hasEnoughSpace: true
    });
    expect(plan.requiredBytes).toBe(plan.copyBytes - plan.deleteBytes);

    // 削除しない設定では、対象から外れた曲を残す
    await commands.updateSyncDevice(device.id, { ...device, removeUnselected: false });
    expect(await commands.planDeviceSync(device.id)).toMatchObject({
      deleteCount: 0,
      unchangedCount: 3
    });
  });

  it('同期すると進捗を送り、次の差分はなくなる', async () => {
    const device = await connectedDevice();
    const result = await commands.runDeviceSync(device.id);

    expect(result).toMatchObject({
      copiedCount: 3,
      deletedCount: 1,
      playlistCount: 1,
      errorCount: 0,
      cancelled: false
    });
    const progress = events.filter((e) => e.event === 'device-sync-progress');
    expect(progress.map((e) => (e.payload as { current: number }).current)).toEqual([0, 1, 2, 3]);
    expect(progress[0].payload).toMatchObject({ deviceId: device.id, total: 3, bytesDone: 0 });

    expect(await commands.planDeviceSync(device.id)).toMatchObject({
      copyCount: 0,
      deleteCount: 0,
      unchangedCount: 5
    });
    const synced = await connectedDevice();
    expect(synced.lastSyncedAt).not.toBe(device.lastSyncedAt);
    expect(synced.freeBytes).toBeLessThan(device.freeBytes!);
  });

  it('全曲を同期する', async () => {
    const device = await connectedDevice();
    await commands.updateSyncDevice(device.id, { ...device, syncAll: true });

    const total = (await commands.getAllTracks()).length;
    // コピー済みの3曲はすべて対象に含まれる
    expect(await commands.planDeviceSync(device.id)).toMatchObject({
      copyCount: total - 3,
      deleteCount: 0
    });
  });

  it('同期を中止すると、次の曲の前で止まる', async () => {
    // 1曲目のコピー中に中止を要求する
    const backend: MockBackend = createMockBackend({
      emit: () => {},
      sleep: async () => {
        await backend.invoke('cancel_device_sync');
      }
    });
    mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));
    const device = await connectedDevice();

    const result = await commands.runDeviceSync(device.id);

    expect(result).toMatchObject({ copiedCount: 1, cancelled: true, playlistCount: 0 });
    expect((await connectedDevice()).lastSyncedAt).toBe(device.lastSyncedAt);
    // 続きから同期できる
    expect(await commands.planDeviceSync(device.id)).toMatchObject({ copyCount: 2 });
  });

  it('同期する対象がない・接続されていないデバイスは同期できない', async () => {
    const device = await connectedDevice();
    await commands.updateSyncDevice(device.id, { ...device, playlistIds: [] });
    await expect(commands.runDeviceSync(device.id)).rejects.toMatchObject({ code: 'VALIDATION' });

    const disconnected = (await commands.getSyncDevices()).find((d) => !d.connected)!;
    await expect(commands.planDeviceSync(disconnected.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
    await expect(commands.runDeviceSync(disconnected.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('転送先を変えると接続され、登録を解除すると一覧から消える', async () => {
    const disconnected = (await commands.getSyncDevices()).find((d) => !d.connected)!;

    const relinked = await commands.relinkSyncDevice(disconnected.id, '/Volumes/WALKMAN 1/MUSIC');
    expect(relinked).toMatchObject({ connected: true, path: '/Volumes/WALKMAN 1/MUSIC' });

    await commands.removeSyncDevice(disconnected.id);
    expect((await commands.getSyncDevices()).map((d) => d.id)).not.toContain(disconnected.id);
    await expect(commands.planDeviceSync(disconnected.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });
});
