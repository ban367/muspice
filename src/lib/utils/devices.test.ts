import { describe, expect, it } from 'vitest';
import {
  isDeviceNameTooLong,
  isDeviceUpToDate,
  missingSpaceBytes,
  suggestDeviceName,
  syncProgressPercent
} from './devices';

describe('suggestDeviceName', () => {
  it('ボリュームのマウント先ではボリューム名を使う', () => {
    expect(suggestDeviceName('/Volumes/SDCARD')).toBe('SDCARD');
    expect(suggestDeviceName('/Volumes/SDCARD/Music')).toBe('SDCARD');
    expect(suggestDeviceName('/media/demo/WALKMAN/MUSIC')).toBe('WALKMAN');
    expect(suggestDeviceName('/run/media/demo/WALKMAN')).toBe('WALKMAN');
  });

  it('それ以外はフォルダ名を使う', () => {
    expect(suggestDeviceName('/Users/demo/Backup/')).toBe('Backup');
    expect(suggestDeviceName('E:\\Music')).toBe('Music');
  });

  it('ルートでは候補を作らない', () => {
    expect(suggestDeviceName('E:\\')).toBe('');
    expect(suggestDeviceName('/')).toBe('');
  });
});

describe('isDeviceNameTooLong', () => {
  it('UTF-8のバイト数で数える', () => {
    expect(isDeviceNameTooLong('a'.repeat(100))).toBe(false);
    expect(isDeviceNameTooLong('a'.repeat(101))).toBe(true);
    expect(isDeviceNameTooLong('あ'.repeat(33))).toBe(false);
    expect(isDeviceNameTooLong('あ'.repeat(34))).toBe(true);
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
  it('コピー・削除・名前の変更がなければtrue', () => {
    expect(isDeviceUpToDate({ copyCount: 0, deleteCount: 0, renameCount: 0 })).toBe(true);
    expect(isDeviceUpToDate({ copyCount: 1, deleteCount: 0, renameCount: 0 })).toBe(false);
    expect(isDeviceUpToDate({ copyCount: 0, deleteCount: 2, renameCount: 0 })).toBe(false);
    expect(isDeviceUpToDate({ copyCount: 0, deleteCount: 0, renameCount: 3 })).toBe(false);
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
