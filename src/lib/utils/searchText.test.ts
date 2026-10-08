import { describe, expect, it } from 'vitest';
import { matchesSearchTerms, normalizeSearchText, searchTerms } from './searchText.js';

describe('normalizeSearchText', () => {
  it('大文字と小文字・全角と半角・ひらがなとカタカナをそろえる', () => {
    expect(normalizeSearchText('The Beatles')).toBe('the beatles');
    expect(normalizeSearchText('ａｉｋｏ　２０２４')).toBe('aiko 2024');
    expect(normalizeSearchText('ｶﾌﾞﾄﾑｼ')).toBe(normalizeSearchText('カブトムシ'));
    expect(normalizeSearchText('宇多田ヒカル')).toBe('宇多田ひかる');
    expect(normalizeSearchText('スーパーカー')).toBe('すーぱーかー');
    expect(normalizeSearchText('ヴァイオリン')).toBe('ゔぁいおりん');
    expect(normalizeSearchText('ひかる')).toBe('ひかる');
  });
});

describe('searchTerms', () => {
  it('空白（全角を含む）で区切り、空の語は除く', () => {
    expect(searchTerms('Beatles  HELP')).toEqual(['beatles', 'help']);
    expect(searchTerms('宇多田　ヒカル')).toEqual(['宇多田', 'ひかる']);
    expect(searchTerms('  　 ')).toEqual([]);
  });
});

describe('matchesSearchTerms', () => {
  const fields = ['First Love', '宇多田ヒカル', null];

  it('語の途中に含まれていれば一致する', () => {
    expect(matchesSearchTerms(fields, searchTerms('ヒカル'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('ひかる'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('ﾋｶﾙ'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('ＦＩＲＳＴ'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('宇'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('second'))).toBe(false);
  });

  it('すべての語を含む場合だけ一致する（項目が違ってもよい）', () => {
    expect(matchesSearchTerms(fields, searchTerms('love 宇多田'))).toBe(true);
    expect(matchesSearchTerms(fields, searchTerms('love 椎名'))).toBe(false);
  });

  it('1つの語が項目をまたぐ一致は含めない', () => {
    expect(matchesSearchTerms(fields, searchTerms('love宇多田'))).toBe(false);
  });

  it('語がなければ一致する', () => {
    expect(matchesSearchTerms(fields, [])).toBe(true);
  });
});
