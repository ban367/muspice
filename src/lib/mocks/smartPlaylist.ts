/**
 * 自動プレイリストの条件から曲を求める（モックのバックエンド用）
 *
 * 実装（Rustの`smart_playlist.rs`）はSQLで求める。ここでは同じ規則を、モックが持つ曲の一覧に
 * 対して行う。アプリの本体からは使わない。
 */
import type { SmartOrder, SmartRule, SmartRules, Track } from '#lib/types/models.js';
import { normalizeSearchText } from '#lib/utils/searchText.js';

const DAY_MS = 24 * 60 * 60 * 1000;

/** 条件から曲を求める時の状況（Rustの`EvalContext`に対応） */
export interface SmartPlaylistContext {
  /** 現在の日時（ミリ秒。日付の条件「過去N日以内」の基準） */
  now: number;
  /** ランダムな並びの種（同じ種なら、同じ並びになる） */
  shuffleSeed: number;
}

/** 条件の誤りを説明する文（誤りがなければnull。Rustの`validate_rules`に対応） */
export function smartRulesError(rules: SmartRules): string | null {
  if (rules.rules.length > 50) return '条件が多すぎます（50個まで）';
  if (rules.limit !== null && (rules.limit < 1 || rules.limit > 100_000)) {
    return '曲数の上限は1から100000の範囲で指定してください';
  }

  for (const rule of rules.rules) {
    switch (rule.kind) {
      case 'text': {
        const needsValue = rule.op !== 'isEmpty' && rule.op !== 'isNotEmpty';
        if (needsValue && rule.value.trim() === '') return '条件の値を入力してください';
        if ([...rule.value].length > 255) return '条件の値が長すぎます（255文字まで）';
        break;
      }
      case 'number': {
        const max = rule.field === 'rating' ? 5 : 1_000_000;
        const inRange = (value: number) => Number.isInteger(value) && value >= 0 && value <= max;
        if (!inRange(rule.value)) return '条件の数値が範囲の外です';
        if (rule.op === 'between') {
          if (rule.valueTo === null || !inRange(rule.valueTo) || rule.valueTo < rule.value) {
            return '範囲の条件は、小さい値と大きい値の順に指定してください';
          }
        }
        break;
      }
      case 'date': {
        const needsDays = rule.op === 'inLast' || rule.op === 'notInLast';
        if (needsDays && (rule.days < 1 || rule.days > 36_500)) {
          return '日数は1から36500の範囲で指定してください';
        }
        break;
      }
      case 'favorite':
        break;
    }
  }
  return null;
}

type TextRule = Extract<SmartRule, { kind: 'text' }>;
type NumberRule = Extract<SmartRule, { kind: 'number' }>;
type DateRule = Extract<SmartRule, { kind: 'date' }>;

function textValue(track: Track, field: TextRule['field']): string | null {
  return field === 'path' ? track.filePath : track[field];
}

function matchesText(track: Track, rule: TextRule): boolean {
  const raw = textValue(track, rule.field);
  if (rule.op === 'isEmpty') return raw === null || raw.trim() === '';
  if (rule.op === 'isNotEmpty') return raw !== null && raw.trim() !== '';

  // 検索と同じ正規化をして比べる（値のない項目は、空の文字列として扱う）
  const value = normalizeSearchText(raw ?? '');
  const needle = normalizeSearchText(rule.value.trim());
  switch (rule.op) {
    case 'contains':
      return value.includes(needle);
    case 'notContains':
      return !value.includes(needle);
    case 'is':
      return value === needle;
    case 'isNot':
      return value !== needle;
    case 'startsWith':
      return value.startsWith(needle);
    case 'endsWith':
      return value.endsWith(needle);
  }
}

function matchesNumber(track: Track, rule: NumberRule): boolean {
  const value: number | null = track[rule.field];
  // 値のない曲（年のない曲など）は、「その値ではない」にだけ含める
  if (value === null) return rule.op === 'isNot';
  switch (rule.op) {
    case 'is':
      return value === rule.value;
    case 'isNot':
      return value !== rule.value;
    case 'atLeast':
      return value >= rule.value;
    case 'atMost':
      return value <= rule.value;
    case 'between':
      return value >= rule.value && value <= (rule.valueTo ?? rule.value);
  }
}

function matchesDate(track: Track, rule: DateRule, now: number): boolean {
  const text = track[rule.field];
  const time = text === null ? null : new Date(text).getTime();
  const cutoff = now - rule.days * DAY_MS;
  switch (rule.op) {
    case 'inLast':
      return time !== null && time >= cutoff;
    // 「長く聴いていない曲」には、未再生の曲も入る
    case 'notInLast':
      return time === null || time < cutoff;
    case 'isEmpty':
      return time === null;
    case 'isNotEmpty':
      return time !== null;
  }
}

/** 曲が、1つの条件に合うか */
export function matchesSmartRule(track: Track, rule: SmartRule, now: number): boolean {
  switch (rule.kind) {
    case 'text':
      return matchesText(track, rule);
    case 'number':
      return matchesNumber(track, rule);
    case 'date':
      return matchesDate(track, rule, now);
    case 'favorite':
      return track.isFavorite === rule.value;
  }
}

type OrderKey = string | number | null;

/** 値のないものを先頭にして比べる（SQLiteの昇順と同じ） */
function compareKeys(a: OrderKey, b: OrderKey): number {
  if (a === b) return 0;
  if (a === null) return -1;
  if (b === null) return 1;
  return a < b ? -1 : 1;
}

const time = (text: string | null) => (text === null ? null : new Date(text).getTime());

function orderKey(track: Track, field: SmartOrder['field']): OrderKey {
  switch (field) {
    case 'title':
      return normalizeSearchText(track.sortTags.title ?? track.title ?? track.fileName);
    case 'artist': {
      const artist = track.sortTags.artist ?? track.artist;
      return artist === null ? null : normalizeSearchText(artist);
    }
    case 'album': {
      const album = track.sortTags.album ?? track.album;
      return album === null ? null : normalizeSearchText(album);
    }
    case 'createdAt':
      return time(track.createdAt);
    case 'lastPlayedAt':
      return time(track.lastPlayedAt);
    case 'playCount':
    case 'rating':
    case 'year':
    case 'duration':
      return track[field];
    case 'random':
      return null;
  }
}

/** 同じ値の曲の並び（アーティスト → アルバム → アルバムの中の並び） */
function compareWithinSameKey(a: Track, b: Track): number {
  return (
    compareKeys(a.albumArtist ?? a.artist, b.albumArtist ?? b.artist) ||
    compareKeys(a.album, b.album) ||
    compareKeys(a.discNumber ?? 1, b.discNumber ?? 1) ||
    compareKeys(a.trackNumber, b.trackNumber) ||
    compareKeys(a.title, b.title) ||
    compareKeys(a.id, b.id)
  );
}

/** ランダムな並びでの、曲の位置を決める値（種と曲のIDから決まる。FNV-1a） */
function shuffleKey(seed: number, trackId: string): number {
  let hash = 0x811c9dc5;
  for (const char of `${seed}:${trackId}`) {
    hash ^= char.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** 条件に合う曲（並び順・上限は付けない） */
function matchingTracks(tracks: readonly Track[], rules: SmartRules, now: number): Track[] {
  // 「すべてを満たす」で条件がなければすべての曲、「いずれかを満たす」で条件がなければ曲なし
  return tracks.filter((track) =>
    rules.matchMode === 'all'
      ? rules.rules.every((rule) => matchesSmartRule(track, rule, now))
      : rules.rules.some((rule) => matchesSmartRule(track, rule, now))
  );
}

/** 条件に合う曲を、並び順のとおりに（上限があれば、その曲数まで）求める */
export function findSmartPlaylistTracks(
  tracks: readonly Track[],
  rules: SmartRules,
  context: SmartPlaylistContext
): Track[] {
  const matched = matchingTracks(tracks, rules, context.now);
  const { field, descending } = rules.order;

  if (field === 'random') {
    matched.sort(
      (a, b) =>
        shuffleKey(context.shuffleSeed, a.id) - shuffleKey(context.shuffleSeed, b.id) ||
        compareKeys(a.id, b.id)
    );
  } else {
    const direction = descending ? -1 : 1;
    matched.sort(
      (a, b) =>
        direction * compareKeys(orderKey(a, field), orderKey(b, field)) ||
        compareWithinSameKey(a, b)
    );
  }
  return rules.limit === null ? matched : matched.slice(0, rules.limit);
}

/** 条件に合う曲数（上限があれば、それを超えない） */
export function countSmartPlaylistTracks(
  tracks: readonly Track[],
  rules: SmartRules,
  now: number
): number {
  const count = matchingTracks(tracks, rules, now).length;
  return rules.limit === null ? count : Math.min(count, rules.limit);
}
