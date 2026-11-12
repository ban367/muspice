import { describe, expect, it } from 'vitest';
import type { ImportResult } from '#lib/types/models.js';
import { isOutsideMusicFolder, musicFolderMessage } from './musicFolder.js';

describe('isOutsideMusicFolder', () => {
  it('音楽フォルダの外の曲か（パスの要素の単位で比べる）', () => {
    expect(isOutsideMusicFolder('/Music/Muspice/A/01.mp3', '/Music/Muspice')).toBe(false);
    expect(isOutsideMusicFolder('/Music/MuspiceOld/01.mp3', '/Music/Muspice')).toBe(true);
    expect(isOutsideMusicFolder('/Users/me/Downloads/01.mp3', '/Music/Muspice')).toBe(true);
    // 音楽フォルダが未設定なら、外の曲とはしない
    expect(isOutsideMusicFolder('/Users/me/Downloads/01.mp3', null)).toBe(false);
  });
});

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

describe('musicFolderMessage', () => {
  const set = (result: ImportResult | null) =>
    ({ kind: 'set', path: '/Users/a/Music', imported: result, replaced: null }) as const;

  it('空のフォルダ・印を付けただけの場合は、音楽フォルダにしたことだけを知らせる', () => {
    for (const result of [null, imported({ skippedCount: 3 })]) {
      expect(musicFolderMessage(set(result))).toBe('音楽フォルダを「/Users/a/Music」にしました');
    }
  });

  it('読み込んだ曲の数を知らせる', () => {
    expect(
      musicFolderMessage(set(imported({ importedCount: 12, errorCount: 2, errors: ['a', 'b'] })))
    ).toBe('音楽フォルダを「/Users/a/Music」にし、12曲を読み込みました');
  });

  it('ライブラリを置き換えた場合は、外した曲と消したプレイリストの数を知らせる', () => {
    expect(
      musicFolderMessage({
        ...set(imported({ importedCount: 5 })),
        replaced: { removedTrackCount: 120, removedPlaylistCount: 2, backupPath: '/b/x' }
      })
    ).toBe(
      '音楽フォルダを「/Users/a/Music」にし、ライブラリを置き換えました（120曲を外し、プレイリスト2件を消しました）'
    );
  });
});
