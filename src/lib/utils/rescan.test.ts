import { describe, expect, it } from 'vitest';
import type { RescanResult } from '#lib/types/models.js';
import { describeRescan, sumRescanResults } from './rescan';

function result(overrides: Partial<RescanResult> = {}): RescanResult {
  return {
    addedCount: 0,
    updatedCount: 0,
    removedCount: 0,
    errorCount: 0,
    errors: [],
    removalSkipped: false,
    ...overrides
  };
}

describe('sumRescanResults', () => {
  it('件数を合計し、曲を外さなかったフォルダを集める', () => {
    const totals = sumRescanResults([
      { path: '/a', result: result({ addedCount: 2, errorCount: 1 }) },
      { path: '/b', result: result({ updatedCount: 3, removalSkipped: true }) },
      { path: '/c', result: result({ addedCount: 1, removedCount: 4 }) }
    ]);

    expect(totals).toEqual({
      addedCount: 3,
      updatedCount: 3,
      removedCount: 4,
      errorCount: 1,
      removalSkippedPaths: ['/b']
    });
  });
});

describe('describeRescan', () => {
  it('変更のあった種類だけを並べる', () => {
    const totals = sumRescanResults([
      { path: '/a', result: result({ addedCount: 2, removedCount: 1 }) }
    ]);
    expect(describeRescan(totals)).toBe('再スキャンしました（追加 2曲・削除 1曲）');
  });

  it('変更がなければその旨を伝える', () => {
    expect(describeRescan(sumRescanResults([{ path: '/a', result: result() }]))).toBe(
      '再スキャンしました（変更はありませんでした）'
    );
  });
});
