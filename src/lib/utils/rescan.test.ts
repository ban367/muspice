import { describe, expect, it } from 'vitest';
import type { RescanResult } from '#lib/types/models.js';
import { describeRescan, sumRescanResults } from './rescan';

function result(overrides: Partial<RescanResult> = {}): RescanResult {
  return {
    addedCount: 0,
    updatedCount: 0,
    relinkedCount: 0,
    missingCount: 0,
    restoredCount: 0,
    errorCount: 0,
    errors: [],
    missingSkipped: false,
    ...overrides
  };
}

describe('sumRescanResults', () => {
  it('件数を合計し、見つからない曲にしなかったフォルダを集める', () => {
    const totals = sumRescanResults([
      { path: '/a', result: result({ addedCount: 2, errorCount: 1, relinkedCount: 5 }) },
      { path: '/b', result: result({ updatedCount: 3, missingSkipped: true }) },
      { path: '/c', result: result({ addedCount: 1, missingCount: 4, restoredCount: 2 }) }
    ]);

    expect(totals).toEqual({
      addedCount: 3,
      updatedCount: 3,
      relinkedCount: 5,
      missingCount: 4,
      restoredCount: 2,
      errorCount: 1,
      missingSkippedPaths: ['/b']
    });
  });
});

describe('describeRescan', () => {
  it('変更のあった種類だけを並べる', () => {
    const totals = sumRescanResults([
      { path: '/a', result: result({ addedCount: 2, missingCount: 1 }) }
    ]);
    expect(describeRescan(totals)).toBe('再スキャンしました（追加 2曲・見つからない 1曲）');
  });

  it('移動したファイルの引き継ぎと、見つかった曲を伝える', () => {
    const totals = sumRescanResults([
      { path: '/a', result: result({ relinkedCount: 3, restoredCount: 1 }) }
    ]);
    expect(describeRescan(totals)).toBe('再スキャンしました（移動 3曲・見つかった 1曲）');
  });

  it('変更がなければその旨を伝える', () => {
    expect(describeRescan(sumRescanResults([{ path: '/a', result: result() }]))).toBe(
      '再スキャンしました（変更はありませんでした）'
    );
  });
});
