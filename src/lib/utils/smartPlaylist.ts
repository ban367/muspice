/**
 * 自動プレイリストの条件（編集画面で使う定義と変換）
 *
 * 条件に合う曲はバックエンド（Rustの`smart_playlist.rs`）が求める。ここでは、編集画面の入力
 * （文字列のままの値）と、保存する条件（`SmartRules`）との変換・検証と、条件の説明文を扱う。
 */
import type {
  DateField,
  DateOp,
  KeyOp,
  MatchMode,
  NumberField,
  NumberOp,
  SmartOrder,
  SmartOrderField,
  SmartRule,
  SmartRules,
  TextField,
  TextOp
} from '#lib/types/models.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { MAX_RATING, formatRatingStars } from './rating.js';
import { formatMusicKey } from './musicKey.js';

/** 条件にできる項目 */
export type RuleField =
  | TextField
  | NumberField
  | DateField
  | 'favorite'
  | 'inMusicFolder'
  | 'analyzedKey'
  | 'playlist'
  | 'missing';

/** 項目の種類（比べ方・値の入力の形が決まる） */
export type RuleKind = SmartRule['kind'];

/**
 * お気に入り・音楽フォルダの中にあるか・見つからないか・プレイリストに入っているかの比べ方
 * （保存する条件では、プレイリスト以外は`value`の真偽にする）
 */
export type FavoriteOp = 'is' | 'isNot';

export type RuleOp = TextOp | NumberOp | DateOp | FavoriteOp | KeyOp;

/** 数値の項目の、入力の形 */
type NumberInput = 'integer' | 'rating' | 'minutes' | 'kilohertz';

interface RuleFieldDefinition {
  kind: RuleKind;
  /**
   * 数値の項目の入力の形（評価は星の数を選ぶ。長さは分で入力し、秒で保存する。サンプルレートは
   * kHzで入力し、Hzで保存する）
   */
  input?: NumberInput;
}

/** 条件にできる項目（この並びが、項目を選ぶメニューの順になる） */
export const RULE_FIELDS: Record<RuleField, RuleFieldDefinition> = {
  title: { kind: 'text' },
  artist: { kind: 'text' },
  albumArtist: { kind: 'text' },
  album: { kind: 'text' },
  composer: { kind: 'text' },
  grouping: { kind: 'text' },
  genre: { kind: 'text' },
  comment: { kind: 'text' },
  year: { kind: 'number', input: 'integer' },
  rating: { kind: 'number', input: 'rating' },
  favorite: { kind: 'favorite' },
  playCount: { kind: 'number', input: 'integer' },
  skipCount: { kind: 'number', input: 'integer' },
  lastPlayedAt: { kind: 'date' },
  createdAt: { kind: 'date' },
  playlist: { kind: 'playlist' },
  duration: { kind: 'number', input: 'minutes' },
  trackNumber: { kind: 'number', input: 'integer' },
  discNumber: { kind: 'number', input: 'integer' },
  bpm: { kind: 'number', input: 'integer' },
  analyzedBpm: { kind: 'number', input: 'integer' },
  analyzedKey: { kind: 'key' },
  format: { kind: 'text' },
  bitrate: { kind: 'number', input: 'integer' },
  sampleRate: { kind: 'number', input: 'kilohertz' },
  bitDepth: { kind: 'number', input: 'integer' },
  channels: { kind: 'number', input: 'integer' },
  path: { kind: 'text' },
  inMusicFolder: { kind: 'inMusicFolder' },
  missing: { kind: 'missing' }
};
export const RULE_FIELD_IDS = Object.keys(RULE_FIELDS) as RuleField[];

const TEXT_OPS: readonly TextOp[] = [
  'contains',
  'notContains',
  'is',
  'isNot',
  'startsWith',
  'endsWith',
  'isEmpty',
  'isNotEmpty'
];
const NUMBER_OPS: readonly NumberOp[] = ['is', 'isNot', 'atLeast', 'atMost', 'between'];
const DATE_OPS: readonly DateOp[] = ['inLast', 'notInLast', 'isEmpty', 'isNotEmpty'];
const FAVORITE_OPS: readonly FavoriteOp[] = ['is', 'isNot'];
const KEY_OPS: readonly KeyOp[] = ['is', 'isNot', 'isEmpty', 'isNotEmpty'];

/**
 * キーの条件で選べる値（主音の音名 + 12 × 調。ハ長調・ハ短調・変ニ長調…の順）
 *
 * 値の表記は、一覧の列と同じ（`formatMusicKey`）。
 */
export const KEY_VALUES: readonly number[] = Array.from({ length: 24 }, (_, index) =>
  index % 2 === 0 ? index / 2 : (index - 1) / 2 + 12
);

/** 並び順に使える項目（この並びが、メニューの順になる） */
export const ORDER_FIELDS: readonly SmartOrderField[] = [
  'random',
  'title',
  'artist',
  'album',
  'createdAt',
  'lastPlayedAt',
  'playCount',
  'rating',
  'year',
  'duration'
];

/** 条件の数の上限（Rustの`MAX_RULES`と同じ） */
export const MAX_RULES = 50;
/** 曲数の上限として指定できる最大の値（Rustの`MAX_LIMIT`と同じ） */
const MAX_LIMIT = 100_000;
/** キーの条件の値の上限（Rustの`MAX_KEY`と同じ） */
const MAX_KEY = 23;
const MAX_NUMBER = 1_000_000;
const MAX_DAYS = 36_500;
const MAX_TEXT_LENGTH = 255;

/** 項目で選べる比べ方 */
export function opsFor(field: RuleField): readonly RuleOp[] {
  switch (RULE_FIELDS[field].kind) {
    case 'text':
      return TEXT_OPS;
    case 'number':
      return NUMBER_OPS;
    case 'date':
      // 追加日はどの曲にもあるため、「なし・あり」は出さない
      return field === 'createdAt' ? DATE_OPS.slice(0, 2) : DATE_OPS;
    case 'favorite':
    case 'inMusicFolder':
    case 'missing':
    case 'playlist':
      return FAVORITE_OPS;
    case 'key':
      return KEY_OPS;
  }
}

/** 編集画面の、1つの条件（値は入力した文字列のまま持つ） */
export interface RuleDraft {
  /** 行を見分ける番号（一覧の描画用。保存しない） */
  key: number;
  field: RuleField;
  op: RuleOp;
  /** 値（文字列・数値・日数。キーは`KEY_VALUES`の値、プレイリストはID） */
  value: string;
  /** 範囲の条件の、大きい方の値 */
  valueTo: string;
}

/** 値の入力がいる比べ方か（空かどうか・お気に入りかどうかなどは、値がいらない） */
export function needsValue(draft: Pick<RuleDraft, 'field' | 'op'>): boolean {
  const kind = RULE_FIELDS[draft.field].kind;
  if (kind === 'favorite' || kind === 'inMusicFolder' || kind === 'missing') return false;
  return draft.op !== 'isEmpty' && draft.op !== 'isNotEmpty';
}

/**
 * 項目を選んだ直後の値（評価・キーは一覧から選ぶため、最初から値（星4・ハ長調）を入れておく。
 * プレイリストは、選ぶまで空）
 */
function initialValue(field: RuleField): string {
  if (RULE_FIELDS[field].input === 'rating') return '8';
  return RULE_FIELDS[field].kind === 'key' ? '0' : '';
}

/** 新しい条件（項目の最初の比べ方・空の値） */
export function newRuleDraft(key: number, field: RuleField = 'artist'): RuleDraft {
  const [op] = opsFor(field);
  // 数値は「以上」、日付は「過去N日以内」から始める方が、よく使う条件に近い
  const kind = RULE_FIELDS[field].kind;
  return {
    key,
    field,
    op: kind === 'number' ? 'atLeast' : op,
    value: initialValue(field),
    valueTo: ''
  };
}

/**
 * 条件の項目を変える
 *
 * 同じ種類の項目へ変える場合は、比べ方と値を残す（入力の形が違う場合は、値を空にする）。
 */
export function changeRuleField(draft: RuleDraft, field: RuleField): RuleDraft {
  const before = RULE_FIELDS[draft.field];
  const after = RULE_FIELDS[field];
  if (before.kind !== after.kind) return newRuleDraft(draft.key, field);

  const op = opsFor(field).includes(draft.op) ? draft.op : opsFor(field)[0];
  if (before.input !== after.input) {
    return { ...draft, field, op, value: initialValue(field), valueTo: initialValue(field) };
  }
  return { ...draft, field, op };
}

/** 秒を、分の入力に出す文字列にする（90 → "1.5"） */
const toMinutesText = (seconds: number) => String(Math.round((seconds / 60) * 100) / 100);

/** Hzを、kHzの入力に出す文字列にする（44100 → "44.1"） */
const toKilohertzText = (hertz: number) => String(Math.round(hertz / 100) / 10);

function numberText(field: NumberField, value: number): string {
  switch (RULE_FIELDS[field].input) {
    case 'minutes':
      return toMinutesText(value);
    case 'kilohertz':
      return toKilohertzText(value);
    default:
      return String(value);
  }
}

/**
 * 保存してある条件を、編集画面の条件にする
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export function toRuleDrafts(rules: readonly SmartRule[]): RuleDraft[] {
  return rules.map((rule, key): RuleDraft => {
    switch (rule.kind) {
      case 'text':
        return { key, field: rule.field, op: rule.op, value: rule.value, valueTo: '' };
      case 'number':
        return {
          key,
          field: rule.field,
          op: rule.op,
          value: numberText(rule.field, rule.value),
          valueTo: rule.valueTo === null ? '' : numberText(rule.field, rule.valueTo)
        };
      case 'date':
        return {
          key,
          field: rule.field,
          op: rule.op,
          value: rule.days > 0 ? String(rule.days) : '',
          valueTo: ''
        };
      case 'favorite':
        return { key, field: 'favorite', op: rule.value ? 'is' : 'isNot', value: '', valueTo: '' };
      case 'inMusicFolder':
      case 'missing':
        return {
          key,
          field: rule.kind,
          op: rule.value ? 'is' : 'isNot',
          value: '',
          valueTo: ''
        };
      case 'key': {
        const usesValue = rule.op === 'is' || rule.op === 'isNot';
        return {
          key,
          field: 'analyzedKey',
          op: rule.op,
          value: usesValue ? String(rule.value) : initialValue('analyzedKey'),
          valueTo: ''
        };
      }
      case 'playlist':
        return { key, field: 'playlist', op: rule.op, value: rule.playlistId, valueTo: '' };
    }
  });
}

/** 入力の誤りの種類（メッセージは`m.smartPlaylist.errors`） */
export type SmartRuleError =
  | 'valueRequired'
  | 'valueTooLong'
  | 'numberInvalid'
  | 'ratingInvalid'
  | 'rangeInvalid'
  | 'daysInvalid'
  | 'limitInvalid'
  | 'tooManyRules'
  | 'playlistRequired';

export type SmartRulesResult =
  | { rules: SmartRules; error: null }
  | { rules: null; error: SmartRuleError; ruleKey: number | null };

/** 0以上の整数を読む（読めなければnull） */
function parseCount(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isSafeInteger(value) ? value : null;
}

/** 数値の項目の値を読む（長さは分、サンプルレートはkHzで入力し、秒・Hzにする。読めなければnull） */
function parseNumber(field: NumberField, text: string): number | null {
  const input = RULE_FIELDS[field].input;
  if (input !== 'minutes' && input !== 'kilohertz') return parseCount(text);
  const trimmed = text.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) return null;
  return Math.round(Number(trimmed) * (input === 'minutes' ? 60 : 1000));
}

function toRule(draft: RuleDraft): SmartRule | SmartRuleError {
  const definition = RULE_FIELDS[draft.field];
  switch (definition.kind) {
    case 'text': {
      const value = needsValue(draft) ? draft.value.trim() : '';
      if (needsValue(draft) && value === '') return 'valueRequired';
      if ([...value].length > MAX_TEXT_LENGTH) return 'valueTooLong';
      return { kind: 'text', field: draft.field as TextField, op: draft.op as TextOp, value };
    }
    case 'number': {
      const field = draft.field as NumberField;
      const op = draft.op as NumberOp;
      const max = definition.input === 'rating' ? MAX_RATING : MAX_NUMBER;
      const invalid = definition.input === 'rating' ? 'ratingInvalid' : 'numberInvalid';
      if (draft.value.trim() === '') return 'valueRequired';
      const value = parseNumber(field, draft.value);
      if (value === null || value > max) return invalid;
      if (op !== 'between') return { kind: 'number', field, op, value, valueTo: null };

      if (draft.valueTo.trim() === '') return 'valueRequired';
      const valueTo = parseNumber(field, draft.valueTo);
      if (valueTo === null || valueTo > max) return invalid;
      if (valueTo < value) return 'rangeInvalid';
      return { kind: 'number', field, op, value, valueTo };
    }
    case 'date': {
      const field = draft.field as DateField;
      const op = draft.op as DateOp;
      if (!needsValue(draft)) return { kind: 'date', field, op, days: 0 };
      if (draft.value.trim() === '') return 'valueRequired';
      const days = parseCount(draft.value);
      if (days === null || days < 1 || days > MAX_DAYS) return 'daysInvalid';
      return { kind: 'date', field, op, days };
    }
    case 'favorite':
      return { kind: 'favorite', value: draft.op === 'is' };
    case 'inMusicFolder':
      return { kind: 'inMusicFolder', value: draft.op === 'is' };
    case 'missing':
      return { kind: 'missing', value: draft.op === 'is' };
    case 'key': {
      const op = draft.op as KeyOp;
      if (!needsValue(draft)) return { kind: 'key', op, value: 0 };
      const value = parseCount(draft.value);
      if (value === null || value > MAX_KEY) return 'numberInvalid';
      return { kind: 'key', op, value };
    }
    case 'playlist':
      if (draft.value === '') return 'playlistRequired';
      return { kind: 'playlist', op: draft.op as FavoriteOp, playlistId: draft.value };
  }
}

/** 編集画面の入力 */
export interface SmartRulesForm {
  matchMode: MatchMode;
  drafts: readonly RuleDraft[];
  /** 曲数を制限するか */
  isLimited: boolean;
  /** 曲数の上限（入力した文字列） */
  limit: string;
  order: SmartOrder;
}

/**
 * 編集画面の入力を、保存する条件にする
 *
 * 入力に誤りがある場合は、最初の誤りと、その条件の番号（`RuleDraft.key`）を返す。
 */
export function toSmartRules(form: SmartRulesForm): SmartRulesResult {
  if (form.drafts.length > MAX_RULES) return { rules: null, error: 'tooManyRules', ruleKey: null };

  const rules: SmartRule[] = [];
  for (const draft of form.drafts) {
    const rule = toRule(draft);
    if (typeof rule === 'string') return { rules: null, error: rule, ruleKey: draft.key };
    rules.push(rule);
  }

  let limit: number | null = null;
  if (form.isLimited) {
    limit = parseCount(form.limit);
    if (limit === null || limit < 1 || limit > MAX_LIMIT) {
      return { rules: null, error: 'limitInvalid', ruleKey: null };
    }
  }

  return {
    rules: {
      // 条件がない場合は、すべての曲を対象にする（「いずれか」のままだと、1曲も合わない）
      matchMode: rules.length === 0 ? 'all' : form.matchMode,
      rules,
      order: {
        field: form.order.field,
        descending: isDirected(form.order.field) && form.order.descending
      },
      limit
    },
    error: null
  };
}

/** 昇順・降順を選べる並び順か（ランダムには向きがない） */
export const isDirected = (field: SmartOrderField) => field !== 'random';

/**
 * 並び順の項目を選んだ時の、最初の向き
 *
 * 名前は昇順、数値・日付は降順（新しい順・多い順）から始める。
 */
export function initialOrder(field: SmartOrderField): SmartOrder {
  const isName = field === 'title' || field === 'artist' || field === 'album';
  return { field, descending: isDirected(field) && !isName };
}

/** 新しい自動プレイリストの、最初の入力 */
export function newSmartRulesForm(): SmartRulesForm {
  return {
    matchMode: 'all',
    drafts: [newRuleDraft(0)],
    isLimited: false,
    limit: '25',
    order: initialOrder('artist')
  };
}

/** 保存してある条件を、編集画面の入力にする */
export function toSmartRulesForm(rules: SmartRules): SmartRulesForm {
  return {
    matchMode: rules.matchMode,
    drafts: toRuleDrafts(rules.rules),
    isLimited: rules.limit !== null,
    limit: String(rules.limit ?? 25),
    order: { ...rules.order }
  };
}

/** 項目の表示名 */
export function ruleFieldLabel(field: RuleField): string {
  switch (field) {
    case 'path':
      return m.smartPlaylist.pathField;
    case 'inMusicFolder':
      return m.smartPlaylist.inMusicFolderField;
    case 'playlist':
      return m.smartPlaylist.playlistField;
    case 'missing':
      return m.smartPlaylist.missingField;
    default:
      return m.fields[field];
  }
}

/** 並び順の項目の表示名 */
export function orderFieldLabel(field: SmartOrderField): string {
  return field === 'random' ? m.smartPlaylist.random : m.fields[field];
}

/** 比べ方の表示名 */
export function ruleOpLabel(field: RuleField, op: RuleOp): string {
  const labels = m.smartPlaylist.ops[RULE_FIELDS[field].kind] as Record<string, string>;
  return labels[op];
}

/** 数値の条件の値の単位（入力欄の後ろと、説明文に出す。単位のない項目はnull） */
export function numberUnit(field: NumberField): string | null {
  switch (field) {
    case 'bitrate':
      return 'kbps';
    case 'sampleRate':
      return 'kHz';
    case 'bitDepth':
      return 'bit';
    default:
      return null;
  }
}

/** 数値の条件の値を、説明文に出す形にする（評価は星、長さは分、サンプルレートはkHz） */
function describeNumber(field: NumberField, value: number): string {
  switch (RULE_FIELDS[field].input) {
    case 'rating':
      return value === 0 ? m.smartPlaylist.noRating : formatRatingStars(value);
    case 'minutes':
      return m.smartPlaylist.minutes(toMinutesText(value));
    default: {
      const unit = numberUnit(field);
      const text = numberText(field, value);
      return unit === null ? text : `${text} ${unit}`;
    }
  }
}

/**
 * 条件の説明（プレイリストの画面に出す。例: 「ジャンル: 「Jazz」を含む」）
 *
 * @param playlistName プレイリストのIDから名前を求める（プレイリストの条件に使う。消した
 *   プレイリストはnull）
 */
export function describeRule(
  rule: SmartRule,
  playlistName: (playlistId: string) => string | null = () => null
): string {
  const describe = m.smartPlaylist.describe;
  switch (rule.kind) {
    case 'text':
      return describe.rule(ruleFieldLabel(rule.field), describe.text[rule.op](rule.value));
    case 'number': {
      const value = describeNumber(rule.field, rule.value);
      const valueTo = describeNumber(rule.field, rule.valueTo ?? rule.value);
      return describe.rule(ruleFieldLabel(rule.field), describe.number[rule.op](value, valueTo));
    }
    case 'date':
      return describe.rule(ruleFieldLabel(rule.field), describe.date[rule.op](rule.days));
    case 'favorite':
      return describe.favorite(rule.value);
    case 'inMusicFolder':
      return describe.inMusicFolder(rule.value);
    case 'missing':
      return describe.missing(rule.value);
    case 'key':
      return describe.rule(
        ruleFieldLabel('analyzedKey'),
        describe.key[rule.op](formatMusicKey(rule.value))
      );
    case 'playlist':
      return describe.playlist(
        playlistName(rule.playlistId) ?? m.smartPlaylist.deletedPlaylist,
        rule.op === 'is'
      );
  }
}

/** 並び順と曲数の上限の説明（例: 「再生回数の多い順に25曲まで」） */
export function describeOrder(rules: Pick<SmartRules, 'order' | 'limit'>): string {
  const describe = m.smartPlaylist.describe;
  const order = isDirected(rules.order.field)
    ? describe.order(orderFieldLabel(rules.order.field), rules.order.descending)
    : m.smartPlaylist.random;
  return rules.limit === null ? order : describe.limited(order, rules.limit);
}
