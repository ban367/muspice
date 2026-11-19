import { describe, expect, it } from 'vitest';
import type { SmartOrder, SmartRule, SmartRules, Track } from '#lib/types/models.js';
import {
  countSmartPlaylistTracks,
  findSmartPlaylistTracks,
  smartRulesError
} from './smartPlaylist.js';

const NOW = new Date('2026-10-10T00:00:00Z').getTime();

function track(id: string, overrides: Partial<Track> = {}): Track {
  return {
    id,
    filePath: `/music/${id}.mp3`,
    fileName: `${id}.mp3`,
    title: id,
    artist: null,
    album: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: null,
    fileSize: 1,
    format: 'mp3',
    bitrate: null,
    sampleRate: null,
    isFavorite: false,
    rating: 0,
    playCount: 0,
    skipCount: 0,
    lastPlayedAt: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
    sortTags: { title: null, artist: null, album: null, albumArtist: null },
    isMissing: false,
    compilation: false,
    details: {
      composer: null,
      grouping: null,
      comment: null,
      bpm: null,
      bitDepth: null,
      channels: null,
      analyzedBpm: null,
      analyzedKey: null
    },
    ...overrides
  };
}

const LIBRARY: Track[] = [
  track('a', {
    artist: '宇多田ヒカル',
    genre: 'J-Pop',
    year: 1999,
    rating: 5,
    playCount: 30,
    isFavorite: true,
    createdAt: '2026-10-07T00:00:00Z',
    lastPlayedAt: '2026-10-09T12:00:00+09:00'
  }),
  track('b', { artist: 'ZARD', genre: 'J-Pop', rating: 4, playCount: 10 }),
  track('c', {
    artist: 'Miles Davis',
    genre: 'Jazz',
    year: 1959,
    rating: 2,
    playCount: 20,
    isFavorite: true,
    lastPlayedAt: '2026-07-02T00:00:00Z'
  }),
  track('d', { artist: 'Bill Evans' })
];

const TITLE: SmartOrder = { field: 'title', descending: false };

function rules(overrides: Partial<SmartRules> = {}): SmartRules {
  return { matchMode: 'all', rules: [], order: TITLE, limit: null, ...overrides };
}

function ids(smartRules: SmartRules, shuffleSeed = 1, library = LIBRARY): string[] {
  return findSmartPlaylistTracks(library, smartRules, { now: NOW, shuffleSeed }).map((t) => t.id);
}

const match = (...list: SmartRule[]) => ids(rules({ rules: list }));

describe('findSmartPlaylistTracks', () => {
  it('条件がなければ、「すべて」はすべての曲、「いずれか」は曲なし', () => {
    expect(ids(rules())).toEqual(['a', 'b', 'c', 'd']);
    expect(ids(rules({ matchMode: 'any' }))).toEqual([]);
  });

  it('文字列は、大文字と小文字・全角と半角・ひらがなとカタカナを区別せずに比べる', () => {
    expect(match({ kind: 'text', field: 'artist', op: 'contains', value: 'ひかる' })).toEqual([
      'a'
    ]);
    expect(match({ kind: 'text', field: 'artist', op: 'is', value: 'ｚａｒｄ' })).toEqual(['b']);
    expect(match({ kind: 'text', field: 'artist', op: 'startsWith', value: 'miles' })).toEqual([
      'c'
    ]);
    expect(match({ kind: 'text', field: 'artist', op: 'endsWith', value: 'EVANS' })).toEqual(['d']);
    // ジャンルのない曲は、「J-Popではない」に含める
    expect(match({ kind: 'text', field: 'genre', op: 'isNot', value: 'j-pop' })).toEqual([
      'c',
      'd'
    ]);
    expect(match({ kind: 'text', field: 'genre', op: 'notContains', value: 'pop' })).toEqual([
      'c',
      'd'
    ]);
    expect(match({ kind: 'text', field: 'genre', op: 'isEmpty', value: '' })).toEqual(['d']);
    expect(match({ kind: 'text', field: 'genre', op: 'isNotEmpty', value: '' })).toHaveLength(3);
    expect(match({ kind: 'text', field: 'path', op: 'contains', value: '/music/b' })).toEqual([
      'b'
    ]);
    expect(match({ kind: 'text', field: 'format', op: 'is', value: 'MP3' })).toHaveLength(4);
  });

  it('アーティスト・ジャンルが複数ある曲は、値のどれかが条件に合えば合う', () => {
    const library = [
      track('a', { artist: '宇多田ヒカル; ZARD', genre: 'Rock; J-Pop', album: 'Live; Remastered' }),
      track('b', { artist: 'ZARD', genre: 'J-Pop' }),
      track('c', { artist: 'Miles Davis', genre: 'Jazz' }),
      track('d', { artist: 'Popper', genre: null })
    ];
    const find = (rule: SmartRule) => ids(rules({ rules: [rule] }), 1, library);

    expect(find({ kind: 'text', field: 'genre', op: 'is', value: 'rock' })).toEqual(['a']);
    expect(find({ kind: 'text', field: 'genre', op: 'is', value: 'j-pop' })).toEqual(['a', 'b']);
    // 値の一部だけでは「である」に合わない。分ける前の文字列そのものには合う
    expect(find({ kind: 'text', field: 'genre', op: 'is', value: 'pop' })).toEqual([]);
    expect(find({ kind: 'text', field: 'genre', op: 'is', value: 'Rock; J-Pop' })).toEqual(['a']);
    // 「ではない」は、どの値も合わない曲（値のない曲を含む）
    expect(find({ kind: 'text', field: 'genre', op: 'isNot', value: 'j-pop' })).toEqual(['c', 'd']);
    expect(find({ kind: 'text', field: 'artist', op: 'startsWith', value: 'zard' })).toEqual([
      'a',
      'b'
    ]);
    expect(find({ kind: 'text', field: 'artist', op: 'endsWith', value: 'ひかる' })).toEqual(['a']);
    expect(find({ kind: 'text', field: 'artist', op: 'notContains', value: 'zard' })).toEqual([
      'c',
      'd'
    ]);
    // アルバムなど、ほかの項目は分けない
    expect(find({ kind: 'text', field: 'album', op: 'is', value: 'Live' })).toEqual([]);
  });

  it('数値を比べる（値のない曲は、「その値ではない」にだけ含める）', () => {
    const number = (
      field: 'rating' | 'year' | 'playCount',
      op: 'is' | 'isNot' | 'atLeast' | 'atMost' | 'between',
      value: number,
      valueTo: number | null = null
    ) => match({ kind: 'number', field, op, value, valueTo });

    expect(number('rating', 'atLeast', 4)).toEqual(['a', 'b']);
    expect(number('rating', 'atMost', 2)).toEqual(['c', 'd']);
    expect(number('rating', 'is', 0)).toEqual(['d']);
    expect(number('year', 'is', 1999)).toEqual(['a']);
    expect(number('year', 'isNot', 1999)).toEqual(['b', 'c', 'd']);
    expect(number('year', 'atMost', 1999)).toEqual(['a', 'c']);
    expect(number('year', 'between', 1950, 1960)).toEqual(['c']);
    expect(number('playCount', 'is', 0)).toEqual(['d']);
  });

  it('日付を比べる（「過去N日より前」には、日付のない曲も入る）', () => {
    const date = (
      field: 'createdAt' | 'lastPlayedAt',
      op: 'inLast' | 'notInLast' | 'isEmpty' | 'isNotEmpty',
      days: number
    ) => match({ kind: 'date', field, op, days });

    expect(date('createdAt', 'inLast', 7)).toEqual(['a']);
    expect(date('createdAt', 'notInLast', 7)).toEqual(['b', 'c', 'd']);
    expect(date('lastPlayedAt', 'inLast', 7)).toEqual(['a']);
    expect(date('lastPlayedAt', 'notInLast', 30)).toEqual(['b', 'c', 'd']);
    expect(date('lastPlayedAt', 'isEmpty', 0)).toEqual(['b', 'd']);
    expect(date('lastPlayedAt', 'isNotEmpty', 0)).toEqual(['a', 'c']);
  });

  it('音楽フォルダの中にあるか（音楽フォルダが未設定なら、どの曲も中にない）', () => {
    const library = [
      ...LIBRARY.slice(0, 2),
      { ...LIBRARY[2], filePath: '/Users/me/Downloads/c.mp3' }
    ];
    const inside = (value: boolean, musicFolderPath: string | null) =>
      findSmartPlaylistTracks(library, rules({ rules: [{ kind: 'inMusicFolder', value }] }), {
        now: NOW,
        shuffleSeed: 1,
        musicFolderPath
      }).map((t) => t.id);

    expect(inside(true, '/music')).toEqual(['a', 'b']);
    expect(inside(false, '/music')).toEqual(['c']);
    expect(inside(true, null)).toEqual([]);
    expect(inside(false, null)).toEqual(['a', 'b', 'c']);
  });

  it('お気に入りと、条件の組み合わせ方', () => {
    const favorite: SmartRule = { kind: 'favorite', value: true };
    const jazz: SmartRule = { kind: 'text', field: 'genre', op: 'is', value: 'Jazz' };
    const zard: SmartRule = { kind: 'text', field: 'artist', op: 'is', value: 'ZARD' };

    expect(match(favorite)).toEqual(['a', 'c']);
    expect(match({ kind: 'favorite', value: false })).toEqual(['b', 'd']);
    expect(match(favorite, jazz)).toEqual(['c']);
    expect(ids(rules({ matchMode: 'any', rules: [jazz, zard] }))).toEqual(['b', 'c']);
  });

  it('並び順と上限（上限は、並び順の先頭から選ぶ）', () => {
    const ordered = (field: SmartOrder['field'], descending: boolean, limit: number | null) =>
      ids(rules({ order: { field, descending }, limit }));

    expect(ordered('playCount', true, null)).toEqual(['a', 'c', 'b', 'd']);
    expect(ordered('playCount', true, 2)).toEqual(['a', 'c']);
    expect(ordered('playCount', false, 1)).toEqual(['d']);
    expect(ordered('artist', false, null)).toEqual(['d', 'c', 'b', 'a']);
    // 値のない曲は、昇順では先頭、降順では末尾
    expect(ordered('year', false, null)).toEqual(['d', 'b', 'c', 'a']);
    expect(ordered('year', true, null)).toEqual(['a', 'c', 'd', 'b']);
  });

  it('ランダムな並びは、種が同じ間は変わらず、種を変えると変わる', () => {
    const library = Array.from({ length: 40 }, (_, index) =>
      track(`t${String(index).padStart(2, '0')}`)
    );
    const random = (seed: number, limit: number | null = null) =>
      ids(rules({ order: { field: 'random', descending: false }, limit }), seed, library);

    const first = random(1);
    expect([...first].sort()).toEqual(library.map((t) => t.id));
    expect(first).not.toEqual(library.map((t) => t.id));
    expect(random(1)).toEqual(first);
    expect(random(1, 5)).toEqual(first.slice(0, 5));
    expect(random(2)).not.toEqual(first);
  });
});

describe('タグ・解析・ファイルの状態・プレイリストの条件', () => {
  const details = (overrides: Partial<Track['details']>): Partial<Track> => ({
    details: { ...track('x').details, ...overrides }
  });
  const library = [
    track('a', {
      ...details({ composer: 'Bach', bpm: 128, analyzedBpm: 124.5, analyzedKey: 21 }),
      sampleRate: 96_000
    }),
    track('b', { ...details({ composer: 'Bachmann', analyzedKey: 0 }), isMissing: true }),
    track('c'),
    track('d')
  ];
  const find = (
    list: SmartRule[],
    context: Partial<Parameters<typeof findSmartPlaylistTracks>[2]> = {}
  ) =>
    findSmartPlaylistTracks(library, rules({ rules: list }), {
      now: NOW,
      shuffleSeed: 1,
      ...context
    }).map((t) => t.id);

  it('作曲者で比べる', () => {
    expect(find([{ kind: 'text', field: 'composer', op: 'is', value: 'bach' }])).toEqual(['a']);
    expect(find([{ kind: 'text', field: 'composer', op: 'contains', value: 'bach' }])).toEqual([
      'a',
      'b'
    ]);
  });

  it('解析したテンポは四捨五入して比べ、キー・見つからない曲・サンプルレートで絞り込む', () => {
    const number = (
      field: 'analyzedBpm' | 'sampleRate' | 'bpm',
      op: 'is' | 'atLeast',
      value: number
    ) => ({ kind: 'number', field, op, value, valueTo: null }) as const;
    expect(find([number('analyzedBpm', 'is', 125)])).toEqual(['a']);
    expect(find([number('bpm', 'atLeast', 120)])).toEqual(['a']);
    expect(find([number('sampleRate', 'atLeast', 48_000)])).toEqual(['a']);
    expect(find([{ kind: 'key', op: 'is', value: 21 }])).toEqual(['a']);
    expect(find([{ kind: 'key', op: 'isNot', value: 21 }])).toEqual(['b', 'c', 'd']);
    expect(find([{ kind: 'key', op: 'isEmpty', value: 0 }])).toEqual(['c', 'd']);
    expect(find([{ kind: 'missing', value: true }])).toEqual(['b']);
  });

  it('プレイリストに入っている曲で比べる（プレイリストが分からなければ、曲のないプレイリスト）', () => {
    const playlistMembers = (id: string) => new Set(id === 'p1' ? ['a', 'c'] : []);
    expect(find([{ kind: 'playlist', op: 'is', playlistId: 'p1' }], { playlistMembers })).toEqual([
      'a',
      'c'
    ]);
    expect(
      find([{ kind: 'playlist', op: 'isNot', playlistId: 'p1' }], { playlistMembers })
    ).toEqual(['b', 'd']);
    expect(find([{ kind: 'playlist', op: 'isNot', playlistId: 'p1' }])).toEqual([
      'a',
      'b',
      'c',
      'd'
    ]);
  });

  it('キー・プレイリストの条件を検証する', () => {
    expect(
      smartRulesError(rules({ rules: [{ kind: 'key', op: 'is', value: 24 }] }))
    ).not.toBeNull();
    expect(
      smartRulesError(rules({ rules: [{ kind: 'key', op: 'isEmpty', value: 99 }] }))
    ).toBeNull();
    expect(
      smartRulesError(rules({ rules: [{ kind: 'playlist', op: 'is', playlistId: 'not-a-uuid' }] }))
    ).not.toBeNull();
  });
});

describe('countSmartPlaylistTracks', () => {
  it('条件に合う曲数を、上限を超えない範囲で返す', () => {
    expect(countSmartPlaylistTracks(LIBRARY, rules(), { now: NOW, shuffleSeed: 1 })).toBe(4);
    expect(
      countSmartPlaylistTracks(LIBRARY, rules({ limit: 3 }), { now: NOW, shuffleSeed: 1 })
    ).toBe(3);
    expect(
      countSmartPlaylistTracks(LIBRARY, rules({ limit: 10 }), { now: NOW, shuffleSeed: 1 })
    ).toBe(4);
  });
});

describe('smartRulesError', () => {
  const error = (rule: SmartRule) => smartRulesError(rules({ rules: [rule] }));

  it('正しい条件は、誤りなし', () => {
    expect(smartRulesError(rules())).toBeNull();
    expect(error({ kind: 'text', field: 'genre', op: 'isEmpty', value: '' })).toBeNull();
    expect(
      error({ kind: 'number', field: 'year', op: 'between', value: 1990, valueTo: 1999 })
    ).toBeNull();
    expect(error({ kind: 'date', field: 'lastPlayedAt', op: 'isEmpty', days: 0 })).toBeNull();
  });

  it('誤りのある条件は、説明を返す', () => {
    expect(error({ kind: 'text', field: 'genre', op: 'is', value: ' ' })).not.toBeNull();
    expect(
      error({ kind: 'number', field: 'rating', op: 'atLeast', value: 11, valueTo: null })
    ).not.toBeNull();
    expect(
      error({ kind: 'number', field: 'year', op: 'between', value: 1999, valueTo: 1990 })
    ).not.toBeNull();
    expect(error({ kind: 'date', field: 'createdAt', op: 'inLast', days: 0 })).not.toBeNull();
    expect(smartRulesError(rules({ limit: 0 }))).not.toBeNull();
  });
});
