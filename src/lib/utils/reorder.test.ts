import { describe, expect, it } from 'vitest';
import { moveItems } from './reorder.js';

describe('moveItems', () => {
  const order = ['a', 'b', 'c', 'd', 'e'];

  it('下へ動かすと、落とした項目の後ろに入る', () => {
    expect(moveItems(order, ['a'], 'c')).toEqual(['b', 'c', 'a', 'd', 'e']);
    expect(moveItems(order, ['a'], 'e')).toEqual(['b', 'c', 'd', 'e', 'a']);
  });

  it('上へ動かすと、落とした項目の前に入る', () => {
    expect(moveItems(order, ['d'], 'b')).toEqual(['a', 'd', 'b', 'c', 'e']);
    expect(moveItems(order, ['e'], 'a')).toEqual(['e', 'a', 'b', 'c', 'd']);
  });

  it('複数の項目は、元の並びの順のまま、まとめて動かす', () => {
    // 選んだ順（d → b）ではなく、一覧の順（b → d）で入る
    expect(moveItems(order, ['d', 'b'], 'e')).toEqual(['a', 'c', 'e', 'b', 'd']);
    expect(moveItems(order, ['c', 'e'], 'a')).toEqual(['c', 'e', 'a', 'b', 'd']);
    // 離れた項目を、間の項目へ落とす（最初の項目が上にあるため、後ろに入る）
    expect(moveItems(order, ['a', 'e'], 'c')).toEqual(['b', 'c', 'a', 'e', 'd']);
  });

  it('落とした項目が運んだ項目に含まれる・一覧にない場合は、何も変えない', () => {
    expect(moveItems(order, ['b', 'c'], 'c')).toEqual(order);
    expect(moveItems(order, ['a'], 'z')).toEqual(order);
    expect(moveItems(order, ['z'], 'a')).toEqual(order);
    expect(moveItems(order, [], 'a')).toEqual(order);
  });

  it('元の配列は変えない', () => {
    const copy = [...order];
    const moved = moveItems(order, ['a'], 'b');

    expect(order).toEqual(copy);
    expect(moved).not.toBe(order);
    expect(moveItems(order, ['a'], 'a')).not.toBe(order);
  });
});
