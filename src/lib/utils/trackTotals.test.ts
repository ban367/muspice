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
  it('CUEシートの曲は、同じファイルの大きさを1回だけ数える', () => {
    const cue = (start: number) => ({ source: '/Music/Album.flac', start, end: null });
    expect(
      trackTotals([
        { duration: 100, fileSize: 1000, cue: cue(0) },
        { duration: 200, fileSize: 1000, cue: cue(7500) },
        { duration: 50, fileSize: 30, cue: null }
      ])
    ).toEqual({ count: 3, duration: 350, size: 1030 });
  });
});
