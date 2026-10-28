import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { commands, events as bindingEvents } from '#lib/bindings.js';
import type { ConvertOptions, SmartRules } from '#lib/types/models.js';
import {
  createMockBackend,
  MOCK_EVENT_NAMES,
  toCommandName,
  type MockBackend,
  type MockBackendOptions,
  type MockEventName
} from './backend';
import { mockPlaylistId, mockTrackId } from './fixtures';

/** バックエンドから送信されたイベント */
let events: { event: string; payload: unknown }[];
let backend: MockBackend;

beforeEach(() => {
  // Node環境には`window`がないため、`__TAURI_INTERNALS__`の置き場所として空オブジェクトを用意する
  vi.stubGlobal('window', {});
  events = [];
  backend = createMockBackend({
    emit: (event, payload) => events.push({ event: MOCK_EVENT_NAMES[event], payload }),
    sleep: async () => {}
  });
  mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

/** フォルダを選ぶダイアログ（Rust側が開く）でフォルダを選んだことにして、インポートする */
function importFolder(path: string) {
  backend.setFolderResult(path);
  return commands.importFolder('Skip');
}

describe('形式の変換', () => {
  const options = (overrides: Partial<ConvertOptions> = {}): ConvertOptions => ({
    format: 'flac',
    aacBitrateKbps: 256,
    mode: 'export',
    template: '%albumartist%/%album%/%track% %title%',
    addToLibrary: false,
    ...overrides
  });
  const byTitle = async (title: string) =>
    (await commands.getAllTracks()).find((track) => track.title === title)!;

  it('変換先にできる形式を返す', async () => {
    expect(await commands.getConvertFormats()).toEqual(['aac', 'alac', 'flac', 'wav', 'aiff']);
  });

  it('書き出しの計画は、書式に従った場所と、元の曲が可逆かどうかを返す', async () => {
    const lossy = await byTitle('Paper Planes');
    const lossless = await byTitle('Morning Walk');
    const missing = await byTitle('Lost Tape');

    const plan = await commands.planConversion([lossy.id, lossless.id, missing.id], options());

    expect(plan.entries).toEqual([
      {
        trackId: lossy.id,
        sourceName: '02 Paper Planes.mp3',
        outputPath: 'Aoi Sora/Blue Horizon/02 Paper Planes.flac',
        status: 'convert',
        sourceLossless: false
      },
      {
        trackId: lossless.id,
        sourceName: '01 Morning Walk.flac',
        outputPath: 'Aoi Sora/Field Notes/01 Morning Walk.flac',
        status: 'convert',
        sourceLossless: true
      },
      {
        trackId: missing.id,
        sourceName: missing.fileName,
        outputPath: null,
        status: 'missing',
        sourceLossless: false
      }
    ]);
  });

  it('書き出しは、ライブラリを変えない（加える指定の時だけ、曲を足す）', async () => {
    const track = await byTitle('Paper Planes');
    const count = (await commands.getAllTracks()).length;

    const result = await commands.convertTracks([track.id], options());

    expect(result).toEqual({
      convertedCount: 1,
      skippedCount: 0,
      failedCount: 0,
      errors: [],
      cancelled: false
    });
    expect(await commands.getAllTracks()).toHaveLength(count);
    expect(events.filter((e) => e.event === 'convert-progress')).toEqual([
      {
        event: 'convert-progress',
        payload: { current: 1, total: 1, currentFile: '02 Paper Planes.mp3' }
      }
    ]);
    expect(events.map((e) => e.event)).not.toContain('library-changed');

    // 同じ場所へもう一度書き出すと、同じ名前のファイルがあるため、変換しない
    expect(await commands.convertTracks([track.id], options())).toMatchObject({
      convertedCount: 0,
      skippedCount: 1
    });

    // ライブラリに加える（別の形式なら、同じ名前にならない）
    const added = await commands.convertTracks(
      [track.id],
      options({ format: 'aac', aacBitrateKbps: 192, addToLibrary: true })
    );
    expect(added?.convertedCount).toBe(1);
    const tracks = await commands.getAllTracks();
    expect(tracks).toHaveLength(count + 1);
    expect(tracks.find((t) => t.filePath.startsWith('/Users/mock/Export/'))).toMatchObject({
      filePath: '/Users/mock/Export/Aoi Sora/Blue Horizon/02 Paper Planes.m4a',
      format: 'M4A',
      bitrate: 192,
      title: 'Paper Planes'
    });
    expect(events.map((e) => e.event)).toContain('library-changed');
  });

  it('書き出すフォルダを選ばなかった場合は、nullを返す', async () => {
    const track = await byTitle('Paper Planes');
    backend.setConvertExportFolder(null);

    expect(await commands.convertTracks([track.id], options())).toBeNull();
    expect(events).toEqual([]);
  });

  it('置き換えは、曲のIDを保ったまま、場所と形式を変える', async () => {
    const track = await byTitle('Paper Planes');
    await commands.setFavorite([track.id], true);
    events = [];

    const plan = await commands.planConversion([track.id], options({ mode: 'replace' }));
    expect(plan.entries[0]).toMatchObject({
      outputPath: '02 Paper Planes.flac',
      status: 'convert'
    });

    const result = await commands.convertTracks([track.id], options({ mode: 'replace' }));

    expect(result?.convertedCount).toBe(1);
    const replaced = (await commands.getAllTracks()).find((t) => t.id === track.id)!;
    expect(replaced).toMatchObject({
      filePath: track.filePath.replace(/\.mp3$/, '.flac'),
      fileName: '02 Paper Planes.flac',
      format: 'FLAC',
      title: 'Paper Planes',
      isFavorite: true
    });
    expect(events.map((e) => e.event)).toContain('library-changed');

    // 同じ形式への置き換えは、出力先が元のファイル自身（変換できる）
    const again = await commands.planConversion([track.id], options({ mode: 'replace' }));
    expect(again.entries[0].status).toBe('convert');
  });

  it('中断すると、残りの曲を変換しない', async () => {
    const tracks = (await commands.getAllTracks()).filter((t) => !t.isMissing).slice(0, 3);
    backend = createMockBackend({
      emit: (event, payload) => {
        events.push({ event: MOCK_EVENT_NAMES[event], payload });
        if (event === 'convertProgress') void commands.cancelConversion();
      },
      sleep: async () => {}
    });
    mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));

    const result = await commands.convertTracks(
      tracks.map((t) => t.id),
      options({ mode: 'replace' })
    );

    expect(result).toMatchObject({ convertedCount: 1, cancelled: true });
  });

  it('入力の誤りは、VALIDATIONのエラーにする', async () => {
    const track = await byTitle('Paper Planes');
    const invalid = { code: 'VALIDATION' };
    await expect(commands.planConversion([], options())).rejects.toMatchObject(invalid);
    await expect(commands.planConversion(['x'], options())).rejects.toMatchObject(invalid);
    await expect(
      commands.planConversion([track.id], options({ template: '%nope%' }))
    ).rejects.toMatchObject(invalid);
    await expect(
      commands.convertTracks([track.id], options({ format: 'aac', aacBitrateKbps: 100 }))
    ).rejects.toMatchObject(invalid);
    // AACのほかでは、ビットレートを確かめない
    await expect(
      commands.planConversion([track.id], options({ aacBitrateKbps: 0 }))
    ).resolves.toBeDefined();
  });
});

describe('音量の解析', () => {
  const pendingCount = async () => (await commands.getLoudnessScanStatus()).pendingCount;
  const progressEvents = () => events.filter((e) => e.event === 'loudness-scan-progress');

  it('音量の情報が欠けている曲を解析し、アルバムの全曲にアルバムのゲインを書く', async () => {
    const before = await commands.getAllTracks();
    const pending = before.filter((t) => !t.isMissing && t.replayGain.trackGain === null);
    expect(pending.length).toBeGreaterThan(0);
    expect(await pendingCount()).toBeGreaterThanOrEqual(pending.length);

    const result = await commands.startLoudnessScan(null);

    expect(result).toMatchObject({ errorCount: 0, errors: [], cancelled: false });
    expect(result.writtenCount).toBeGreaterThanOrEqual(pending.length);
    expect(await pendingCount()).toBe(0);
    const after = await commands.getAllTracks();
    for (const track of after.filter((t) => !t.isMissing)) {
      expect(track.replayGain.trackGain, track.fileName).not.toBeNull();
    }
    // アルバムのゲインは、アルバムのある曲にだけ書く
    for (const { id } of pending) {
      const track = after.find((t) => t.id === id)!;
      expect(track.replayGain.albumGain === null, track.fileName).toBe(track.album === null);
    }
    // すでにあった曲のゲインは、変えない
    for (const track of before.filter((t) => t.replayGain.trackGain !== null)) {
      expect(after.find((t) => t.id === track.id)?.replayGain.trackGain).toBe(
        track.replayGain.trackGain
      );
    }
    // 進捗と完了を知らせ、曲の一覧を取り直させる
    const total = progressEvents().length;
    expect(total).toBeGreaterThanOrEqual(pending.length);
    expect(progressEvents().at(-1)?.payload).toMatchObject({ current: total, total });
    expect(events.map((e) => e.event)).toContain('library-changed');
    expect(events.at(-1)).toEqual({ event: 'loudness-scan-finished', payload: result });

    // もう一度解析しても、対象はない
    events = [];
    expect(await commands.startLoudnessScan(null)).toMatchObject({
      writtenCount: 0,
      unchangedCount: 0
    });
    expect(progressEvents()).toEqual([]);
  });

  it('選んだ曲は、ゲインがあっても解析し直す（同じ値なら書き込まない）', async () => {
    await commands.startLoudnessScan(null);
    const tracks = await commands.getAllTracks();
    // フィクスチャで、はじめからゲインのあった曲
    const target = tracks.find((t) => t.album === 'Blue Horizon')!;
    const album = tracks.filter((t) => t.album === 'Blue Horizon' && !t.isMissing);
    events = [];

    // 選んだ曲のゲインを解析した値に書き換え、アルバムのほかの曲は読むだけ
    const result = await commands.startLoudnessScan([target.id]);
    expect(progressEvents()).toHaveLength(album.length);
    expect(result).toMatchObject({ writtenCount: 1, unchangedCount: album.length - 1 });
    const rescanned = (await commands.getAllTracks()).find((t) => t.id === target.id)!;
    expect(rescanned.replayGain.trackGain).not.toBe(target.replayGain.trackGain);
    expect(rescanned.replayGain.albumGain).toBe(target.replayGain.albumGain);

    // もう一度解析すると、同じ値のため書き込まない
    events = [];
    expect(await commands.startLoudnessScan([target.id])).toMatchObject({
      writtenCount: 0,
      unchangedCount: album.length
    });
    expect(events.map((e) => e.event)).not.toContain('library-changed');
  });

  it('中断すると、解析の途中のアルバムには何も書かない', async () => {
    const before = await pendingCount();
    // 1曲目を読み終わった時点で中断する
    backend = createMockBackend({
      emit: (event, payload) => {
        events.push({ event: MOCK_EVENT_NAMES[event], payload });
        if (event === 'loudnessScanProgress') void commands.cancelLoudnessScan();
      },
      sleep: async () => {}
    });
    mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));

    const result = await commands.startLoudnessScan(null);

    expect(result).toMatchObject({ writtenCount: 0, cancelled: true });
    expect(await pendingCount()).toBe(before);
    expect((await commands.getLoudnessScanStatus()).running).toBe(false);
  });

  it('同時に実行できる解析は1つで、入力の誤りはVALIDATIONのエラーにする', async () => {
    let release!: () => void;
    backend = createMockBackend({
      emit: () => {},
      sleep: () => new Promise((resolve) => (release = () => resolve()))
    });
    mockIPC((cmd, payload) => backend.invoke(cmd, payload as Record<string, unknown>));

    const running = commands.startLoudnessScan(null);
    await Promise.resolve();
    expect(await commands.getLoudnessScanStatus()).toMatchObject({ running: true, current: 0 });
    await expect(commands.startLoudnessScan(null)).rejects.toMatchObject({ code: 'LOCK' });
    await commands.cancelLoudnessScan();
    release();
    expect((await running).cancelled).toBe(true);

    await expect(commands.startLoudnessScan([])).rejects.toMatchObject({ code: 'VALIDATION' });
    await expect(commands.startLoudnessScan(['x'])).rejects.toMatchObject({ code: 'VALIDATION' });
  });
});

describe('ファイルの整理', () => {
  const DEFAULT = '%albumartist%/%album%/%track% %title%';

  it('計画を立てるだけでは、曲の場所を変えない', async () => {
    const tracks = await commands.getAllTracks();
    const compilation = tracks.find((track) => track.title === 'Harbor Lights')!;

    const plan = await commands.planFileOrganize([compilation.id], DEFAULT, 'libraryFolder');

    // コンピレーションの曲は、アルバムアーティストのフォルダへ移す
    expect(plan.entries).toEqual([
      {
        trackId: compilation.id,
        currentPath: 'ネオン通り/City Nights Collection/01 Harbor Lights.mp3',
        newPath: 'Various Artists/City Nights Collection/01 Harbor Lights.mp3',
        status: 'move'
      }
    ]);
    const after = await commands.getAllTracks();
    expect(after.find((track) => track.id === compilation.id)?.filePath).toBe(compilation.filePath);
  });

  it('動かせる曲だけを移し、トラックのIDとお気に入りは変えない', async () => {
    backend.addDuplicateTracks();
    const tracks = await commands.getAllTracks();
    const downloads = tracks.filter((track) => track.filePath.includes('/Downloads/'));
    expect(downloads).toHaveLength(4);
    const favorite = downloads.find((track) => track.fileName === 'overclock.flac')!;
    await commands.setFavorite([favorite.id], true);
    events = [];

    const ids = downloads.map((track) => track.id);
    const plan = await commands.planFileOrganize(ids, DEFAULT, 'libraryFolder');
    expect(plan.entries.map((entry) => entry.status).sort()).toEqual([
      'move',
      'sameTarget',
      'sameTarget',
      'targetExists'
    ]);

    const result = await commands.organizeFiles(ids, DEFAULT, 'libraryFolder');

    expect(result).toEqual({
      movedCount: 1,
      skippedCount: 3,
      failedCount: 0,
      errors: [],
      cancelled: false
    });
    expect(events).toContainEqual({
      event: 'organize-progress',
      payload: { current: 1, total: 1, currentFile: 'overclock.flac' }
    });
    const moved = (await commands.getAllTracks()).find((track) => track.id === favorite.id)!;
    expect(moved.filePath).toBe(
      '/Users/demo/Music/The Voltage/Midnight Circuit/01 ｏｖｅｒｃｌｏｃｋ.flac'
    );
    expect(moved.fileName).toBe('01 ｏｖｅｒｃｌｏｃｋ.flac');
    expect(moved.isFavorite).toBe(true);
    expect(events.map((e) => e.event)).toContain('library-changed');
  });

  it('入力の誤りは、VALIDATIONのエラーにする', async () => {
    const [track] = await commands.getAllTracks();
    await expect(commands.planFileOrganize([], DEFAULT, 'libraryFolder')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.planFileOrganize(['x'], DEFAULT, 'libraryFolder')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(
      commands.organizeFiles([track.id], '%unknown%', 'libraryFolder')
    ).rejects.toMatchObject({
      code: 'VALIDATION',
      message: '使えない項目です: %unknown%'
    });
  });
});

describe('音楽フォルダへ集める', () => {
  /** 別のドライブ（`/Volumes/Backup`）から、参照して取り込んだ曲（ファイル名の順） */
  async function importFromBackup() {
    backend.setFolderResult('/Volumes/Backup/Album');
    await commands.importFolder('Skip');
    return (await commands.getAllTracks())
      .filter((track) => track.filePath.startsWith('/Volumes/Backup/Album/'))
      .sort((a, b) => a.fileName.localeCompare(b.fileName));
  }

  it('音楽フォルダが未設定なら、NOT_FOUNDにする', async () => {
    const [track] = await commands.getAllTracks();
    await expect(
      commands.planFileOrganize([track.id], '%title%', 'musicFolder')
    ).rejects.toMatchObject({ code: 'NOT_FOUND' });
  });

  it('音楽フォルダの書式で、別のドライブの曲はコピーとして計画する', async () => {
    const backup = await importFromBackup();
    await commands.setMusicFolder('system');

    // 画面の書式は使わない
    const plan = await commands.planFileOrganize(
      backup.map((track) => track.id),
      '%title%',
      'musicFolder'
    );

    expect(plan.entries[0]).toMatchObject({
      currentPath: 'Mock Track 01.mp3',
      newPath: 'Mock Importer/Album/1-01 Mock Track 01.mp3',
      status: 'move'
    });
    expect(plan.copyCount).toBe(6);
    expect(plan.copyBytes).toBe(backup.reduce((sum, track) => sum + track.fileSize, 0));
    expect(plan.freeBytes).toBeGreaterThan(plan.copyBytes);
  });

  it('集めた曲は音楽フォルダへ移り、IDは変えない', async () => {
    const backup = await importFromBackup();
    await commands.setMusicFolder('system');
    const ids = backup.map((track) => track.id);

    const result = await commands.organizeFiles(ids, '', 'musicFolder');

    expect(result).toMatchObject({ movedCount: 6, cancelled: false });
    const moved = (await commands.getAllTracks()).find((track) => track.id === ids[0])!;
    expect(moved.filePath).toBe('/Users/demo/Music/Mock Importer/Album/1-01 Mock Track 01.mp3');
  });

  it('空き容量が足りなければ、何も移さない', async () => {
    const backup = await importFromBackup();
    await commands.setMusicFolder('system');
    backend.setMusicFolderFreeBytes(1024);

    await expect(
      commands.organizeFiles(
        backup.map((track) => track.id),
        '',
        'musicFolder'
      )
    ).rejects.toMatchObject({ code: 'VALIDATION' });
    expect(await commands.getAllTracks()).toEqual(expect.arrayContaining(backup));
  });

  it('中止すると、次の曲の前で止まる', async () => {
    // 1曲目を移している間（整理の待ち時間）に止めて、中止を送る
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    backend = createMockBackend({
      emit: (event, payload) => events.push({ event: MOCK_EVENT_NAMES[event], payload }),
      sleep: (ms) => (ms === 200 ? gate : Promise.resolve())
    });
    const backup = await importFromBackup();
    await commands.setMusicFolder('system');
    events = [];

    const running = commands.organizeFiles(
      backup.map((track) => track.id),
      '',
      'musicFolder'
    );
    await vi.waitFor(() => expect(events.map((e) => e.event)).toContain('organize-progress'));
    // 実行中は、ほかの整理を始めない
    await expect(commands.organizeFiles([backup[0].id], '', 'musicFolder')).rejects.toMatchObject({
      code: 'LOCK'
    });
    await commands.cancelFileOrganize();
    release();
    const result = await running;

    expect(result).toMatchObject({ cancelled: true, movedCount: 1 });
  });
});

describe('音楽フォルダを整理された状態に保つ', () => {
  /** 音楽フォルダ（OSの音楽フォルダ）の中の曲 */
  const inMusicFolder = async () =>
    (await commands.getAllTracks()).find(
      (track) => track.title === 'Harbor Lights' && !track.isMissing
    )!;

  it('設定がオフなら、タグを保存しても移さない', async () => {
    await commands.setMusicFolder('system');
    const track = await inMusicFolder();

    const organized = await commands.updateTrackMetadata(track.id, {
      title: 'Harbor Lights',
      artist: track.artist,
      album: 'Renamed Album',
      albumArtist: track.albumArtist,
      trackNumber: track.trackNumber
    });

    expect(organized).toBeNull();
    expect((await inMusicFolder()).filePath).toBe(track.filePath);
  });

  it('オンなら、音楽フォルダの曲を書式どおりの場所へ移し直す', async () => {
    await commands.setMusicFolder('system');
    const settings = await commands.getSettings();
    await commands.saveSettings({ ...settings, keepMusicFolderOrganized: true });
    const track = await inMusicFolder();
    events = [];

    const result = await commands.updateMultipleTracksMetadata([track.id], {
      album: 'Renamed Album'
    });

    expect(result.organized).toEqual({ movedCount: 1, notMovedCount: 0, errors: [] });
    expect((await inMusicFolder()).filePath).toBe(
      '/Users/demo/Music/Various Artists/Renamed Album/1-01 Harbor Lights.mp3'
    );
    expect(events.map((e) => e.event)).toContain('library-changed');
  });

  it('音楽フォルダが未設定なら、移さない', async () => {
    const settings = await commands.getSettings();
    await commands.saveSettings({ ...settings, keepMusicFolderOrganized: true });
    const track = await inMusicFolder();

    const result = await commands.applyMetadataChanges([
      { trackId: track.id, metadata: { album: 'Renamed Album' } }
    ]);

    expect(result.organized).toBeNull();
    expect((await inMusicFolder()).filePath).toBe(track.filePath);
  });
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
    expect(await commands.searchTracks('   ')).toEqual([]);
  });

  it('検索は記号を取り除かず、文字として探す', async () => {
    // 記号を取り除くと「voltage」になり、The Voltageの曲に一致してしまう
    expect(await commands.searchTracks("voltage'")).toEqual([]);
    expect(await commands.searchTracks('"voltage"')).toEqual([]);
    expect((await commands.searchTracks('voltage')).length).toBeGreaterThan(0);
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

  it('アルバムアーティストのアルバムは、アルバムの一覧の1件と同じまとまりで、まとめたアーティストの下に入る', async () => {
    const albums = await commands.getAlbums();
    // どのアルバムも、まとめたアーティストのアルバムに1回だけ入る（アルバムのない曲は含めない）
    const artists = [...new Set(albums.map((album) => album.artist))];
    const groups = (
      await Promise.all(artists.map((artist) => commands.getAlbumArtistAlbums(artist)))
    ).flat();
    expect(groups.map((group) => [group.name, group.artist]).sort()).toEqual(
      albums.map((album) => [album.name, album.artist]).sort()
    );
    expect(groups.map((group) => group.name)).not.toContain('不明なアルバム');

    // コンピレーションは、アルバムアーティストの下だけに入る（参加アーティストの下には入らない）
    const various = await commands.getAlbumArtistAlbums('Various Artists');
    expect(various.map((album) => album.name)).toEqual(['City Nights Collection']);
    expect(various[0].tracks.map((t) => t.artist)).toEqual([
      'ネオン通り',
      'Aoi Sora',
      'Mika Hayashi'
    ]);
    expect(
      (await commands.getAlbumArtistAlbums('Aoi Sora')).map((album) => album.name)
    ).not.toContain('City Nights Collection');
    // アルバムの曲の一覧には、まとめたアーティストの読みを含めない（Rustの`AlbumGroup`と同じ）
    expect(various[0]).not.toHaveProperty('artistSortName');
  });

  it('アーティストのアルバムでは、アルバム未設定のトラックを「不明なアルバム」にまとめる', async () => {
    const artist = (await commands.getArtists()).find((a) => a.name === 'The Voltage');
    const albums = await commands.getArtistAlbums('The Voltage');

    expect(albums.map((album) => album.name)).toContain('不明なアルバム');
    expect(artist?.albumCount).toBe(albums.length);
    expect(artist?.trackCount).toBe(albums.reduce((total, album) => total + album.trackCount, 0));
    expect(artist?.representativeTrackId).toBe(albums[0].representativeTrackId);
  });

  it('アーティスト・ジャンルが複数ある曲は、それぞれの値の一覧・絞り込みに出る', async () => {
    const track = (await commands.getAllTracks()).find(
      (t) => !t.isMissing && t.albumArtist === null && t.album !== null
    )!;
    const tags = await commands.getTrackTags(track.id);
    await commands.updateTrackMetadata(track.id, {
      ...tags,
      artist: 'Duo One; Duo Two',
      genre: 'Bossa Nova; Chanson',
      artistSort: 'でゅお わん; でゅお つー'
    });

    // アーティストの一覧: それぞれのアーティストに入る（つないだ文字列の項目はない）
    const artists = await commands.getArtists();
    const names = artists.map((artist) => artist.name);
    expect(names).toEqual(expect.arrayContaining(['Duo One', 'Duo Two']));
    expect(names).not.toContain('Duo One; Duo Two');
    const duoTwo = artists.find((artist) => artist.name === 'Duo Two')!;
    expect(duoTwo).toMatchObject({ trackCount: 1, albumCount: 1, sortName: 'でゅお つー' });
    expect(duoTwo.representativeTrackId).toBe(track.id);

    // アーティストの詳細: アルバムの`artist`は、アルバムをまとめるアーティスト（つないだ文字列）
    const albums = await commands.getArtistAlbums('Duo Two');
    expect(albums).toHaveLength(1);
    expect(albums[0]).toMatchObject({ name: track.album, artist: 'Duo One; Duo Two' });
    expect(albums[0].tracks.map((t) => t.id)).toEqual([track.id]);
    expect(
      (await commands.getAlbumTracks(track.album!, 'Duo One; Duo Two')).map((t) => t.id)
    ).toEqual([track.id]);
    expect(await commands.getArtistAlbums('Duo One; Duo Two')).toEqual([]);
    // アルバムアーティストのアルバムは、つないだ文字列で取得する（それぞれのアーティストでは取得しない）
    const duo = await commands.getAlbumArtistAlbums('Duo One; Duo Two');
    expect(duo.map((album) => album.tracks.map((t) => t.id))).toEqual([[track.id]]);
    expect(await commands.getAlbumArtistAlbums('Duo Two')).toEqual([]);
    // アルバムの一覧には、まとめたアーティストの読みを入れる
    const duoAlbum = (await commands.getAlbums()).find(
      (album) => album.artist === 'Duo One; Duo Two'
    );
    expect(duoAlbum?.artistSortName).toBe('でゅお わん; でゅお つー');

    // ジャンル
    const genres = (await commands.getGenres()).map((genre) => genre.name);
    expect(genres).toEqual(expect.arrayContaining(['Bossa Nova', 'Chanson']));
    expect(genres).not.toContain('Bossa Nova; Chanson');
    expect((await commands.getGenreTracks('Chanson')).map((t) => t.id)).toEqual([track.id]);

    // 値の一覧
    expect(await commands.getUniqueArtists()).toEqual(
      expect.arrayContaining(['Duo One', 'Duo Two'])
    );
    expect(await commands.getUniqueGenres()).toEqual(
      expect.arrayContaining(['Bossa Nova', 'Chanson'])
    );
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

    const update = await commands.incrementPlayCount(trackId);
    expect(update.playCount).toBe(track.playCount + 1);
    expect((await commands.getAllTracks()).find((t) => t.id === trackId)?.lastPlayedAt).toBe(
      update.lastPlayedAt
    );

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

  it('プレイリストの名前は、前後の空白を除いて作る（改名と同じ）', async () => {
    const created = await commands.createPlaylist('  通勤 ');
    expect(created.name).toBe('通勤');
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

  it('引数は複製して受け取る（呼び出し側のオブジェクトを持ち続けない）', async () => {
    // 複製できないオブジェクト（画面の状態のプロキシなど）を渡されても、後で結果として返せる
    const hidden = new Proxy(['years'], {});
    const settings = { ...(await commands.getSettings()), hiddenSidebarItems: hidden };

    await backend.invoke('save_settings', { settings });
    hidden.push('folders');

    expect((await commands.getSettings()).hiddenSidebarItems).toEqual(['years']);
  });

  it('グローバルホットキーは、保存した設定がオンの間だけ登録されている', async () => {
    const off = await commands.getGlobalShortcuts();
    expect(off.map((shortcut) => shortcut.action)).toEqual(['toggle', 'next', 'previous']);
    expect(off.every((shortcut) => !shortcut.registered)).toBe(true);
    expect(off[0].keys.at(-1)).toBe('P');

    await commands.saveSettings({ ...(await commands.getSettings()), globalShortcuts: true });
    const on = await commands.getGlobalShortcuts();
    // 「次の曲」は、ほかのアプリが使っていて登録できなかったことにしている
    expect(on.map((shortcut) => shortcut.registered)).toEqual([true, false, true]);
  });

  it('ミニプレーヤーへの切り替えを受け取る', async () => {
    expect(backend.miniPlayer()).toEqual({ enabled: false, alwaysOnTop: false });

    await commands.setMiniPlayer(true, true);
    expect(backend.miniPlayer()).toEqual({ enabled: true, alwaysOnTop: true });
    await commands.setMiniPlayer(true, false);
    expect(backend.miniPlayer()).toEqual({ enabled: true, alwaysOnTop: false });
    await commands.setMiniPlayer(false, true);
    expect(backend.miniPlayer()).toEqual({ enabled: false, alwaysOnTop: false });
  });

  it('歌詞は、.lrcファイルを優先し、なければ埋め込みの歌詞を返す', async () => {
    const synced = await commands.getTrackLyrics(mockTrackId(1));
    expect(synced?.source).toBe('lrcFile');
    expect(synced?.text).toContain('[00:05.00]');

    const plain = await commands.getTrackLyrics(mockTrackId(2));
    expect(plain?.source).toBe('embedded');
    expect(plain?.text).toContain('紙ひこうき');

    expect(await commands.getTrackLyrics(mockTrackId(3))).toBeNull();

    // 編集画面で入れた歌詞は、埋め込みの歌詞として返る（.lrcファイルのある曲は、ファイルが優先）
    await commands.updateTrackMetadata(mockTrackId(3), {
      title: '雨上がりのメロディ',
      lyrics: '新しい歌詞'
    });
    expect(await commands.getTrackLyrics(mockTrackId(3))).toEqual({
      text: '新しい歌詞',
      source: 'embedded'
    });
    await expect(commands.getTrackLyrics('invalid')).rejects.toMatchObject({ code: 'VALIDATION' });
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

    expect(result).toEqual({ updatedCount: 2, failedCount: 0, errors: [], organized: null });
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

    expect(result).toEqual({
      updatedCount: ids.length,
      failedCount: 0,
      errors: [],
      organized: null
    });
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

    expect(result).toEqual({ updatedCount: 2, failedCount: 0, errors: [], organized: null });
    expect(backend.albumArtUrl(folder.id)).toBe(folderUrl);
    expect(await commands.getAlbumArtInfo(embedded.id)).toBeNull();
    expect(backend.albumArtUrl(embedded.id)).toBeNull();
    const [after] = await albumTracks('Midnight Circuit');
    expect(after.fileSize).toBe(folder.fileSize);

    // 埋め込みの画像がない曲は書き換えず、件数にも数えない
    expect(await commands.removeAlbumArt([folder.id, embedded.id])).toEqual({
      updatedCount: 0,
      failedCount: 0,
      errors: [],
      organized: null
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
    const first = await importFolder('/Users/demo/Music/New');
    const second = await importFolder('/Users/demo/Music/New');

    expect(first).toMatchObject({
      kind: 'imported',
      result: { importedCount: 6, skippedCount: 0 }
    });
    expect(second).toMatchObject({
      kind: 'imported',
      result: { importedCount: 0, skippedCount: 6 }
    });
    expect(events[0]).toEqual({
      event: 'import-progress',
      payload: { phase: 'reading', current: 1, total: 6, currentFile: 'Mock Track 01.mp3' }
    });
  });

  it('フォルダを選ばずに閉じたら、nullを返して何もしない', async () => {
    const tracks = await commands.getAllTracks();
    const folders = await commands.getLibraryFolders();
    backend.setFolderResult(null);

    expect(await commands.importFolder('Skip')).toBeNull();
    expect(await commands.getAllTracks()).toEqual(tracks);
    expect(await commands.getLibraryFolders()).toEqual(folders);
    expect(events).toEqual([]);
  });

  it('ファイルを選ぶインポートは、曲だけを登録する（フォルダは記録しない）', async () => {
    const folders = (await commands.getLibraryFolders()).folders.length;

    // 音楽フォルダを設定していなければ、今の場所のまま登録する
    const result = await commands.importFiles('Skip');
    expect(result).toMatchObject({
      kind: 'imported',
      result: { importedCount: 2, skippedCount: 0 }
    });
    const picked = (await commands.getAllTracks()).filter((track) =>
      track.filePath.startsWith('/Users/mock/Downloads/')
    );
    expect(picked.map((track) => track.fileName).sort()).toEqual([
      'Picked Song A.flac',
      'Picked Song B.mp3'
    ]);
    expect(picked.map((track) => track.format).sort()).toEqual(['FLAC', 'MP3']);
    expect((await commands.getLibraryFolders()).folders).toHaveLength(folders);
    expect(events.map((e) => e.event)).toContain('library-changed');

    // 同じファイルは、重複としてスキップする
    expect(await commands.importFiles('Skip')).toMatchObject({
      result: { importedCount: 0, skippedCount: 2 }
    });
    // 選ばずに閉じた場合は、何もしない
    backend.setFileImportMode('cancel');
    expect(await commands.importFiles('Skip')).toBeNull();
  });

  it('ドロップされたものは、音楽ファイルだけをインポートする', async () => {
    const result = await commands.importDroppedFiles(['Dropped.FLAC', 'cover.jpg', 'notes.txt']);
    expect(result).toMatchObject({
      kind: 'imported',
      result: { importedCount: 1, skippedCount: 0 }
    });
    const tracks = await commands.getAllTracks();
    expect(tracks.some((track) => track.filePath === '/Users/mock/Desktop/Dropped.FLAC')).toBe(
      true
    );

    // 音楽ファイルが1つもなければエラー
    await expect(commands.importDroppedFiles(['cover.jpg'])).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(commands.importDroppedFiles([])).rejects.toMatchObject({ code: 'VALIDATION' });
  });

  it('開かれたファイルは、インポートしてから知らせ、1回だけ渡す', async () => {
    expect(await commands.takeOpenedFiles()).toEqual({ tracks: [], errors: [] });

    backend.openFiles(['Opened B.mp3', 'readme.txt', 'Opened A.flac']);
    await vi.waitFor(() => expect(events.map((e) => e.event)).toContain('tracks-opened'));

    const opened = await commands.takeOpenedFiles();
    // 開かれた順
    expect(opened.tracks.map((track) => track.fileName)).toEqual(['Opened B.mp3', 'Opened A.flac']);
    expect(opened.errors).toHaveLength(1);
    expect(opened.errors[0]).toContain('readme.txt');
    // ライブラリに入っている
    const ids = (await commands.getAllTracks()).map((track) => track.id);
    expect(ids).toEqual(expect.arrayContaining(opened.tracks.map((track) => track.id)));
    // 受け取った分は、残らない
    expect(await commands.takeOpenedFiles()).toEqual({ tracks: [], errors: [] });
  });
});

describe('音楽フォルダへのコピー', () => {
  /** 取り込んだ結果（取り込んでいなければ、テストを失敗させる） */
  function imported(outcome: Awaited<ReturnType<typeof commands.importDroppedFiles>> | null) {
    if (outcome?.kind !== 'imported') throw new Error(`取り込んでいない: ${outcome?.kind}`);
    return outcome.result;
  }
  const trackPaths = async (prefix: string) =>
    (await commands.getAllTracks())
      .map((track) => track.filePath)
      .filter((path) => path.startsWith(prefix))
      .sort();

  beforeEach(async () => {
    await commands.setMusicFolder('system');
  });

  it('ファイルは、音楽フォルダへ書式どおりにコピーし、同じファイルをもう一度取り込むとスキップする', async () => {
    const result = imported(await commands.importFiles('Skip'));

    expect(result).toMatchObject({ importedCount: 2, copiedCount: 2, notCopied: [] });
    expect(await trackPaths('/Users/demo/Music/Mock Importer/')).toEqual([
      '/Users/demo/Music/Mock Importer/Downloads/1-01 Picked Song A.flac',
      '/Users/demo/Music/Mock Importer/Downloads/1-02 Picked Song B.mp3'
    ]);
    // 元の場所では登録しない
    expect(await trackPaths('/Users/mock/Downloads/')).toEqual([]);
    expect(events.map((e) => e.payload)).toContainEqual(
      expect.objectContaining({ phase: 'copying', current: 2, total: 2 })
    );

    const again = imported(await commands.importFiles('Skip'));
    expect(again).toMatchObject({ importedCount: 0, skippedCount: 2, copiedCount: 0 });
  });

  it('設定がオフなら、今の場所のまま登録する', async () => {
    const settings = await commands.getSettings();
    await commands.saveSettings({ ...settings, copyToMusicFolder: false });

    const result = imported(await commands.importDroppedFiles(['Dropped.flac']));

    expect(result.copiedCount).toBe(0);
    expect(await trackPaths('/Users/mock/Desktop/')).toEqual(['/Users/mock/Desktop/Dropped.flac']);
  });

  it('コピー先に別の曲があれば、上書きせず、取り込まずに知らせる', async () => {
    const settings = await commands.getSettings();
    await commands.saveSettings({ ...settings, musicFolderTemplate: '%title%' });
    await commands.importFiles('Skip');

    // 別の場所の、同じ名前の曲
    const result = imported(await commands.importDroppedFiles(['Picked Song A.flac']));

    expect(result.importedCount).toBe(0);
    expect(result.notCopied).toEqual([
      {
        source: '/Users/mock/Desktop/Picked Song A.flac',
        target: 'Picked Song A.flac',
        reason: 'targetExists'
      }
    ]);
    expect(await trackPaths('/Users/mock/Desktop/')).toEqual([]);
  });

  it('ライブラリフォルダの外のフォルダは、取り込み方を選ぶまで預かる', async () => {
    const outcome = await importFolder('/Volumes/Backup/Album');
    expect(outcome).toEqual({
      kind: 'chooseFolderMode',
      pending: {
        folders: ['/Volumes/Backup/Album'],
        trackCount: 6,
        totalBytes: expect.any(Number),
        copyCount: 6,
        copyBytes: expect.any(Number),
        musicFolder: '/Users/demo/Music',
        musicFolderExists: true,
        freeBytes: expect.any(Number)
      }
    });
    // まだ取り込んでいない
    expect(await trackPaths('/Users/demo/Music/Mock Importer/Album/')).toEqual([]);

    const result = imported(await commands.importPendingFolders('copy', 'Skip'));
    expect(result.copiedCount).toBe(6);
    expect(await trackPaths('/Users/demo/Music/Mock Importer/Album/')).toHaveLength(6);
    // コピーした時は、元のフォルダをライブラリフォルダとして記録しない
    const folders = (await commands.getLibraryFolders()).folders.map((folder) => folder.path);
    expect(folders).not.toContain('/Volumes/Backup/Album');
    // 預かったものは、なくなる
    await expect(commands.importPendingFolders('copy', 'Skip')).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('ドロップしたフォルダも預かり、やめたら手放す', async () => {
    expect(await commands.importDroppedFiles(['Album', 'Single.mp3'])).toMatchObject({
      kind: 'chooseFolderMode',
      pending: { folders: ['/Users/mock/Desktop/Album'], trackCount: 7 }
    });

    await commands.cancelPendingImport();

    await expect(commands.importPendingFolders('reference', 'Skip')).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('空き容量が足りなければ何も取り込まず、預かったフォルダは参照して取り込める', async () => {
    backend.setMusicFolderFreeBytes(1024);
    await importFolder('/Volumes/Backup/Album');

    const outcome = await commands.importPendingFolders('copy', 'Skip');

    expect(outcome).toMatchObject({ kind: 'notEnoughSpace', freeBytes: 1024 });
    expect(await trackPaths('/Volumes/Backup/Album/')).toEqual([]);
    const result = imported(await commands.importPendingFolders('reference', 'Skip'));
    expect(result.importedCount).toBe(6);
    expect(await trackPaths('/Volumes/Backup/Album/')).toHaveLength(6);
  });

  it('音楽フォルダが見つからなければ、何も取り込まない', async () => {
    backend.setFolderResult('/Volumes/External/Music');
    await commands.setMusicFolder('pick');

    expect(await commands.importFiles('Skip')).toEqual({
      kind: 'musicFolderMissing',
      path: '/Volumes/External/Music'
    });
    expect(await trackPaths('/Users/mock/Downloads/')).toEqual([]);
  });

  it('開かれたファイルは、コピー先の曲を渡す', async () => {
    backend.openFiles(['Opened.mp3']);
    await vi.waitFor(() => expect(events.map((e) => e.event)).toContain('tracks-opened'));

    const opened = await commands.takeOpenedFiles();
    expect(opened.tracks.map((track) => track.filePath)).toEqual([
      '/Users/demo/Music/Mock Importer/Desktop/1-01 Opened.mp3'
    ]);
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
    await importFolder('/Users/demo/Imports/A');
    await importFolder('/Users/demo/Imports/A/Live');
    let paths = (await commands.getLibraryFolders()).folders.map((f) => f.path);
    expect(paths).toContain('/Users/demo/Imports/A');
    expect(paths).not.toContain('/Users/demo/Imports/A/Live');

    await importFolder('/Users/demo/Imports');
    paths = (await commands.getLibraryFolders()).folders.map((f) => f.path);
    expect(paths).toContain('/Users/demo/Imports');
    expect(paths).not.toContain('/Users/demo/Imports/A');
  });

  it('末尾の区切り文字を除いて登録し、ルートのフォルダもRustと同じ接頭辞で数える', async () => {
    await importFolder('/Users/demo/Imports/B/');
    let folders = (await commands.getLibraryFolders()).folders;
    const imported = folders.find((f) => f.path === '/Users/demo/Imports/B');
    expect(imported?.trackCount).toBeGreaterThan(0);
    // インポートしたら、設定ウィンドウの一覧も読み直せるよう変更を通知する
    expect(events.map((e) => e.event)).toContain('library-changed');

    await importFolder('/');
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

describe('読み込まないフォルダ', () => {
  const musicFolder = async () =>
    (await commands.getLibraryFolders()).folders.find((f) => f.path === '/Users/demo/Music')!;

  it('ライブラリフォルダごとに、相対パスと中の曲数を返す', async () => {
    const music = await musicFolder();
    expect(music.exclusions.map((e) => [e.path, e.trackCount])).toEqual([
      ['Music', 0],
      ['MusicBee', 0]
    ]);
  });

  it('足したフォルダの曲はライブラリに残し、外す操作で外す', async () => {
    const before = await musicFolder();
    const added = await commands.addLibraryFolderExclusion(before.id);
    expect(added?.trackCount).toBeGreaterThan(0);
    const total = (await commands.getAllTracks()).length;

    let music = await musicFolder();
    expect(music.exclusions.map((e) => e.path)).toContain(added!.path);
    expect(music.trackCount).toBe(before.trackCount);

    await expect(commands.removeExcludedTracks(added!.id)).resolves.toBe(added!.trackCount);
    expect(await commands.getAllTracks()).toHaveLength(total - added!.trackCount);
    expect(events.map((e) => e.event)).toContain('library-changed');
    music = await musicFolder();
    expect(music.exclusions.find((e) => e.id === added!.id)?.trackCount).toBe(0);
  });

  it('選ばなかった場合はnull。中・外・同じフォルダは拒否し、含むフォルダはまとめる', async () => {
    const music = await musicFolder();
    backend.setExclusionFolder(null);
    await expect(commands.addLibraryFolderExclusion(music.id)).resolves.toBeNull();

    for (const path of [
      '/Users/demo/Music',
      '/Users/demo/Other',
      '/Users/demo/Music/Music/Media'
    ]) {
      backend.setExclusionFolder(path);
      await expect(commands.addLibraryFolderExclusion(music.id)).rejects.toMatchObject({
        code: 'VALIDATION'
      });
    }

    backend.setExclusionFolder('/Users/demo/Music/Apps/MusicBee/Podcasts');
    await commands.addLibraryFolderExclusion(music.id);
    backend.setExclusionFolder('/Users/demo/Music/Apps');
    await commands.addLibraryFolderExclusion(music.id);
    expect((await musicFolder()).exclusions.map((e) => e.path)).toEqual([
      'Apps',
      'Music',
      'MusicBee'
    ]);
  });

  it('一覧から外すと再スキャンし、見つからないフォルダでは再スキャンしない', async () => {
    const music = await musicFolder();
    const result = await commands.removeLibraryFolderExclusion(music.exclusions[0].id);
    expect(result).toMatchObject({ addedCount: 0, missingSkipped: false });
    expect((await musicFolder()).exclusions.map((e) => e.path)).toEqual(['MusicBee']);

    const external = (await commands.getLibraryFolders()).folders.find((f) => !f.exists)!;
    await expect(commands.addLibraryFolderExclusion(external.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
    await expect(commands.removeLibraryFolderExclusion(crypto.randomUUID())).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('中のフォルダをまとめて登録すると、読み込まないフォルダを引き継ぐ', async () => {
    const music = await musicFolder();
    await commands.removeLibraryFolder(music.id, false);
    await importFolder('/Users/demo/Music');
    const reimported = await musicFolder();
    backend.setExclusionFolder('/Users/demo/Music/Podcasts');
    await commands.addLibraryFolderExclusion(reimported.id);

    await importFolder('/Users/demo');
    const demo = (await commands.getLibraryFolders()).folders.find(
      (f) => f.path === '/Users/demo'
    )!;
    expect(demo.exclusions.map((e) => e.path)).toEqual(['Music/Podcasts']);
  });
});

describe('ライブラリのデータのバックアップ', () => {
  it('保存先を選ぶと、今のライブラリの件数を返す（拡張子がなければ付ける）', async () => {
    const tracks = await commands.getAllTracks();
    await expect(commands.exportLibraryBackup()).resolves.toMatchObject({
      path: '/Users/demo/Documents/Muspice Backup.muspicebackup',
      trackCount: tracks.length
    });
    backend.setBackupFile('/Users/demo/Backup');
    await expect(commands.exportLibraryBackup()).resolves.toMatchObject({
      path: '/Users/demo/Backup.muspicebackup'
    });
    backend.setBackupFile(null);
    await expect(commands.exportLibraryBackup()).resolves.toBeNull();
  });

  it('復元できるファイルを預かり、復元するかやめると手放す', async () => {
    await expect(commands.planLibraryRestore()).resolves.toMatchObject({
      kind: 'ready',
      fileName: 'Muspice Backup.muspicebackup',
      includesSettings: true
    });
    await expect(commands.restoreLibraryBackup()).resolves.toBeNull();
    await expect(commands.restoreLibraryBackup()).rejects.toMatchObject({ code: 'NOT_FOUND' });

    backend.setBackupFile('/Users/demo/muspice.db');
    await expect(commands.planLibraryRestore()).resolves.toMatchObject({
      kind: 'ready',
      includesSettings: false
    });
    await commands.cancelLibraryRestore();
    await expect(commands.restoreLibraryBackup()).rejects.toMatchObject({ code: 'NOT_FOUND' });
  });

  it('復元できないファイルは、理由を返して預からない', async () => {
    for (const [path, kind] of [
      ['/Users/demo/notes.txt', 'notBackup'],
      ['/Users/demo/damaged.muspicebackup', 'damaged'],
      ['/Users/demo/newer.muspicebackup', 'newerFormat']
    ]) {
      backend.setBackupFile(path);
      await expect(commands.planLibraryRestore()).resolves.toMatchObject({ kind });
      await expect(commands.restoreLibraryBackup()).rejects.toMatchObject({ code: 'NOT_FOUND' });
    }
  });
});

describe('ライブラリフォルダの場所の変更', () => {
  const folderAt = async (path: string) =>
    (await commands.getLibraryFolders()).folders.find((f) => f.path === path);

  it('選んだ場所を預かり、付け替えると曲のパスだけを変える（ID・読み込まないフォルダは保つ）', async () => {
    const music = (await folderAt('/Users/demo/Music'))!;
    const before = await commands.getAllTracks();
    const missingCount = before.filter((track) => track.isMissing).length;

    const relocation = await commands.planLibraryFolderRelocation(music.id);
    expect(relocation).toEqual({
      kind: 'ready',
      path: '/Volumes/Backup/Music',
      trackCount: music.trackCount,
      foundCount: music.trackCount - missingCount
    });

    events.length = 0;
    const result = await commands.relocateLibraryFolder(music.id);
    expect(result).toMatchObject({
      path: '/Volumes/Backup/Music',
      relocatedCount: music.trackCount,
      rescan: { addedCount: 0, missingCount: 0 }
    });
    expect(events.map((e) => e.event)).toContain('library-changed');

    const after = await commands.getAllTracks();
    expect(after.map((track) => track.id)).toEqual(before.map((track) => track.id));
    expect(after.map((track) => track.filePath)).toEqual(
      before.map((track) => track.filePath.replace('/Users/demo/Music/', '/Volumes/Backup/Music/'))
    );
    const relocated = (await folderAt('/Volumes/Backup/Music'))!;
    expect(relocated).toMatchObject({ id: music.id, exists: true, trackCount: music.trackCount });
    expect(relocated.exclusions.map((e) => e.path)).toEqual(['Music', 'MusicBee']);
  });

  it('選ばなかった場合はnull。やめると手放し、付け替えられない', async () => {
    const external = (await commands.getLibraryFolders()).folders.find((f) => !f.exists)!;
    backend.setRelocationFolder(null);
    await expect(commands.planLibraryFolderRelocation(external.id)).resolves.toBeNull();

    backend.setRelocationFolder('/Volumes/External 2/Music/');
    await expect(commands.planLibraryFolderRelocation(external.id)).resolves.toMatchObject({
      kind: 'ready',
      path: '/Volumes/External 2/Music'
    });
    await commands.cancelLibraryFolderRelocation();
    await expect(commands.relocateLibraryFolder(external.id)).rejects.toMatchObject({
      code: 'NOT_FOUND'
    });
  });

  it('今の場所・ほかのライブラリフォルダ・転送先デバイスと重なる場所には付け替えない', async () => {
    const music = (await folderAt('/Users/demo/Music'))!;
    const external = (await folderAt('/Volumes/External/Music'))!;
    const cases = [
      { folder: music, path: '/Users/demo/Music/Sub', kind: 'overlapsCurrent' },
      { folder: music, path: '/Users/demo', kind: 'overlapsCurrent' },
      {
        folder: external,
        path: '/Users/demo/Music/Moved',
        kind: 'overlapsLibraryFolder',
        libraryFolderPath: '/Users/demo/Music'
      },
      {
        folder: external,
        path: '/Volumes/SDCARD',
        kind: 'overlapsDevice',
        deviceName: 'SDカード'
      }
    ];
    for (const { folder, path, ...expected } of cases) {
      backend.setRelocationFolder(path);
      await expect(commands.planLibraryFolderRelocation(folder.id)).resolves.toEqual({
        path,
        ...expected
      });
      // 付け替えられない場所は預からない
      await expect(commands.relocateLibraryFolder(folder.id)).rejects.toMatchObject({
        code: 'NOT_FOUND'
      });
    }
  });

  it('選んだ後に付け替えられなくなった場所には、付け替えない', async () => {
    const external = (await folderAt('/Volumes/External/Music'))!;
    await commands.planLibraryFolderRelocation(external.id);
    await importFolder('/Volumes/Backup');

    await expect(commands.relocateLibraryFolder(external.id)).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    expect((await folderAt('/Volumes/External/Music'))?.id).toBe(external.id);
  });
});

describe('音楽フォルダ', () => {
  const musicFolderPath = async () => (await commands.getMusicFolder()).folder?.path ?? null;
  const markedPaths = async () =>
    (await commands.getLibraryFolders()).folders.filter((f) => f.isMusicFolder).map((f) => f.path);

  it('はじめは設定しておらず、OSの音楽フォルダを候補に返す', async () => {
    expect(await commands.getMusicFolder()).toEqual({
      folder: null,
      systemFolder: '/Users/demo/Music'
    });
  });

  it('登録済みのライブラリフォルダを選ぶと、印を移すだけで読み込み直さない', async () => {
    const total = (await commands.getAllTracks()).length;

    await expect(commands.setMusicFolder('system')).resolves.toEqual({
      kind: 'set',
      path: '/Users/demo/Music',
      imported: null
    });
    expect(await markedPaths()).toEqual(['/Users/demo/Music']);
    expect(await commands.getAllTracks()).toHaveLength(total);
    expect(events.map((e) => e.event)).toContain('library-changed');

    backend.setFolderResult('/Volumes/External/Music');
    await commands.setMusicFolder('pick');
    expect(await markedPaths()).toEqual(['/Volumes/External/Music']);
  });

  it('新しいフォルダは、空でも登録し、曲があれば読み込む', async () => {
    backend.setMusicFolderFileCount(0);
    backend.setFolderResult('/Users/demo/Empty/');
    await expect(commands.setMusicFolder('pick')).resolves.toMatchObject({
      kind: 'set',
      path: '/Users/demo/Empty',
      imported: { importedCount: 0 }
    });
    expect(await musicFolderPath()).toBe('/Users/demo/Empty');

    backend.setMusicFolderFileCount(3);
    backend.setFolderResult('/Users/demo/New Music');
    await expect(commands.setMusicFolder('pick')).resolves.toMatchObject({
      imported: { importedCount: 3 }
    });
    expect(await markedPaths()).toEqual(['/Users/demo/New Music']);
    // 前の音楽フォルダは、ライブラリフォルダとして残る
    const paths = (await commands.getLibraryFolders()).folders.map((f) => f.path);
    expect(paths).toContain('/Users/demo/Empty');
  });

  it('ライブラリフォルダの中のフォルダは、音楽フォルダにせず外側のフォルダを返す', async () => {
    backend.setFolderResult('/Users/demo/Music/Muspice');
    await expect(commands.setMusicFolder('pick')).resolves.toEqual({
      kind: 'insideLibraryFolder',
      path: '/Users/demo/Music/Muspice',
      libraryFolderPath: '/Users/demo/Music'
    });
    expect(await musicFolderPath()).toBeNull();

    backend.setFolderResult(null);
    await expect(commands.setMusicFolder('pick')).resolves.toBeNull();
  });

  it('外側のフォルダをインポートすると、印は外側のフォルダへ移る', async () => {
    await commands.setMusicFolder('system');
    // 取り込む曲をコピーする設定（既定）では、参照するかコピーするかを選ぶ
    expect(await importFolder('/Users/demo')).toMatchObject({ kind: 'chooseFolderMode' });
    await commands.importPendingFolders('reference', 'Skip');
    expect(await markedPaths()).toEqual(['/Users/demo']);
  });

  it('解除・ライブラリフォルダの削除で、未設定に戻る', async () => {
    await commands.setMusicFolder('system');
    await commands.clearMusicFolder();
    expect(await musicFolderPath()).toBeNull();
    expect((await commands.getLibraryFolders()).folders.map((f) => f.path)).toContain(
      '/Users/demo/Music'
    );

    const folder = (await commands.setMusicFolder('system'))!;
    expect(folder.kind).toBe('set');
    const music = (await commands.getMusicFolder()).folder!;
    events = [];
    await commands.removeLibraryFolder(music.id, false);
    expect(await musicFolderPath()).toBeNull();
    // 曲を外さなくても、音楽フォルダの設定が変わったことを知らせる
    expect(events.map((e) => e.event)).toContain('library-changed');
  });

  it('書式の例は、最近追加したアルバムの最初の曲を書式に当てはめる', async () => {
    const examples = await commands.previewMusicFolderTemplate('%albumartist%/%album%/%title%');
    expect(examples.length).toBeGreaterThan(0);
    expect(examples.length).toBeLessThanOrEqual(5);
    for (const example of examples) expect(example.split('/')).toHaveLength(3);

    // 新しく取り込んだアルバムが、先頭に来る
    await importFolder('/Users/demo/Imports/Fresh');
    const [first] = await commands.previewMusicFolderTemplate(
      '%albumartist%/%album%/[%disc%-]%track% %title%'
    );
    expect(first).toBe('Mock Importer/Fresh/1-01 Mock Track 01.mp3');

    await expect(commands.previewMusicFolderTemplate('%unknown%')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
  });

  it('読めない書式は、設定に保存しない', async () => {
    const settings = await commands.getSettings();
    expect(settings.musicFolderTemplate).toBe('%albumartist%/%album%/[%disc%-]%track% %title%');
    await expect(
      commands.saveSettings({ ...settings, musicFolderTemplate: '%artist%/' })
    ).rejects.toMatchObject({ code: 'VALIDATION' });
    await commands.saveSettings({ ...settings, musicFolderTemplate: '%artist%/%title%' });
    expect((await commands.getSettings()).musicFolderTemplate).toBe('%artist%/%title%');
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

  /** フォルダを選ぶダイアログでフォルダを選び（nullは選ばずに閉じた）、デバイスとして登録する */
  const registerDevice = (path: string | null) => {
    backend.setFolderResult(path);
    return commands.registerSyncDevice();
  };

  it('選んだフォルダを、ドライブ・フォルダの名前で登録し、ライブラリフォルダと重なるフォルダは拒否する', async () => {
    const device = (await registerDevice('/Volumes/NEW_SD/'))!;
    expect(device).toMatchObject({
      name: 'NEW_SD',
      path: '/Volumes/NEW_SD',
      connected: true,
      syncAll: false,
      playlistIds: [],
      removeUnselected: true
    });
    expect((await commands.getSyncDevices()).map((d) => d.id)).toContain(device.id);
    expect((await registerDevice('/Users/demo/Backup/Music'))?.name).toBe('Music');

    await expect(registerDevice('/Users/demo/Music/Sub')).rejects.toMatchObject({
      code: 'VALIDATION'
    });
    await expect(registerDevice('/Users')).rejects.toMatchObject({ code: 'VALIDATION' });
  });

  it('フォルダを選ばずに閉じたら、nullを返して登録しない', async () => {
    const devices = await commands.getSyncDevices();

    expect(await registerDevice(null)).toBeNull();
    expect(await commands.getSyncDevices()).toEqual(devices);
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

    // 選ばずに閉じたら、何も変えない
    backend.setFolderResult(null);
    expect(await commands.relinkSyncDevice(disconnected.id)).toBeNull();
    expect((await commands.getSyncDevices()).find((d) => d.id === disconnected.id)).toEqual(
      disconnected
    );

    backend.setFolderResult('/Volumes/WALKMAN 1/MUSIC');
    const relinked = await commands.relinkSyncDevice(disconnected.id);
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
    // 再生速度は、0.5〜2倍
    for (const speed of [0.4, 2.1, Number.NaN]) {
      await expect(commands.playbackSetSpeed(speed)).rejects.toMatchObject({ code: 'VALIDATION' });
    }
    await expect(commands.playbackSetSpeed(1.5)).resolves.toBeNull();
    await expect(commands.playbackSetSpeed(1)).resolves.toBeNull();
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

  it('再生していた曲と後ろの曲がなくなっていたら、前に残っている最後の曲を指す', async () => {
    const trackIds = [mockTrackId(1), mockTrackId(2), mockTrackId(3)];
    await commands.savePlaybackState(
      { ...cursor, currentIndex: 1 },
      { trackIds, originalTrackIds: null }
    );
    await commands.deleteTracksCommand([mockTrackId(2), mockTrackId(3)]);

    const state = await commands.getPlaybackState();

    expect(state.queue.map((track) => track.id)).toEqual([mockTrackId(1)]);
    expect(state.currentIndex).toBe(0);
  });

  it('何も再生していなかったキューは、再生していた曲の位置をnullで返す', async () => {
    const trackIds = [mockTrackId(1), mockTrackId(2)];
    await commands.savePlaybackState(
      { ...cursor, currentIndex: null },
      { trackIds, originalTrackIds: null }
    );

    const state = await commands.getPlaybackState();

    expect(state.queue.map((track) => track.id)).toEqual(trackIds);
    expect(state.currentIndex).toBeNull();
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

describe('イベント', () => {
  it('名前と内容は、bindings.tsのeventsの型で拘束する', () => {
    const emit: MockBackendOptions['emit'] = () => {};
    // @ts-expect-error bindings.tsにないイベント
    emit('unknownEvent', null);
    // @ts-expect-error 内容の項目が足りない
    emit('importProgress', { phase: 'reading' });
    // @ts-expect-error 内容のないイベントに、内容を渡す
    emit('libraryChanged', { changed: true });
    emit('libraryChanged', null);
    emit('libraryScanProgress', { current: 1, total: 2, currentFile: 'a.mp3' });
  });

  it('IPCで送る名前は、bindings.tsのeventsが購読する名前と同じ', async () => {
    // 購読のコールバックのIDを作るため、`crypto`も置く
    vi.stubGlobal('window', { crypto: globalThis.crypto });
    const listened: string[] = [];
    mockIPC((cmd, payload) => {
      if (cmd === 'plugin:event|listen') listened.push((payload as { event: string }).event);
      return 1;
    });
    const names = Object.keys(MOCK_EVENT_NAMES) as MockEventName[];
    expect(names.sort()).toEqual(Object.keys(bindingEvents).sort());
    for (const name of names) await bindingEvents[name].listen(() => {});

    expect(listened).toEqual(names.map((name) => MOCK_EVENT_NAMES[name]));
  });

  it('再スキャン・場所の付け替えは、読み込みの進み具合を送る', async () => {
    const { folders } = await commands.getLibraryFolders();
    const folder = folders.find((candidate) => candidate.exists && candidate.trackCount > 0)!;
    const progress = () =>
      events.filter((e) => e.event === 'library-scan-progress').map((e) => e.payload);

    await commands.rescanLibraryFolder(folder.id);

    const sent = progress() as { current: number; total: number; currentFile: string }[];
    expect(sent.length).toBeGreaterThan(0);
    expect(sent.map((p) => p.current)).toEqual(sent.map((_, index) => index + 1));
    expect(sent.every((p) => p.total === sent.length && p.currentFile !== '')).toBe(true);
  });
});
