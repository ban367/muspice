/**
 * 評価（半分の星）の扱い
 *
 * 評価は、半分の星の単位の整数で持つ（0は評価なし、7は星3.5、10は星5つ。ADR-082）。
 * ファイルのタグには、MusicBee・MediaMonkeyと同じ値で書く（Rust側の`rating_tag`）。
 */

/** 星5つの評価（半分の星の単位） */
export const MAX_RATING = 10;

/** 選べる評価（評価なしを除く。星0.5から星5まで） */
export const RATING_VALUES: readonly number[] = Array.from(
  { length: MAX_RATING },
  (_, index) => index + 1
);

/** 評価を、星の数（3.5のような小数）にする */
export function ratingToStars(rating: number): number {
  return rating / 2;
}

/** 評価を、星の文字にする（星3.5は`★★★½`。評価なしは空の文字列） */
export function formatRatingStars(rating: number): string {
  return '★'.repeat(Math.floor(rating / 2)) + (rating % 2 === 1 ? '½' : '');
}

/** 星（1〜5番目）の塗り方 */
export type StarFill = 'full' | 'half' | 'empty';

/** 評価のとき、`star`番目（1〜5）の星をどう塗るか */
export function starFill(star: number, rating: number): StarFill {
  if (rating >= star * 2) return 'full';
  if (rating === star * 2 - 1) return 'half';
  return 'empty';
}

/**
 * 星の上の位置から、選ぶ評価を求める（星の左半分は半分の星、右半分はその星まで）
 *
 * @param offsetRatio 星の左端からの位置（星の幅に対する割合。0〜1）
 */
export function ratingAt(star: number, offsetRatio: number): number {
  return offsetRatio < 0.5 ? star * 2 - 1 : star * 2;
}
