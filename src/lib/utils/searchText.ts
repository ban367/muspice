/**
 * 検索用の文字列の正規化と照合
 *
 * 検索では、大文字と小文字・全角と半角・ひらがなとカタカナの違いを同じとみなす。
 * 曲の検索はバックエンド（Rustの`search_text.rs`）が行い、ここの関数は同じ規則で
 * 一覧の絞り込み（アルバム・アーティスト・ジャンル）とモックの検索を行う。
 */

/** ひらがなとカタカナの文字コードの差 */
const KANA_OFFSET = 0x60;

/**
 * 文字列を検索用に正規化する
 *
 * 互換の文字をそろえ（NFKC。全角の英数字・半角のカタカナなど）、小文字にし、カタカナを
 * ひらがなにする。
 */
export function normalizeSearchText(text: string): string {
  return (
    text
      .normalize('NFKC')
      .toLowerCase()
      // ァ〜ヶ → ぁ〜ゖ、ヽヾ → ゝゞ
      .replace(/[ァ-ヶヽヾ]/g, (char) => String.fromCharCode(char.charCodeAt(0) - KANA_OFFSET))
  );
}

/**
 * 検索語を、空白で区切った語（正規化済み）にする
 */
export function searchTerms(query: string): string[] {
  return normalizeSearchText(query)
    .split(/\s+/)
    .filter((term) => term !== '');
}

/**
 * すべての語を、項目のどれかに含むか（語の途中の一致を含む）
 *
 * 1つの語が複数の項目にまたがる一致は含めない。
 * @param fields - 検索の対象の項目（値のないものはnull）
 * @param terms - `searchTerms`で求めた語
 */
export function matchesSearchTerms(fields: readonly (string | null)[], terms: readonly string[]) {
  const normalized = fields
    .filter((field): field is string => field !== null)
    .map(normalizeSearchText);
  return terms.every((term) => normalized.some((field) => field.includes(term)));
}
