import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { commands } from '#lib/bindings.js';
import type { SmartRules } from '#lib/types/models.js';
import { createMockBackend, toCommandName, type MockBackend } from './backend';
import { mockPlaylistId, mockTrackId } from './fixtures';

/** バックエンドから送信されたイベント */
let events: { event: string; payload: unknown }[];
let backend: MockBackend;

beforeEach(() => {
  // Node環境には`window`がないため、`__TAURI_INTERNALS__`の置き場所として空オブジェクトを用意する
  vi.stubGlobal('window', {});
  events = [];
  backend = createMockBackend({
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
    await expect(commands.setFavorite(['invalid'], true)).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.setFavorite([mockTrackId(999)], true)).rejects.toMatchObject({
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

  it('検索は語の途中に一致し、全角と半角・ひらがなとカタカナを同じとみなす', async () => {
    const titles = async (query: string) =>
      (await commands.searchTracks(query)).map((track) => track.title).sort();

    // 区切りのない日本語の途中・1文字
    expect(await titles('ドライブ')).toEqual(['ミッドナイト・ドライブ']);
    expect(await titles('港')).toEqual(['港の灯り']);
    // ひらがなで入力してカタカナの曲、全角の英字で入力して半角の曲
    expect(await titles('どらいぶ')).toEqual(['ミッドナイト・ドライブ']);
    expect(await titles('ＯＶＥＲ')).toEqual(['Overclock']);
    // 空白で区切った語を、すべて含む曲（項目が違ってもよい）
    expect(await titles('voltage rain')).toEqual(['Neon Rain']);
    expect(await titles('voltage どらいぶ')).toEqual([]);
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

describe('お気に入り', () => {
  const isFavorite = async (trackId: string) =>
    (await commands.getAllTracks()).find((track) => track.id === trackId)!.isFavorite;

  it('複数の曲をまとめてお気に入りにし、最近お気に入りにした曲を先に返す', async () => {
    const before = await commands.getFavoriteTracks();
    const targets = (await commands.getAllTracks())
      .filter((track) => !track.isFavorite)
      .slice(0, 2)
      .map((track) => track.id);

    await commands.setFavorite(targets, true);

    const favorites = await commands.getFavoriteTracks();
    expect(favorites).toHaveLength(before.length + 2);
    expect(
      favorites
        .slice(0, 2)
        .map((track) => track.id)
        .sort()
    ).toEqual([...targets].sort());
    expect(favorites.every((track) => track.isFavorite)).toBe(true);
  });

  it('お気に入りから外す', async () => {
    const [first, second] = await commands.getFavoriteTracks();

    await commands.setFavorite([first.id], false);

    expect(await isFavorite(first.id)).toBe(false);
    expect(await isFavorite(second.id)).toBe(true);
    expect((await commands.getFavoriteTracks()).some((track) => track.id === first.id)).toBe(false);
  });

  it('見つからない曲があれば、1曲も変えない', async () => {
    const target = (await commands.getAllTracks()).find((track) => !track.isFavorite)!.id;

    await expect(commands.setFavorite([target, mockTrackId(999)], true)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
    expect(await isFavorite(target)).toBe(false);
  });
});

describe('再生統計', () => {
  it('再生回数に数えると、再生履歴の先頭に加わる', async () => {
    const trackId = mockTrackId(20);
    const before = await commands.getPlayHistory();
    const track = (await commands.getAllTracks()).find((t) => t.id === trackId)!;

    expect(await commands.incrementPlayCount(trackId)).toBe(track.playCount + 1);

    const history = await commands.getPlayHistory();
    expect(history).toHaveLength(before.length + 1);
    expect(history[0].trackId).toBe(trackId);
    // 行を見分けるIDは、すべて違う
    expect(new Set(history.map((entry) => entry.id)).size).toBe(history.length);
  });

  it('再生履歴は新しい順で、同じ曲が何度も出る', async () => {
    const history = await commands.getPlayHistory();

    expect(history.length).toBeGreaterThan(10);
    const times = history.map((entry) => Date.parse(entry.playedAt));
    expect(times).toEqual([...times].sort((a, b) => b - a));
    expect(new Set(history.map((entry) => entry.trackId)).size).toBeLessThan(history.length);
  });

  it('スキップ回数を数える（再生回数・再生履歴は変えない）', async () => {
    const trackId = mockTrackId(2);
    const before = (await commands.getAllTracks()).find((t) => t.id === trackId)!;
    const historyLength = (await commands.getPlayHistory()).length;

    expect(await commands.incrementSkipCount(trackId)).toBe(before.skipCount + 1);

    const after = (await commands.getAllTracks()).find((t) => t.id === trackId)!;
    expect(after.skipCount).toBe(before.skipCount + 1);
    expect(after.playCount).toBe(before.playCount);
    expect(await commands.getPlayHistory()).toHaveLength(historyLength);
  });

  it('よく再生する曲は、再生回数の多い順に返す', async () => {
    const tracks = await commands.getMostPlayedTracks(5);

    expect(tracks).toHaveLength(5);
    const counts = tracks.map((track) => track.playCount);
    expect(counts).toEqual([...counts].sort((a, b) => b - a));
  });

  it('ライブラリから外した曲は、再生履歴からも消える', async () => {
    const [latest] = await commands.getPlayHistory();

    await commands.deleteTracksCommand([latest.trackId]);

    const history = await commands.getPlayHistory();
    expect(history.some((entry) => entry.trackId === latest.trackId)).toBe(false);
  });
});

describe('曲のタグ（編集画面）', () => {
  const trackId = mockTrackId(1);

  it('1曲の編集は、すべての項目を置き換える（データベースにない項目も読み直せる）', async () => {
    const before = await commands.getTrackTags(trackId);
    expect(before.title).toBeTruthy();
    expect(before.composer).toBeUndefined();

    await commands.updateTrackMetadata(trackId, {
      title: '新しいタイトル',
      albumArtist: 'Various Artists',
      trackNumber: 7,
      discNumber: 2,
      composer: '作曲者',
      bpm: 128,
      compilation: true,
      lyrics: '歌詞'
    });

    expect(await commands.getTrackTags(trackId)).toEqual({
      title: '新しいタイトル',
      albumArtist: 'Various Artists',
      trackNumber: 7,
      discNumber: 2,
      composer: '作曲者',
      bpm: 128,
      compilation: true,
      lyrics: '歌詞'
    });
    const track = (await commands.getAllTracks()).find((t) => t.id === trackId)!;
    expect(track).toMatchObject({
      title: '新しいタイトル',
      artist: null,
      albumArtist: 'Various Artists',
      trackNumber: 7,
      discNumber: 2
    });
  });

  it('一括編集は、値のある項目だけを変える', async () => {
    const ids = [mockTrackId(1), mockTrackId(2)];
    const titles = (await commands.getAllTracks())
      .filter((track) => ids.includes(track.id))
      .map((track) => track.title);
    await commands.updateTrackMetadata(ids[0], { title: titles[0] ?? 'x', composer: '元の作曲者' });

    await commands.updateMultipleTracksMetadata(ids, { grouping: 'グループ', discNumber: 3 });

    const first = await commands.getTrackTags(ids[0]);
    expect(first).toMatchObject({ composer: '元の作曲者', grouping: 'グループ', discNumber: 3 });
    expect((await commands.getTrackTags(ids[1])).grouping).toBe('グループ');

    // コンピレーションの印は、付けて外せる
    await commands.updateMultipleTracksMetadata(ids, { compilation: true });
    expect((await commands.getTrackTags(ids[1])).compilation).toBe(true);
    await commands.updateMultipleTracksMetadata(ids, { compilation: false });
    expect((await commands.getTrackTags(ids[1])).compilation).toBeUndefined();
  });

  it('範囲の外の値・長すぎる値はエラーにする（文字数で数える）', async () => {
    await expect(commands.updateTrackMetadata(trackId, { bpm: 1000 })).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.updateTrackMetadata(trackId, { discTotal: 0 })).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(
      commands.updateTrackMetadata(trackId, { title: 'あ'.repeat(255) })
    ).resolves.toBeNull();
    await expect(
      commands.updateTrackMetadata(trackId, { title: 'あ'.repeat(256) })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
  });

  it('ファイルが見つからない曲は、タグを読めない', async () => {
    const missing = (await commands.getAllTracks()).find((track) => track.isMissing)!;

    await expect(commands.getTrackTags(missing.id)).rejects.toMatchObject({ code: 'METADATA' });
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
    expect(await commands.removeTracksFromPlaylist(playlist.id, [mockTrackId(1)])).toBe(1);

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

  it('自動プレイリストは、条件に合う曲を返し、曲を自分で変える操作を受け付けない', async () => {
    const smart = (await commands.getPlaylists()).find((p) => p.id === mockPlaylistId(4))!;
    expect(smart.rules).not.toBeNull();
    // 一覧には曲を持たせない（開く時に条件から求める）
    expect(smart.tracks).toEqual([]);

    const tracks = await commands.getPlaylistTracks(smart.id);
    expect(tracks.length).toBeGreaterThan(0);
    expect(tracks.every((track) => track.rating >= 4)).toBe(true);
    // 評価の高い順
    expect(tracks.map((track) => track.rating)).toEqual(
      tracks.map((track) => track.rating).sort((a, b) => b - a)
    );

    // 評価を変えると、次に開いた時の曲が変わる
    await commands.setRating(tracks[0].id, 1);
    const after = await commands.getPlaylistTracks(smart.id);
    expect(after.map((track) => track.id)).not.toContain(tracks[0].id);

    const rejected = { code: 'VALIDATION' };
    await expect(commands.addTracksToPlaylist(smart.id, [mockTrackId(4)])).rejects.toMatchObject(
      rejected
    );
    await expect(commands.removeTracksFromPlaylist(smart.id, [after[0].id])).rejects.toMatchObject(
      rejected
    );
    await expect(commands.reorderPlaylistTracks(smart.id, [after[0].id])).rejects.toMatchObject(
      rejected
    );
  });

  it('自動プレイリストを作り、条件を変えられる', async () => {
    const rules: SmartRules = {
      matchMode: 'all',
      rules: [{ kind: 'text', field: 'genre', op: 'is', value: 'jazz' }],
      order: { field: 'title', descending: false },
      limit: null
    };
    expect(await commands.countSmartPlaylistTracks(rules)).toBe(3);

    const created = await commands.createSmartPlaylist(' ジャズ ', rules);
    expect(created).toMatchObject({ name: 'ジャズ', rules, tracks: [] });
    const jazz = await commands.getPlaylistTracks(created.id);
    expect(jazz.map((track) => track.genre)).toEqual(['Jazz', 'Jazz', 'Jazz']);

    await commands.updateSmartPlaylist(created.id, { ...rules, limit: 2 });
    expect(await commands.getPlaylistTracks(created.id)).toHaveLength(2);
    expect(await commands.countSmartPlaylistTracks({ ...rules, limit: 2 })).toBe(2);

    // 誤りのある条件・通常のプレイリストには設定できない
    await expect(
      commands.createSmartPlaylist('誤り', { ...rules, limit: 0 })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
    await expect(commands.updateSmartPlaylist(mockPlaylistId(1), rules)).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.updateSmartPlaylist(mockPlaylistId(99), rules)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('複数の曲を、プレイリストからまとめて外す', async () => {
    const id = mockPlaylistId(1);
    // 入っていない曲は飛ばす
    const removed = await commands.removeTracksFromPlaylist(id, [7, 1, 2].map(mockTrackId));
    expect(removed).toBe(2);

    const saved = (await commands.getPlaylists()).find((p) => p.id === id)!;
    expect(saved.tracks.map((entry) => [entry.trackId, entry.position])).toEqual([
      [mockTrackId(8), 0],
      [mockTrackId(10), 1],
      [mockTrackId(12), 2]
    ]);

    await expect(commands.removeTracksFromPlaylist(id, [])).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(
      commands.removeTracksFromPlaylist(mockPlaylistId(99), [mockTrackId(1)])
    ).rejects.toMatchObject({ code: 'NOT_FOUND' });
  });

  it('渡した曲でプレイリストを作る（同じ曲は最初の1回だけ）', async () => {
    const ids = [3, 1, 3, 2].map(mockTrackId);
    const created = await commands.createPlaylistWithTracks(' キュー ', ids);

    expect(created.name).toBe('キュー');
    expect(created.tracks.map((entry) => entry.trackId)).toEqual([3, 1, 2].map(mockTrackId));
    expect((await commands.getPlaylistTracks(created.id)).map((track) => track.id)).toEqual(
      [3, 1, 2].map(mockTrackId)
    );

    const before = (await commands.getPlaylists()).length;
    await expect(
      commands.createPlaylistWithTracks('だめ', [mockTrackId(1), mockTrackId(999)])
    ).rejects.toMatchObject({ code: 'NOT_FOUND' });
    expect(await commands.getPlaylists()).toHaveLength(before);
  });

  it('プレイリストの説明を変えられる', async () => {
    const id = mockPlaylistId(1);
    const description = async () =>
      (await commands.getPlaylists()).find((p) => p.id === id)!.description;

    await commands.setPlaylistDescription(id, '  休日のドライブ用  ');
    expect(await description()).toBe('休日のドライブ用');
    await commands.setPlaylistDescription(id, '   ');
    expect(await description()).toBeNull();
    await commands.setPlaylistDescription(id, '説明');
    await commands.setPlaylistDescription(id, null);
    expect(await description()).toBeNull();

    await expect(commands.setPlaylistDescription(id, 'あ'.repeat(1001))).rejects.toMatchObject({
      code: 'VALIDATION'
    });
  });

  it('フォルダを作り、プレイリストを出し入れできる。フォルダを消しても、プレイリストは残る', async () => {
    const folder = await commands.createPlaylistFolder(' 外出 ');
    expect(folder.name).toBe('外出');
    const folderOf = async (id: string) =>
      (await commands.getPlaylists()).find((p) => p.id === id)!.folderId;

    await commands.movePlaylist(mockPlaylistId(1), folder.id);
    await commands.movePlaylist(mockPlaylistId(2), folder.id);
    expect(await folderOf(mockPlaylistId(1))).toBe(folder.id);

    await commands.renamePlaylistFolder(folder.id, 'おでかけ');
    await commands.movePlaylist(mockPlaylistId(2), null);
    expect(await folderOf(mockPlaylistId(2))).toBeNull();
    expect((await commands.getPlaylistFolders()).map((f) => f.name)).toContain('おでかけ');

    await expect(commands.movePlaylist(mockPlaylistId(99), folder.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
    await expect(commands.createPlaylistFolder('  ')).rejects.toMatchObject({
      code: 'VALIDATION'
    });

    const playlistCount = (await commands.getPlaylists()).length;
    await commands.deletePlaylistFolder(folder.id);
    expect((await commands.getPlaylistFolders()).map((f) => f.id)).not.toContain(folder.id);
    expect(await commands.getPlaylists()).toHaveLength(playlistCount);
    expect(await folderOf(mockPlaylistId(1))).toBeNull();
    await expect(commands.deletePlaylistFolder(folder.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('プレイリストとフォルダを、手動で並べ替えられる', async () => {
    const byPosition = async () =>
      (await commands.getPlaylists()).sort((a, b) => a.position - b.position).map((p) => p.id);
    expect(await byPosition()).toEqual([1, 2, 3, 4].map(mockPlaylistId));

    await commands.reorderPlaylists([3, 1, 4, 2].map(mockPlaylistId));
    expect(await byPosition()).toEqual([3, 1, 4, 2].map(mockPlaylistId));

    // 新しいプレイリストと、フォルダへ移したプレイリストは、いちばん後ろになる
    const created = await commands.createPlaylist('新しい');
    const [existing] = await commands.getPlaylistFolders();
    await commands.movePlaylist(mockPlaylistId(3), existing.id);
    expect((await byPosition()).slice(-2)).toEqual([created.id, mockPlaylistId(3)]);

    const second = await commands.createPlaylistFolder('あとから');
    await commands.reorderPlaylistFolders([second.id, existing.id]);
    expect((await commands.getPlaylistFolders()).map((f) => f.id)).toEqual([
      second.id,
      existing.id
    ]);
  });

  it('ランダムな並びの自動プレイリストは、選び直すまで同じ並びになる', async () => {
    const created = await commands.createSmartPlaylist('ランダム', {
      matchMode: 'all',
      rules: [],
      order: { field: 'random', descending: false },
      limit: null
    });
    const order = async () => (await commands.getPlaylistTracks(created.id)).map((t) => t.id);

    const first = await order();
    expect(await order()).toEqual(first);
    await commands.reshuffleSmartPlaylists();
    const reshuffled = await order();
    expect(reshuffled).not.toEqual(first);
    expect([...reshuffled].sort()).toEqual([...first].sort());
  });
});

describe('並び順に使う値（ソート用のタグ）', () => {
  it('曲・アルバム・アーティストが、読みを持つ', async () => {
    const tracks = await commands.getAllTracks();
    const track = tracks.find((t) => t.title === '港の灯り')!;
    expect(track.sortTags).toEqual({
      title: 'みなとのあかり',
      artist: 'ねおんどおり',
      album: 'よあけのしぐなる',
      albumArtist: null
    });

    const albums = await commands.getAlbums();
    expect(albums.find((album) => album.name === '夜明けのシグナル')?.sortName).toBe(
      'よあけのしぐなる'
    );
    expect(albums.find((album) => album.name === 'Blue Horizon')?.sortName).toBeNull();
    const artists = await commands.getArtists();
    expect(artists.find((artist) => artist.name === 'ネオン通り')?.sortName).toBe('ねおんどおり');
    const [group] = await commands.getArtistAlbums('ネオン通り');
    expect(group.sortName).toBe('よあけのしぐなる');
  });

  it('1曲の編集で読みを置き換え、ファイルのタグとして読める', async () => {
    const track = (await commands.getAllTracks()).find((t) => t.title === '港の灯り')!;
    const tags = await commands.getTrackTags(track.id);
    expect(tags).toMatchObject({ titleSort: 'みなとのあかり', artistSort: 'ねおんどおり' });
    expect(tags.albumArtistSort).toBeUndefined();

    await commands.updateTrackMetadata(track.id, {
      ...tags,
      artistSort: 'ネオンドオリ',
      albumSort: null,
      albumArtistSort: 'ねおん'
    });

    const updated = (await commands.getAllTracks()).find((t) => t.id === track.id)!;
    expect(updated.sortTags).toEqual({
      title: 'みなとのあかり',
      artist: 'ネオンドオリ',
      album: null,
      albumArtist: 'ねおん'
    });
    expect(await commands.getTrackTags(track.id)).toMatchObject({
      artistSort: 'ネオンドオリ',
      albumArtistSort: 'ねおん'
    });
  });

  it('一括編集は、入力した読みだけを変える', async () => {
    const tracks = (await commands.getAllTracks()).filter((t) => t.artist === 'ネオン通り');
    const ids = tracks.map((t) => t.id);

    await commands.updateMultipleTracksMetadata(ids, { artistSort: 'ネオンどおり' });

    const updated = (await commands.getAllTracks()).filter((t) => ids.includes(t.id));
    expect(updated.every((t) => t.sortTags.artist === 'ネオンどおり')).toBe(true);
    // ほかの読みは変えない
    expect(updated.find((t) => t.title === '港の灯り')?.sortTags.title).toBe('みなとのあかり');
    await expect(
      commands.updateMultipleTracksMetadata(ids, { artistSort: 'あ'.repeat(256) })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});

describe('曲ごとのメタデータの変更（タグの一括ツール）', () => {
  it('曲ごとに違う値を書き込み、値のある項目だけを変える', async () => {
    const tracks = (await commands.getAllTracks()).filter((t) => t.album === 'Blue Horizon');
    const [first, second] = tracks;

    const result = await commands.applyMetadataChanges([
      { trackId: first.id, metadata: { title: '新しいタイトル', trackNumber: 11, trackTotal: 12 } },
      { trackId: second.id, metadata: { artist: 'AOI SORA' } }
    ]);

    expect(result).toEqual({ updatedCount: 2, failedCount: 0, errors: [] });
    const updated = await commands.getAllTracks();
    const updatedFirst = updated.find((t) => t.id === first.id)!;
    expect(updatedFirst).toMatchObject({
      title: '新しいタイトル',
      trackNumber: 11,
      artist: first.artist,
      album: first.album
    });
    expect(updatedFirst.updatedAt).not.toBe(first.updatedAt);
    expect(updated.find((t) => t.id === second.id)).toMatchObject({
      title: second.title,
      artist: 'AOI SORA'
    });
    // ライブラリに持たない項目（総数）は、ファイルのタグとして読める
    expect((await commands.getTrackTags(first.id)).trackTotal).toBe(12);
  });

  it('文字列の項目に空の値を渡すと、その項目を空にする', async () => {
    const track = (await commands.getAllTracks()).find((t) => t.genre === 'Jazz')!;

    await commands.applyMetadataChanges([{ trackId: track.id, metadata: { genre: '' } }]);

    const updated = (await commands.getAllTracks()).find((t) => t.id === track.id)!;
    expect(updated.genre).toBeNull();
    expect(updated.title).toBe(track.title);
    expect((await commands.getTrackTags(track.id)).genre).toBeUndefined();
  });

  it('ファイルが見つからない曲は、書き込めなかった曲として返し、残りを続ける', async () => {
    const tracks = await commands.getAllTracks();
    const missing = tracks.find((t) => t.isMissing)!;
    const present = tracks.find((t) => !t.isMissing)!;

    const result = await commands.applyMetadataChanges([
      { trackId: missing.id, metadata: { title: 'x' } },
      { trackId: mockTrackId(9999), metadata: { title: 'y' } },
      { trackId: present.id, metadata: { title: 'z' } }
    ]);

    expect(result).toMatchObject({ updatedCount: 1, failedCount: 2 });
    expect(result.errors[0]).toContain(missing.filePath);
    const after = await commands.getAllTracks();
    expect(after.find((t) => t.id === missing.id)?.title).toBe(missing.title);
    expect(after.find((t) => t.id === present.id)?.title).toBe('z');
  });

  it('不正な入力が1件でもあれば、何も書き込まない', async () => {
    const [track] = await commands.getAllTracks();

    await expect(commands.applyMetadataChanges([])).rejects.toMatchObject({ code: 'VALIDATION' });
    await expect(
      commands.applyMetadataChanges([
        { trackId: track.id, metadata: { title: '書き込まれない' } },
        { trackId: track.id, metadata: { trackNumber: 1000 } }
      ])
    ).rejects.toMatchObject({ code: 'VALIDATION' });

    expect((await commands.getAllTracks())[0].title).toBe(track.title);
  });
});

describe('アルバムアート', () => {
  /** アルバムの曲（ファイルが見つからない曲を除く） */
  async function albumTracks(album: string) {
    const tracks = await commands.getAllTracks();
    return tracks.filter((track) => track.album === album && !track.isMissing);
  }

  it('埋め込みの画像・フォルダの画像・アートなしを、情報とURLで返す', async () => {
    const [embedded] = await albumTracks('Blue Horizon');
    const [folder] = await albumTracks('Midnight Circuit');
    const [none] = await albumTracks('Quiet Rooms');

    expect(await commands.getAlbumArtInfo(embedded.id)).toMatchObject({
      source: 'embedded',
      fileName: null,
      mimeType: 'image/jpeg'
    });
    expect(await commands.getAlbumArtInfo(folder.id)).toMatchObject({
      source: 'folder',
      fileName: 'cover.jpg'
    });
    expect(await commands.getAlbumArtInfo(none.id)).toBeNull();
    expect(backend.albumArtUrl(embedded.id)).toMatch(/^data:image/);
    expect(backend.albumArtUrl(folder.id)).toMatch(/^data:image/);
    expect(backend.albumArtUrl(none.id)).toBeNull();
    await expect(commands.getAlbumArtInfo(mockTrackId(9999))).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('画像を埋め込むと、埋め込みの画像が優先され、ファイルのサイズが増える', async () => {
    const tracks = await albumTracks('Midnight Circuit');
    const ids = tracks.map((track) => track.id);
    const urlBefore = backend.albumArtUrl(ids[0]);

    const result = await commands.setAlbumArt(ids);

    expect(result).toEqual({ updatedCount: ids.length, failedCount: 0, errors: [] });
    const info = await commands.getAlbumArtInfo(ids[0]);
    expect(info).toMatchObject({ source: 'embedded', fileName: null, mimeType: 'image/png' });
    expect(backend.albumArtUrl(ids[0])).not.toBe(urlBefore);
    // 同じ画像を選んだ曲は、同じ画像になる
    expect(backend.albumArtUrl(ids[1])).toBe(backend.albumArtUrl(ids[0]));
    const after = await albumTracks('Midnight Circuit');
    expect(after[0].fileSize).toBe(tracks[0].fileSize + info!.size);

    // 選び直すと、別の画像に置き換わる
    const urlFirst = backend.albumArtUrl(ids[0]);
    await commands.setAlbumArt([ids[0]]);
    expect(backend.albumArtUrl(ids[0])).not.toBe(urlFirst);
    expect(backend.albumArtUrl(ids[1])).toBe(urlFirst);
  });

  it('埋め込みの画像を取り除くと、フォルダの画像に戻る（なければアートなし）', async () => {
    const [folder] = await albumTracks('Midnight Circuit');
    const [embedded] = await albumTracks('Blue Horizon');
    const folderUrl = backend.albumArtUrl(folder.id);
    await commands.setAlbumArt([folder.id]);

    const result = await commands.removeAlbumArt([folder.id, embedded.id]);

    expect(result).toEqual({ updatedCount: 2, failedCount: 0, errors: [] });
    expect(backend.albumArtUrl(folder.id)).toBe(folderUrl);
    expect(await commands.getAlbumArtInfo(embedded.id)).toBeNull();
    expect(backend.albumArtUrl(embedded.id)).toBeNull();
    const [after] = await albumTracks('Midnight Circuit');
    expect(after.fileSize).toBe(folder.fileSize);

    // 埋め込みの画像がない曲は書き換えず、件数にも数えない
    expect(await commands.removeAlbumArt([folder.id, embedded.id])).toEqual({
      updatedCount: 0,
      failedCount: 0,
      errors: []
    });
  });

  it('ファイルが見つからない曲は、書き込めなかった曲として返す', async () => {
    const tracks = await commands.getAllTracks();
    const missing = tracks.find((track) => track.isMissing)!;
    const [present] = await albumTracks('Blue Horizon');

    const result = await commands.setAlbumArt([missing.id, present.id]);

    expect(result).toMatchObject({ updatedCount: 1, failedCount: 1 });
    expect(result!.errors[0]).toContain(missing.filePath);
  });

  it('選んだことにする画像を切り替えられる', async () => {
    const [track] = await albumTracks('Blue Horizon');
    const url = backend.albumArtUrl(track.id);

    backend.setAlbumArtPickMode('cancel');
    expect(await commands.setAlbumArt([track.id])).toBeNull();
    backend.setAlbumArtPickMode('tooLarge');
    await expect(commands.setAlbumArt([track.id])).rejects.toMatchObject({ code: 'VALIDATION' });

    expect(backend.albumArtUrl(track.id)).toBe(url);
    await expect(commands.setAlbumArt([])).rejects.toMatchObject({ code: 'VALIDATION' });
    await expect(commands.removeAlbumArt([])).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});

describe('M3Uの読み込み・書き出し', () => {
  it('読み込むとプレイリストを作り、対応が付かなかった行を返す', async () => {
    const before = await commands.getPlaylists();

    const [result] = await commands.importM3uPlaylists();

    const playlists = await commands.getPlaylists();
    expect(playlists).toHaveLength(before.length + 1);
    const created = playlists.find((playlist) => playlist.id === result.playlistId)!;
    expect(created.name).toBe(result.playlistName);
    expect(created.tracks).toHaveLength(result.addedCount);
    expect(result.unmatchedCount).toBe(result.unmatched.length);
    expect(result.unmatchedCount).toBeGreaterThan(0);
    expect(result.error).toBeNull();
  });

  it('同じ名前のプレイリストがあれば、番号を付けた名前で作る', async () => {
    const [first] = await commands.importM3uPlaylists();
    const [second] = await commands.importM3uPlaylists();

    expect(second.playlistName).toBe(`${first.playlistName} (2)`);
    expect(second.playlistId).not.toBe(first.playlistId);
  });

  it('選んだことにするファイルを切り替えられる', async () => {
    backend.setM3uImportMode('clean');
    const [clean] = await commands.importM3uPlaylists();
    expect(clean).toMatchObject({ unmatchedCount: 0, duplicateCount: 0, unmatched: [] });

    backend.setM3uImportMode('cancel');
    expect(await commands.importM3uPlaylists()).toEqual([]);
  });

  it('書き出すと、プレイリスト名のファイル名と曲数を返す', async () => {
    const result = await commands.exportPlaylistM3u(mockPlaylistId(1), true);

    expect(result).toEqual({ fileName: 'ドライブ用.m3u8', trackCount: 5 });
    await expect(commands.exportPlaylistM3u(mockPlaylistId(99), false)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });
});

describe('ライブラリのXMLの取り込み', () => {
  it('再生回数を多い方にし、プレイリストを作って、ライブラリの変更を通知する', async () => {
    const before = await commands.getAllTracks();
    const playlistsBefore = await commands.getPlaylists();

    const result = (await commands.importLibraryXml(true))!;

    expect(result.matchedCount).toBeGreaterThan(0);
    expect(result.updatedCount).toBeGreaterThan(0);
    expect(result.unmatchedCount).toBe(result.unmatched.length);
    expect(result.playlistCount).toBe(1);
    const after = await commands.getAllTracks();
    for (const track of after) {
      const previous = before.find((candidate) => candidate.id === track.id)!;
      expect(track.playCount).toBeGreaterThanOrEqual(previous.playCount);
      expect(track.createdAt <= previous.createdAt).toBe(true);
    }
    expect(await commands.getPlaylists()).toHaveLength(playlistsBefore.length + 1);
    expect(events.some((event) => event.event === 'library-changed')).toBe(true);
  });

  it('もう一度取り込んでも値は変わらず、プレイリストを取り込まない指定もできる', async () => {
    await commands.importLibraryXml(false);
    const playlists = await commands.getPlaylists();
    const tracks = await commands.getAllTracks();

    const again = (await commands.importLibraryXml(false))!;

    expect(again.updatedCount).toBe(0);
    expect(again.playlistCount).toBe(0);
    expect(await commands.getAllTracks()).toEqual(tracks);
    expect(await commands.getPlaylists()).toEqual(playlists);
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

describe('見つからない曲', () => {
  it('ライブラリに残し、一覧・アルバムの曲数にも含める', async () => {
    const tracks = await commands.getAllTracks();
    const missing = tracks.filter((track) => track.isMissing);

    expect(missing.map((track) => track.title)).toEqual(['Lost Tape']);
    expect((await commands.getLibraryFolders()).missingTrackCount).toBe(1);
    const sketches = (await commands.getAlbums()).find(
      (album) => album.name === 'Sketches' && album.artist === 'Kenji Mori'
    );
    expect(sketches?.trackCount).toBe(3);
  });

  it('まとめて外すと、外した数を返してライブラリの変更を通知する', async () => {
    const before = (await commands.getAllTracks()).length;

    expect(await commands.removeMissingTracks()).toBe(1);

    const tracks = await commands.getAllTracks();
    expect(tracks).toHaveLength(before - 1);
    expect(tracks.some((track) => track.isMissing)).toBe(false);
    expect((await commands.getLibraryFolders()).missingTrackCount).toBe(0);
    expect(events).toContainEqual({ event: 'library-changed', payload: null });
    // 外す曲がなければ、何もしない
    events.length = 0;
    expect(await commands.removeMissingTracks()).toBe(0);
    expect(events).toEqual([]);
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

describe('再生エンジン', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('再生すると曲の長さを返し、再生位置と曲の終わりを通知する', async () => {
    vi.useFakeTimers();
    const track = (await commands.getAllTracks()).find((t) => !t.isMissing && t.duration)!;

    await expect(commands.playbackPlay(track.id, 1)).resolves.toEqual({
      duration: track.duration
    });
    await commands.playbackSeek(track.duration! - 1);
    vi.advanceTimersByTime(1500);

    const payloads = events.filter((e) => e.event === 'playback-event').map((e) => e.payload);
    expect(payloads[0]).toEqual({ type: 'position', token: 1, position: track.duration! - 1 });
    expect(payloads.at(-1)).toEqual({ type: 'ended', token: 1 });
  });

  it('見つからない曲・不正な値はエラーになる', async () => {
    const missing = (await commands.getAllTracks()).find((t) => t.isMissing)!;

    await expect(commands.playbackPlay(missing.id, 1)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
    await expect(commands.playbackPlay('not-a-uuid', 2)).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.playbackSeek(-1)).rejects.toMatchObject({ code: 'VALIDATION' });
    await expect(commands.playbackSetVolume(Number.NaN)).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    // イコライザのゲインは、バンドの数（10個）で渡す
    await expect(commands.playbackSetEqualizer(true, [0, 0, 0])).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(
      commands.playbackSetEqualizer(true, [0, 0, 0, 0, 0, 6, 0, 0, 0, 0])
    ).resolves.toBeNull();
  });

  it('出力デバイスの一覧を返し、既定のデバイスは1つだけ', async () => {
    const devices = await commands.getOutputDevices();

    expect(devices.length).toBeGreaterThan(1);
    expect(devices.filter((device) => device.isDefault)).toHaveLength(1);
  });

  it('出力デバイスの設定を保存する', async () => {
    const settings = await commands.getSettings();
    expect(settings.outputDeviceId).toBeNull();

    const [, device] = await commands.getOutputDevices();
    await commands.saveSettings({ ...settings, outputDeviceId: device.id });
    expect((await commands.getSettings()).outputDeviceId).toBe(device.id);

    await expect(commands.saveSettings({ ...settings, outputDeviceId: '' })).rejects.toMatchObject({
      code: 'VALIDATION'
    });
  });
});

describe('OSのNow Playing', () => {
  it('伝えた内容を保持し、nullで消す', async () => {
    const update = { trackId: mockTrackId(1), playing: true, position: 12, duration: 200 };
    await commands.setNowPlaying(update);
    expect(backend.nowPlaying()).toEqual(update);

    await commands.setNowPlaying(null);
    expect(backend.nowPlaying()).toBeNull();
  });

  it('ライブラリにない曲はエラーにし、前の曲の情報を残さない', async () => {
    await commands.setNowPlaying({
      trackId: mockTrackId(1),
      playing: true,
      position: 0,
      duration: null
    });

    await expect(
      commands.setNowPlaying({
        trackId: '00000000-0000-4000-8000-00000000ffff',
        playing: true,
        position: 0,
        duration: null
      })
    ).rejects.toMatchObject({ code: 'NOT_FOUND' });
    expect(backend.nowPlaying()).toBeNull();

    await expect(
      commands.setNowPlaying({ trackId: 'x', playing: true, position: 0, duration: null })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});

describe('再生状態の保存と復元', () => {
  const cursor = { volume: 0.4, shuffle: false, repeat: 'all' as const, currentIndex: 1 };

  it('保存していなければ、何も再生していない状態を返す', async () => {
    expect(await commands.getPlaybackState()).toEqual({
      volume: 1,
      shuffle: false,
      repeat: 'off',
      queue: [],
      originalTrackIds: null,
      currentIndex: null
    });
  });

  it('保存したキューを曲の情報にして返し、キューを渡さない保存ではキューを保つ', async () => {
    const trackIds = [mockTrackId(3), mockTrackId(1), mockTrackId(2)];
    await commands.savePlaybackState(cursor, { trackIds, originalTrackIds: null });
    await commands.savePlaybackState({ ...cursor, currentIndex: 2, volume: 7 }, null);

    const state = await commands.getPlaybackState();

    expect(state.queue.map((track) => track.id)).toEqual(trackIds);
    expect(state.currentIndex).toBe(2);
    // 範囲外の音量は丸める
    expect(state.volume).toBe(1);
    expect(state.repeat).toBe('all');
  });

  it('ライブラリからなくなった曲・見つからない曲を除き、再生していた曲の位置を合わせる', async () => {
    const missing = (await commands.getAllTracks()).find((t) => t.isMissing)!;
    const trackIds = [mockTrackId(1), missing.id, mockTrackId(2), mockTrackId(3)];
    await commands.savePlaybackState(
      { ...cursor, shuffle: true, currentIndex: 2 },
      { trackIds, originalTrackIds: [...trackIds].reverse() }
    );
    await commands.deleteTracksCommand([mockTrackId(1)]);

    const state = await commands.getPlaybackState();

    expect(state.queue.map((track) => track.id)).toEqual([mockTrackId(2), mockTrackId(3)]);
    expect(state.originalTrackIds).toEqual([mockTrackId(3), mockTrackId(2)]);
    expect(state.currentIndex).toBe(0);
  });

  it('保存先を渡すと、作り直した後も復元できる', async () => {
    const items = new Map<string, string>();
    const storage = {
      getItem: (key: string) => items.get(key) ?? null,
      setItem: (key: string, value: string) => void items.set(key, value)
    };
    const first = createMockBackend({ emit: () => {}, storage });
    await first.invoke('save_playback_state', {
      cursor,
      queue: { trackIds: [mockTrackId(1), mockTrackId(2)], originalTrackIds: null }
    });

    const second = createMockBackend({ emit: () => {}, storage });
    const state = (await second.invoke('get_playback_state')) as Awaited<
      ReturnType<typeof commands.getPlaybackState>
    >;

    expect(state.queue.map((track) => track.id)).toEqual([mockTrackId(1), mockTrackId(2)]);
    expect(state.currentIndex).toBe(1);
    expect(state.volume).toBe(0.4);
  });

  it('トラックIDの形式でない値は保存しない', async () => {
    await expect(
      commands.savePlaybackState(cursor, { trackIds: ['x'], originalTrackIds: null })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});
