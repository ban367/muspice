import { describe, expect, it } from 'vitest';
import { computeVirtualRange, countColumns, countRows, scrollTopToReveal } from './virtualList.js';

describe('countRows', () => {
  it('最後の行が途中までの場合も1行に数える', () => {
    expect(countRows(10, 1)).toBe(10);
    expect(countRows(10, 4)).toBe(3);
    expect(countRows(8, 4)).toBe(2);
    expect(countRows(0, 4)).toBe(0);
  });
});

describe('computeVirtualRange（リスト）', () => {
  const base = { count: 1000, columns: 1, rowStride: 40, viewportHeight: 400, overscan: 2 };

  it('先頭では、見えている行と後ろの余分な行を返す', () => {
    // 見えているのは0〜10行目（400pxの位置にかかる行を含む）
    expect(computeVirtualRange({ ...base, scrollTop: 0 })).toEqual({
      start: 0,
      end: 13,
      offsetTop: 0
    });
  });

  it('途中では、見えている行の前後に余分な行を加える', () => {
    // 見えているのは50〜60行目
    expect(computeVirtualRange({ ...base, scrollTop: 2000 })).toEqual({
      start: 48,
      end: 63,
      offsetTop: 48 * 40
    });
  });

  it('行の途中までスクロールした位置では、欠けて見えている行も含める', () => {
    const range = computeVirtualRange({ ...base, scrollTop: 2010, overscan: 0 });
    expect(range.start).toBe(50);
    expect(range.end).toBe(61);
  });

  it('末尾では、項目の数を越えない', () => {
    const range = computeVirtualRange({ ...base, scrollTop: 1000 * 40 - 400 });
    expect(range.end).toBe(1000);
    expect(range.start).toBe(988);
  });

  it('末尾を越えたスクロール位置でも、最後の行を返す', () => {
    const range = computeVirtualRange({ ...base, count: 5, scrollTop: 10_000 });
    expect(range).toEqual({ start: 2, end: 5, offsetTop: 80 });
  });

  it('項目がない・行の高さが分からない場合は、何も描画しない', () => {
    expect(computeVirtualRange({ ...base, count: 0, scrollTop: 0 })).toEqual({
      start: 0,
      end: 0,
      offsetTop: 0
    });
    expect(computeVirtualRange({ ...base, rowStride: 0, scrollTop: 0 }).end).toBe(0);
  });

  it('表示範囲の高さがまだ分からない場合も、先頭の行は返す', () => {
    const range = computeVirtualRange({ ...base, viewportHeight: 0, scrollTop: 0, overscan: 0 });
    expect(range).toEqual({ start: 0, end: 1, offsetTop: 0 });
  });
});

describe('computeVirtualRange（グリッド）', () => {
  const base = { count: 103, columns: 4, rowStride: 200, viewportHeight: 500, overscan: 1 };

  it('行の単位で範囲を返す', () => {
    // 見えているのは2〜4行目。前後1行を加えて1〜5行目（項目4〜23）
    expect(computeVirtualRange({ ...base, scrollTop: 400 })).toEqual({
      start: 4,
      end: 24,
      offsetTop: 200
    });
  });

  it('途中までの最後の行は、ある項目だけを返す', () => {
    // 26行（最後の行は3項目）
    const range = computeVirtualRange({ ...base, scrollTop: 26 * 200 - 500 });
    expect(range.end).toBe(103);
  });
});

describe('scrollTopToReveal', () => {
  const base = { columns: 1, rowStride: 40, gap: 0, viewportHeight: 400 };

  it('見えている項目では、スクロール位置を変えない', () => {
    expect(scrollTopToReveal({ ...base, index: 5, scrollTop: 100 })).toBe(100);
  });

  it('上に隠れている項目は、上端にそろえる', () => {
    expect(scrollTopToReveal({ ...base, index: 2, scrollTop: 100 })).toBe(80);
  });

  it('下に隠れている項目は、下端にそろえる', () => {
    // 20行目は800〜840px。下端を840pxにする
    expect(scrollTopToReveal({ ...base, index: 20, scrollTop: 100 })).toBe(440);
  });

  it('グリッドでは、項目のある行を見える位置へ出す（行の間隔は含めない）', () => {
    const grid = { columns: 4, rowStride: 212, gap: 12, viewportHeight: 500 };
    // 項目9は2行目（424〜624px）
    expect(scrollTopToReveal({ ...grid, index: 9, scrollTop: 0 })).toBe(124);
    expect(scrollTopToReveal({ ...grid, index: 9, scrollTop: 600 })).toBe(424);
  });
});

describe('countColumns', () => {
  it('幅に入るだけの列を返す（列の間隔を含めて数える）', () => {
    // 100pxの列と12pxの間隔: 3列で324px、4列で436px
    expect(countColumns(435, 100, 12)).toBe(3);
    expect(countColumns(436, 100, 12)).toBe(4);
  });

  it('幅が足りない・分からない場合も1列にする', () => {
    expect(countColumns(50, 100, 12)).toBe(1);
    expect(countColumns(0, 100, 12)).toBe(1);
    expect(countColumns(500, 0, 12)).toBe(1);
  });
});
