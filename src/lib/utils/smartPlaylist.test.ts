import { describe, expect, it } from 'vitest';
import type { SmartRule, SmartRules } from '#lib/types/models.js';
import {
  KEY_VALUES,
  changeRuleField,
  describeOrder,
  describeRule,
  initialOrder,
  needsValue,
  newRuleDraft,
  newSmartRulesForm,
  opsFor,
  toRuleDrafts,
  toSmartRules,
  toSmartRulesForm,
  type RuleDraft,
  type SmartRulesForm
} from './smartPlaylist.js';

function draft(overrides: Partial<RuleDraft>): RuleDraft {
  return { key: 0, field: 'artist', op: 'contains', value: '', valueTo: '', ...overrides };
}

function form(overrides: Partial<SmartRulesForm> = {}): SmartRulesForm {
  return {
    matchMode: 'all',
    drafts: [],
    isLimited: false,
    limit: '25',
    order: { field: 'artist', descending: false },
    ...overrides
  };
}

describe('opsFor', () => {
  it('項目の種類に合う比べ方を返す', () => {
    expect(opsFor('artist')).toContain('contains');
    expect(opsFor('year')).toEqual(['is', 'isNot', 'atLeast', 'atMost', 'between']);
    expect(opsFor('favorite')).toEqual(['is', 'isNot']);
    expect(opsFor('inMusicFolder')).toEqual(['is', 'isNot']);
    expect(opsFor('lastPlayedAt')).toEqual(['inLast', 'notInLast', 'isEmpty', 'isNotEmpty']);
  });

  it('追加日には「なし・あり」を出さない', () => {
    expect(opsFor('createdAt')).toEqual(['inLast', 'notInLast']);
  });
});

describe('newRuleDraft・changeRuleField', () => {
  it('数値は「以上」、評価は星4から始める', () => {
    expect(newRuleDraft(3, 'rating')).toEqual({
      key: 3,
      field: 'rating',
      op: 'atLeast',
      // 評価は半分の星の単位（星4は8）
      value: '8',
      valueTo: ''
    });
    expect(newRuleDraft(0, 'lastPlayedAt').op).toBe('inLast');
    expect(newRuleDraft(0).op).toBe('contains');
  });

  it('同じ種類の項目へ変える場合は、比べ方と値を残す', () => {
    const before = draft({ key: 2, field: 'artist', op: 'startsWith', value: 'The' });
    expect(changeRuleField(before, 'album')).toEqual({ ...before, field: 'album' });
  });

  it('種類が違う項目へ変える場合は、比べ方と値を作り直す', () => {
    const before = draft({ key: 2, field: 'artist', op: 'startsWith', value: 'The' });
    expect(changeRuleField(before, 'year')).toEqual(newRuleDraft(2, 'year'));
  });

  it('入力の形が違う数値の項目へ変える場合は、値を作り直す', () => {
    const before = draft({ field: 'year', op: 'between', value: '1990', valueTo: '1999' });
    expect(changeRuleField(before, 'rating')).toMatchObject({ op: 'between', value: '8' });
  });

  it('選べない比べ方は、最初の比べ方にする', () => {
    const before = draft({ field: 'lastPlayedAt', op: 'isEmpty' });
    expect(changeRuleField(before, 'createdAt').op).toBe('inLast');
  });
});

describe('needsValue', () => {
  it('空かどうか・お気に入りかどうかは、値がいらない', () => {
    expect(needsValue({ field: 'genre', op: 'isEmpty' })).toBe(false);
    expect(needsValue({ field: 'lastPlayedAt', op: 'isNotEmpty' })).toBe(false);
    expect(needsValue({ field: 'favorite', op: 'is' })).toBe(false);
    expect(needsValue({ field: 'inMusicFolder', op: 'isNot' })).toBe(false);
    expect(needsValue({ field: 'genre', op: 'is' })).toBe(true);
  });
});

describe('toSmartRules', () => {
  it('入力を、保存する条件にする', () => {
    const result = toSmartRules(
      form({
        matchMode: 'any',
        drafts: [
          draft({ key: 0, field: 'genre', op: 'is', value: '  Jazz ' }),
          draft({ key: 1, field: 'year', op: 'between', value: '1990', valueTo: '1999' }),
          draft({ key: 2, field: 'duration', op: 'atLeast', value: '7.5' }),
          draft({ key: 3, field: 'lastPlayedAt', op: 'notInLast', value: '90' }),
          draft({ key: 4, field: 'lastPlayedAt', op: 'isEmpty', value: '90' }),
          draft({ key: 5, field: 'favorite', op: 'isNot' }),
          draft({ key: 6, field: 'genre', op: 'isEmpty', value: '残っていた値' })
        ],
        isLimited: true,
        limit: '50',
        order: { field: 'playCount', descending: true }
      })
    );

    expect(result).toEqual({
      error: null,
      rules: {
        matchMode: 'any',
        rules: [
          { kind: 'text', field: 'genre', op: 'is', value: 'Jazz' },
          { kind: 'number', field: 'year', op: 'between', value: 1990, valueTo: 1999 },
          // 長さは分で入力し、秒で保存する
          { kind: 'number', field: 'duration', op: 'atLeast', value: 450, valueTo: null },
          { kind: 'date', field: 'lastPlayedAt', op: 'notInLast', days: 90 },
          { kind: 'date', field: 'lastPlayedAt', op: 'isEmpty', days: 0 },
          { kind: 'favorite', value: false },
          // 値のいらない比べ方では、残っていた値を保存しない
          { kind: 'text', field: 'genre', op: 'isEmpty', value: '' }
        ],
        order: { field: 'playCount', descending: true },
        limit: 50
      }
    } satisfies { error: null; rules: SmartRules });
  });

  it('条件がない場合は、すべての曲を対象にする', () => {
    const result = toSmartRules(form({ matchMode: 'any' }));
    expect(result.rules).toMatchObject({ matchMode: 'all', rules: [], limit: null });
  });

  it('ランダムな並びには、向きを付けない', () => {
    const result = toSmartRules(form({ order: { field: 'random', descending: true } }));
    expect(result.rules?.order).toEqual({ field: 'random', descending: false });
  });

  it.each<[string, Partial<RuleDraft>, string]>([
    ['値のない文字列', { field: 'artist', op: 'contains', value: '  ' }, 'valueRequired'],
    ['長すぎる文字列', { field: 'artist', op: 'is', value: 'あ'.repeat(256) }, 'valueTooLong'],
    ['値のない数値', { field: 'year', op: 'is', value: '' }, 'valueRequired'],
    ['数でない値', { field: 'year', op: 'is', value: '20x0' }, 'numberInvalid'],
    ['負の数', { field: 'playCount', op: 'atLeast', value: '-1' }, 'numberInvalid'],
    ['小数の回数', { field: 'playCount', op: 'atLeast', value: '1.5' }, 'numberInvalid'],
    ['大きすぎる数', { field: 'year', op: 'is', value: '1000001' }, 'numberInvalid'],
    ['範囲の外の評価', { field: 'rating', op: 'atLeast', value: '11' }, 'ratingInvalid'],
    ['上の値のない範囲', { field: 'year', op: 'between', value: '1990' }, 'valueRequired'],
    ['逆の範囲', { field: 'year', op: 'between', value: '1999', valueTo: '1990' }, 'rangeInvalid'],
    ['0日', { field: 'createdAt', op: 'inLast', value: '0' }, 'daysInvalid'],
    ['日数のない条件', { field: 'createdAt', op: 'inLast', value: '' }, 'valueRequired']
  ])('%sは、誤りとして条件の番号を返す', (_name, overrides, error) => {
    const result = toSmartRules(
      form({ drafts: [draft({ key: 0, value: 'ok' }), draft({ key: 7, ...overrides })] })
    );
    expect(result).toEqual({ rules: null, error, ruleKey: 7 });
  });

  it.each(['', '0', '100001', '2.5', 'abc'])('曲数の上限「%s」は、誤りにする', (limit) => {
    expect(toSmartRules(form({ isLimited: true, limit }))).toEqual({
      rules: null,
      error: 'limitInvalid',
      ruleKey: null
    });
  });

  it('制限しない場合は、上限の入力を見ない', () => {
    expect(toSmartRules(form({ isLimited: false, limit: 'abc' })).rules?.limit).toBeNull();
  });

  it('条件が多すぎる場合は、誤りにする', () => {
    const drafts = Array.from({ length: 51 }, (_, key) =>
      draft({ key, field: 'favorite', op: 'is' })
    );
    expect(toSmartRules(form({ drafts })).error).toBe('tooManyRules');
  });
});

describe('新しい項目（キー・プレイリスト・見つからない曲・サンプルレート）', () => {
  it('比べ方と、最初の値', () => {
    expect(opsFor('analyzedKey')).toEqual(['is', 'isNot', 'isEmpty', 'isNotEmpty']);
    expect(opsFor('playlist')).toEqual(['is', 'isNot']);
    expect(opsFor('missing')).toEqual(['is', 'isNot']);
    expect(newRuleDraft(0, 'analyzedKey')).toMatchObject({ op: 'is', value: '0' });
    expect(newRuleDraft(0, 'playlist')).toMatchObject({ op: 'is', value: '' });
    expect(needsValue(draft({ field: 'missing', op: 'is' }))).toBe(false);
    expect(needsValue(draft({ field: 'playlist', op: 'isNot' }))).toBe(true);
    expect(needsValue(draft({ field: 'analyzedKey', op: 'isEmpty' }))).toBe(false);
  });

  it('キーの選択肢は、24の調を長調・短調の順に並べる', () => {
    expect(KEY_VALUES).toHaveLength(24);
    expect(new Set(KEY_VALUES).size).toBe(24);
    expect(KEY_VALUES.slice(0, 4)).toEqual([0, 12, 1, 13]);
  });

  it('入力を、保存する条件にする', () => {
    const result = toSmartRules(
      form({
        drafts: [
          draft({ key: 0, field: 'analyzedKey', op: 'is', value: '21' }),
          draft({ key: 1, field: 'analyzedKey', op: 'isEmpty', value: '21' }),
          draft({ key: 2, field: 'playlist', op: 'isNot', value: 'p1' }),
          draft({ key: 3, field: 'missing', op: 'is' }),
          draft({ key: 4, field: 'sampleRate', op: 'atLeast', value: '44.1' })
        ]
      })
    );
    expect(result.rules?.rules).toEqual([
      { kind: 'key', op: 'is', value: 21 },
      { kind: 'key', op: 'isEmpty', value: 0 },
      { kind: 'playlist', op: 'isNot', playlistId: 'p1' },
      { kind: 'missing', value: true },
      { kind: 'number', field: 'sampleRate', op: 'atLeast', value: 44_100, valueTo: null }
    ]);
  });

  it('プレイリストを選んでいない条件は、誤りにする', () => {
    const result = toSmartRules(
      form({ drafts: [draft({ key: 7, field: 'playlist', op: 'is', value: '' })] })
    );
    expect(result).toMatchObject({ error: 'playlistRequired', ruleKey: 7 });
  });

  it('保存してある条件を入力にし、同じ条件に戻せる', () => {
    const rules: SmartRule[] = [
      { kind: 'key', op: 'isNot', value: 9 },
      { kind: 'key', op: 'isNotEmpty', value: 0 },
      { kind: 'playlist', op: 'is', playlistId: 'p2' },
      { kind: 'missing', value: false },
      { kind: 'number', field: 'sampleRate', op: 'between', value: 44_100, valueTo: 48_000 },
      { kind: 'number', field: 'analyzedBpm', op: 'atLeast', value: 120, valueTo: null },
      { kind: 'text', field: 'grouping', op: 'contains', value: 'Live' }
    ];
    const drafts = toRuleDrafts(rules);
    expect(drafts[4]).toMatchObject({ value: '44.1', valueTo: '48' });
    expect(toSmartRules(form({ drafts })).rules?.rules).toEqual(rules);
  });
});

describe('toSmartRulesForm・toRuleDrafts', () => {
  const rules: SmartRules = {
    matchMode: 'any',
    rules: [
      { kind: 'text', field: 'path', op: 'contains', value: '/Live/' },
      { kind: 'number', field: 'duration', op: 'between', value: 90, valueTo: 600 },
      { kind: 'number', field: 'rating', op: 'atLeast', value: 4, valueTo: null },
      { kind: 'date', field: 'createdAt', op: 'inLast', days: 30 },
      { kind: 'date', field: 'lastPlayedAt', op: 'isEmpty', days: 0 },
      { kind: 'favorite', value: true }
    ],
    order: { field: 'random', descending: false },
    limit: 25
  };

  it('保存してある条件を入力にし、入力から同じ条件に戻せる', () => {
    const restored = toSmartRulesForm(rules);

    expect(restored.drafts.map(({ value, valueTo }) => [value, valueTo])).toEqual([
      ['/Live/', ''],
      // 秒で保存した長さは、分で出す
      ['1.5', '10'],
      ['4', ''],
      ['30', ''],
      ['', ''],
      ['', '']
    ]);
    expect(restored.drafts.map((d) => d.key)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(restored).toMatchObject({ matchMode: 'any', isLimited: true, limit: '25' });
    expect(toSmartRules(restored).rules).toEqual(rules);
  });

  it('上限のない条件は、制限しない入力にする', () => {
    const restored = toSmartRulesForm({ ...rules, limit: null });
    expect(restored).toMatchObject({ isLimited: false, limit: '25' });
    expect(toRuleDrafts([])).toEqual([]);
  });

  it('新しい入力は、値のない条件を1つ持つ', () => {
    const fresh = newSmartRulesForm();
    expect(fresh.drafts).toHaveLength(1);
    expect(toSmartRules(fresh).error).toBe('valueRequired');
  });
});

describe('initialOrder', () => {
  it('名前は昇順、数値・日付は降順から始める', () => {
    expect(initialOrder('title')).toEqual({ field: 'title', descending: false });
    expect(initialOrder('playCount')).toEqual({ field: 'playCount', descending: true });
    expect(initialOrder('createdAt')).toEqual({ field: 'createdAt', descending: true });
    expect(initialOrder('random')).toEqual({ field: 'random', descending: false });
  });
});

describe('describeRule・describeOrder', () => {
  it.each<[SmartRule, string]>([
    [{ kind: 'text', field: 'genre', op: 'contains', value: 'Jazz' }, 'ジャンル: 「Jazz」を含む'],
    [{ kind: 'text', field: 'path', op: 'isEmpty', value: '' }, 'ファイルの場所: 空'],
    [{ kind: 'number', field: 'rating', op: 'atLeast', value: 8, valueTo: null }, '評価: ★★★★以上'],
    [{ kind: 'number', field: 'rating', op: 'atLeast', value: 7, valueTo: null }, '評価: ★★★½以上'],
    [{ kind: 'number', field: 'rating', op: 'is', value: 0, valueTo: null }, '評価: 評価なし'],
    [
      { kind: 'number', field: 'year', op: 'between', value: 1990, valueTo: 1999 },
      '年: 1990〜1999'
    ],
    [
      { kind: 'number', field: 'duration', op: 'atMost', value: 90, valueTo: null },
      '時間: 1.5分以下'
    ],
    [
      { kind: 'number', field: 'bitrate', op: 'atLeast', value: 320, valueTo: null },
      'ビットレート: 320 kbps以上'
    ],
    [
      { kind: 'date', field: 'lastPlayedAt', op: 'notInLast', days: 90 },
      '最終再生日: 過去90日より前'
    ],
    [{ kind: 'date', field: 'lastPlayedAt', op: 'isEmpty', days: 0 }, '最終再生日: なし'],
    [{ kind: 'favorite', value: true }, 'お気に入り'],
    [{ kind: 'favorite', value: false }, 'お気に入りではない'],
    [{ kind: 'inMusicFolder', value: true }, '音楽フォルダの中にある'],
    [{ kind: 'inMusicFolder', value: false }, '音楽フォルダの外にある'],
    [
      { kind: 'number', field: 'sampleRate', op: 'atLeast', value: 96_000, valueTo: null },
      'サンプルレート: 96 kHz以上'
    ],
    [
      { kind: 'number', field: 'bitDepth', op: 'is', value: 24, valueTo: null },
      'ビット深度: 24 bit'
    ],
    [{ kind: 'text', field: 'composer', op: 'is', value: 'Bach' }, '作曲者: 「Bach」'],
    [{ kind: 'key', op: 'is', value: 21 }, 'キー（解析）: Am'],
    [{ kind: 'key', op: 'isNot', value: 0 }, 'キー（解析）: C以外'],
    [{ kind: 'key', op: 'isEmpty', value: 0 }, 'キー（解析）: なし（未解析）'],
    [{ kind: 'missing', value: true }, 'ファイルが見つからない'],
    [{ kind: 'missing', value: false }, 'ファイルがある']
  ])('条件を説明する', (rule, expected) => {
    expect(describeRule(rule)).toBe(expected);
  });

  it('プレイリストの条件は、プレイリストの名前で説明する（消したプレイリストは、その旨）', () => {
    const names = (id: string) => (id === 'p1' ? '車では聴かない' : null);
    expect(describeRule({ kind: 'playlist', op: 'isNot', playlistId: 'p1' }, names)).toBe(
      '「車では聴かない」に入っていない'
    );
    expect(describeRule({ kind: 'playlist', op: 'is', playlistId: 'gone' }, names)).toBe(
      '「（削除したプレイリスト）」に入っている'
    );
  });

  it('並び順と上限を説明する', () => {
    expect(describeOrder({ order: { field: 'playCount', descending: true }, limit: null })).toBe(
      '再生回数の降順'
    );
    expect(describeOrder({ order: { field: 'title', descending: false }, limit: 25 })).toBe(
      'タイトルの昇順で25曲まで'
    );
    expect(describeOrder({ order: { field: 'random', descending: false }, limit: 10 })).toBe(
      'ランダムで10曲まで'
    );
  });
});
