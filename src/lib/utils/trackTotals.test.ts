import { describe, expect, it } from 'vitest';
import { trackTotals } from './trackTotals.js';

describe('trackTotals', () => {
  it('曲の数・長さ・サイズを合計し、長さの分からない曲は0とする', () => {
    expect(
      trackTotals([
        { duration: 180, fileSize: 1000 },
        { duration: null, fileSize: 500 },
        { duration: 60, fileSize: 0 }
      ])
    ).toEqual({ count: 3, duration: 240, size: 1500 });
  });

  it('曲がなければ、どれも0', () => {
    expect(trackTotals([])).toEqual({ count: 0, duration: 0, size: 0 });
  });
});
