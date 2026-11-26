import { describe, expect, it } from 'vitest';
import type { SyncDevice } from '#lib/types/models.js';
import {
  deviceConfigOf,
  isDeviceNameTooLong,
  isDeviceUpToDate,
  missingSpaceBytes,
  syncProgressPercent
} from './devices';

describe('deviceConfigOf', () => {
  const device: SyncDevice = {
    id: 'd1',
    name: 'SD',
    path: '/Volumes/SD',
    syncAll: false,
    playlistIds: ['p1'],
    smartPlaylistIds: ['s1'],
    removeUnselected: true,
    pathTemplate: '%artist%/%title%',
    copyExtras: false,
    transcode: { mode: 'lossless', bitrateKbps: 256 },
    ejectable: true,
    connected: true,
    freeBytes: 1,
    totalBytes: 2,
    createdAt: '2026-01-01T00:00:00Z',
    lastSyncedAt: null
  };

  it('設定の項目だけを取り出し、変える項目を重ねる', () => {
    expect(deviceConfigOf(device)).toEqual({
      name: 'SD',
      syncAll: false,
      playlistIds: ['p1'],
      smartPlaylistIds: ['s1'],
      removeUnselected: true,
      pathTemplate: '%artist%/%title%',
      copyExtras: false,
      transcode: { mode: 'lossless', bitrateKbps: 256 }
    });
    expect(deviceConfigOf(device, { name: 'Walkman' })).toMatchObject({
      name: 'Walkman',
      playlistIds: ['p1']
    });
  });
});

describe('isDeviceNameTooLong', () => {
  it('文字数で数える（バイト数・UTF-16の長さではない）', () => {
    expect(isDeviceNameTooLong('a'.repeat(100))).toBe(false);
    expect(isDeviceNameTooLong('a'.repeat(101))).toBe(true);
    expect(isDeviceNameTooLong('あ'.repeat(100))).toBe(false);
    expect(isDeviceNameTooLong('あ'.repeat(101))).toBe(true);
    expect(isDeviceNameTooLong('🎵'.repeat(100))).toBe(false);
  });

  it('前後の空白は数えない', () => {
    expect(isDeviceNameTooLong(`  ${'a'.repeat(100)}  `)).toBe(false);
  });
});

describe('syncProgressPercent', () => {
  it('バイト数で割合を出す', () => {
    expect(syncProgressPercent({ current: 0, total: 4, bytesDone: 250, bytesTotal: 1000 })).toBe(
      25
    );
  });

  it('サイズが分からなければファイル数で出す', () => {
    expect(syncProgressPercent({ current: 1, total: 4, bytesDone: 0, bytesTotal: 0 })).toBe(25);
  });

  it('コピーするファイルがなければ100にし、範囲を超えない', () => {
    expect(syncProgressPercent({ current: 0, total: 0, bytesDone: 0, bytesTotal: 0 })).toBe(100);
    expect(syncProgressPercent({ current: 1, total: 1, bytesDone: 1200, bytesTotal: 1000 })).toBe(
      100
    );
  });
});

describe('isDeviceUpToDate', () => {
  const none = {
    copyCount: 0,
    deleteCount: 0,
    renameCount: 0,
    extraCopyCount: 0,
    extraDeleteCount: 0
  };

  it('コピー・削除・名前の変更（曲に添えるファイルを含む）がなければtrue', () => {
    expect(isDeviceUpToDate(none)).toBe(true);
    expect(isDeviceUpToDate({ ...none, copyCount: 1 })).toBe(false);
    expect(isDeviceUpToDate({ ...none, deleteCount: 2 })).toBe(false);
    expect(isDeviceUpToDate({ ...none, renameCount: 3 })).toBe(false);
    // 歌詞ファイル・フォルダの画像だけをコピー・削除する時も、最新ではない
    expect(isDeviceUpToDate({ ...none, extraCopyCount: 1 })).toBe(false);
    expect(isDeviceUpToDate({ ...none, extraDeleteCount: 1 })).toBe(false);
  });
});

describe('missingSpaceBytes', () => {
  it('足りない場合に、あと必要な容量を返す', () => {
    expect(missingSpaceBytes({ requiredBytes: 500, freeBytes: 200, hasEnoughSpace: false })).toBe(
      300
    );
  });

  it('足りていれば0', () => {
    expect(missingSpaceBytes({ requiredBytes: 500, freeBytes: 900, hasEnoughSpace: true })).toBe(0);
  });

  it('残しておく容量の分だけ足りない場合も、負の値にしない', () => {
    expect(missingSpaceBytes({ requiredBytes: 500, freeBytes: 500, hasEnoughSpace: false })).toBe(
      0
    );
  });
});
