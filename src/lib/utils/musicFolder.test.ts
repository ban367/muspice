import { describe, expect, it } from 'vitest';
import type { ImportResult } from '#lib/types/models.js';
import { musicFolderNotice } from './musicFolder.js';

const imported = (counts: Partial<ImportResult>): ImportResult => ({
  importedCount: 0,
  skippedCount: 0,
  relinkedCount: 0,
  errorCount: 0,
  errors: [],
  copiedCount: 0,
  notCopied: [],
  ...counts
});

describe('musicFolderNotice', () => {
  it('空のフォルダ・印を移しただけの場合は、音楽フォルダにしたことだけを知らせる', () => {
    for (const result of [null, imported({ skippedCount: 3 })]) {
      expect(musicFolderNotice({ kind: 'set', path: '/Users/a/Music', imported: result })).toEqual({
        kind: 'success',
        message: '音楽フォルダを「/Users/a/Music」にしました'
      });
    }
  });

  it('読み込んだ曲の数を知らせる', () => {
    expect(
      musicFolderNotice({
        kind: 'set',
        path: '/Users/a/Music',
        imported: imported({ importedCount: 12, errorCount: 2, errors: ['a', 'b'] })
      })
    ).toEqual({
      kind: 'success',
      message: '音楽フォルダを「/Users/a/Music」にし、12曲を読み込みました'
    });
  });

  it('ライブラリフォルダの中のフォルダを選んだ場合は、外側のフォルダを案内する', () => {
    const notice = musicFolderNotice({
      kind: 'insideLibraryFolder',
      path: '/Users/a/Music/Muspice',
      libraryFolderPath: '/Users/a/Music'
    });
    expect(notice.kind).toBe('warning');
    expect(notice.message).toContain('外側のフォルダ（/Users/a/Music）を選んでください');
  });
});
