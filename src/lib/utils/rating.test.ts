import { describe, expect, it } from 'vitest';
import {
  MAX_RATING,
  RATING_VALUES,
  formatRatingStars,
  ratingAt,
  ratingToStars,
  starFill
} from './rating.js';

describe('評価（半分の星の単位）', () => {
  it('選べる評価は、星0.5から星5まで', () => {
    expect(RATING_VALUES).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(MAX_RATING).toBe(10);
    expect(ratingToStars(7)).toBe(3.5);
  });

  it.each([
    [0, ''],
    [1, '½'],
    [2, '★'],
    [7, '★★★½'],
    [8, '★★★★'],
    [10, '★★★★★']
  ])('評価%iを星の文字にする', (rating, expected) => {
    expect(formatRatingStars(rating)).toBe(expected);
  });

  it('星ごとの塗り方', () => {
    // 星3.5: 1〜3番目は塗り、4番目は半分、5番目は塗らない
    expect([1, 2, 3, 4, 5].map((star) => starFill(star, 7))).toEqual([
      'full',
      'full',
      'full',
      'half',
      'empty'
    ]);
    expect([1, 2, 3, 4, 5].map((star) => starFill(star, 0))).toEqual(Array(5).fill('empty'));
    expect(starFill(5, 10)).toBe('full');
  });

  it('星の左半分は半分の星、右半分はその星までを選ぶ', () => {
    expect(ratingAt(4, 0.2)).toBe(7);
    expect(ratingAt(4, 0.49)).toBe(7);
    expect(ratingAt(4, 0.5)).toBe(8);
    expect(ratingAt(1, 0)).toBe(1);
    expect(ratingAt(5, 1)).toBe(10);
  });
});
