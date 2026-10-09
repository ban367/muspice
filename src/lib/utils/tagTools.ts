/**
 * タグの一括ツール（ファイル名からの推定・連番の振り直し・検索と置換）
 *
 * 選んだ曲のタグを、規則に従ってまとめて書き換えるための計算を行う。どのツールも、
 * 曲ごとの変更（前 → 後）を返すだけで、書き込みはしない。画面は、返った変更を一覧で見せ、
 * 確認の後に`toMetadataChanges`でコマンドへ渡す形にして書き込む。
 */
import type { Metadata, Track, TrackMetadataChange } from '#lib/types/models.js';

/** 一括ツールで書き換える項目 */
export type ToolField =
  | 'title'
  | 'artist'
  | 'album'
  | 'albumArtist'
  | 'genre'
  | 'year'
  | 'trackNumber'
  | 'trackTotal'
  | 'discNumber';

/** 文字列の項目（検索と置換の対象にできる） */
export const TEXT_TOOL_FIELDS = ['title', 'artist', 'album', 'albumArtist', 'genre'] as const;
export type TextToolField = (typeof TEXT_TOOL_FIELDS)[number];

/** 文字列の項目の長さの上限（文字数。Rustの`validate_metadata_input`と同じ） */
const TEXT_LIMITS: Record<TextToolField, number> = {
  title: 255,
  artist: 255,
  album: 255,
  albumArtist: 255,
  genre: 100
};

/** 数値の項目の範囲（Rustの`validate_metadata`と同じ） */
const NUMBER_RANGES = {
  year: [1000, 9999],
  trackNumber: [1, 999],
  trackTotal: [1, 999],
  discNumber: [1, 999]
} as const;
type NumberToolField = keyof typeof NUMBER_RANGES;

const isTextField = (field: ToolField): field is TextToolField =>
  (TEXT_TOOL_FIELDS as readonly string[]).includes(field);

/** 1つの項目の変更 */
export interface FieldChange {
  field: ToolField;
  /** 今の値（値がない・分からない場合はnull。トラックの総数は、ライブラリに持っていない） */
  before: string | null;
  /** 変えた後の値（空は、項目を取り除く） */
  after: string;
}

/** 1曲の変更 */
export interface TrackChange {
  track: Track;
  fields: FieldChange[];
}

/** ツールの入力の誤り */
export type ToolError =
  | 'emptyPattern'
  | 'noPlaceholder'
  | 'unknownPlaceholder'
  | 'duplicatePlaceholder'
  | 'adjacentPlaceholders'
  | 'invalidRegex'
  | 'numberOutOfRange'
  | 'noFields'
  | 'tooLong';

/** ツールの結果 */
export interface ToolResult {
  /** 変更のある曲（変更のない曲は含めない） */
  changes: TrackChange[];
  /** 対象にならなかった曲の数（ファイル名が書式に合わない曲） */
  unmatchedCount: number;
  /** 入力の誤り（誤りがあれば、変更は空） */
  error: ToolError | null;
  /** 誤りの補足（知らない項目の名前など） */
  errorDetail?: string;
}

const failed = (error: ToolError, errorDetail?: string): ToolResult => ({
  changes: [],
  unmatchedCount: 0,
  error,
  errorDetail
});

/** 曲の今の値（文字列にしたもの。値がなければnull） */
function currentValue(track: Track, field: ToolField): string | null {
  // トラックの総数は、ファイルのタグだけにある（ライブラリには持っていない）
  if (field === 'trackTotal') return null;
  const value = track[field];
  return value === null ? null : String(value);
}

/** 今の値と違う項目だけを、変更として集める */
function collectChanges(track: Track, values: Partial<Record<ToolField, string>>): FieldChange[] {
  const fields: FieldChange[] = [];
  for (const [field, after] of Object.entries(values) as [ToolField, string][]) {
    const before = currentValue(track, field);
    if ((before ?? '') !== after || field === 'trackTotal') fields.push({ field, before, after });
  }
  return fields;
}

/** 変更の中に、長すぎる値がないかを確かめる */
function withLengthCheck(result: ToolResult): ToolResult {
  for (const change of result.changes) {
    for (const { field, after } of change.fields) {
      // 文字数は、Rust側と同じくコードポイントで数える
      if (isTextField(field) && [...after].length > TEXT_LIMITS[field]) return failed('tooLong');
    }
  }
  return result;
}

// ---------- ファイル名からの推定 ----------

/** 書式に書ける項目（`%名前%`）と、対応するタグの項目（`null`は読み飛ばす） */
const PLACEHOLDERS: Record<string, ToolField | null> = {
  title: 'title',
  artist: 'artist',
  album: 'album',
  albumartist: 'albumArtist',
  genre: 'genre',
  year: 'year',
  track: 'trackNumber',
  disc: 'discNumber',
  dummy: null
};

/** 書式の例（画面の選択肢に出す） */
export const FILE_NAME_PATTERN_PRESETS = [
  '%track% - %title%',
  '%track% %title%',
  '%track%. %title%',
  '%artist% - %title%',
  '%artist% - %album% - %track% - %title%',
  '%artist%/%album%/%track% - %title%',
  '%album%/%track% - %title%'
] as const;

/** 書式の1階層（ファイル名か、フォルダ名）を解釈したもの */
interface PatternSegment {
  regex: RegExp;
  /** 正規表現のグループの順に、対応する項目（`null`は読み飛ばす） */
  fields: (ToolField | null)[];
}

/** 解釈した書式 */
export interface FileNamePattern {
  /** 上のフォルダ → ファイル名の順 */
  segments: PatternSegment[];
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const isNumberField = (field: ToolField | null): field is NumberToolField =>
  field !== null && field in NUMBER_RANGES;

/** 書式の誤り（解釈の途中で投げ、`parseFileNamePattern`が結果に変える） */
class PatternError extends Error {
  constructor(
    readonly code: ToolError,
    readonly detail?: string
  ) {
    super(code);
  }
}

/** 書式を解釈する（誤りは`PatternError`を投げる） */
function parsePattern(pattern: string): FileNamePattern {
  const trimmed = pattern.trim();
  if (trimmed === '') throw new PatternError('emptyPattern');

  const used = new Set<ToolField>();
  const segments: PatternSegment[] = [];

  for (const segmentText of trimmed.split(/[/\\]/)) {
    const tokens = segmentText.split(/(%[^%]*%)/).filter((token) => token !== '');
    const fields: (ToolField | null)[] = [];
    let source = '^';
    let previousWasText = false;

    tokens.forEach((token, index) => {
      const name = /^%([^%]*)%$/.exec(token)?.[1];
      if (name === undefined) {
        source += escapeRegExp(token);
        previousWasText = false;
        return;
      }
      const key = name.toLowerCase();
      if (!(key in PLACEHOLDERS)) throw new PatternError('unknownPlaceholder', token);
      const field = PLACEHOLDERS[key];
      if (field !== null) {
        if (used.has(field)) throw new PatternError('duplicatePlaceholder', token);
        used.add(field);
      }
      // 文字列の項目の直後に別の項目が続くと、どこで分けるかが決まらない
      if (previousWasText) throw new PatternError('adjacentPlaceholders', token);

      const isLast = index === tokens.length - 1;
      if (isNumberField(field)) {
        source += '(\\d+)';
        previousWasText = false;
      } else {
        // 最後の項目は残りすべてに、途中の項目は次の文字までの最短に一致させる
        source += isLast ? '(.+)' : '(.+?)';
        previousWasText = true;
      }
      fields.push(field);
    });

    segments.push({ regex: new RegExp(`${source}$`, 'u'), fields });
  }

  if (used.size === 0) throw new PatternError('noPlaceholder');
  return { segments };
}

/**
 * ファイル名の書式（`%track% - %title%`など）を解釈する
 *
 * - `%名前%`が項目で、ほかの文字はそのまま一致させる。`%dummy%`は、読み飛ばす部分
 * - `/`で区切ると、上のフォルダの名前も使える（`%artist%/%album%/%track% - %title%`）
 * - 番号・年（`%track%`・`%disc%`・`%year%`）は数字だけに一致する
 */
export function parseFileNamePattern(
  pattern: string
): { ok: true; pattern: FileNamePattern } | { ok: false; error: ToolError; detail?: string } {
  try {
    return { ok: true, pattern: parsePattern(pattern) };
  } catch (error) {
    if (error instanceof PatternError) {
      return { ok: false, error: error.code, detail: error.detail };
    }
    throw error;
  }
}

/** 曲の場所を、書式と比べる部分（上のフォルダの名前 … ファイル名（拡張子なし））に分ける */
function pathParts(filePath: string): string[] {
  // macOSのファイル名は、濁点などが分かれた形（NFD）のことがあるため、合成した形にそろえる
  const parts = filePath.normalize('NFC').split(/[/\\]/);
  const fileName = parts.pop() ?? '';
  const dot = fileName.lastIndexOf('.');
  parts.push(dot > 0 ? fileName.slice(0, dot) : fileName);
  return parts;
}

/** 書式に合う曲から読み取った値（合わなければnull） */
function matchPath(
  filePath: string,
  pattern: FileNamePattern
): Partial<Record<ToolField, string>> | null {
  const parts = pathParts(filePath);
  const offset = parts.length - pattern.segments.length;
  if (offset < 0) return null;

  const values: Partial<Record<ToolField, string>> = {};
  for (const [index, segment] of pattern.segments.entries()) {
    const match = segment.regex.exec(parts[offset + index]);
    if (!match) return null;
    for (const [group, field] of segment.fields.entries()) {
      if (field === null) continue;
      const text = match[group + 1].trim();
      if (isNumberField(field)) {
        const [min, max] = NUMBER_RANGES[field];
        const number = Number(text);
        if (!(number >= min && number <= max)) return null;
        values[field] = String(number);
      } else {
        if (text === '') return null;
        values[field] = text;
      }
    }
  }
  return values;
}

/**
 * ファイル名・フォルダ名から、タグの値を読み取る
 *
 * 書式に合わない曲（区切りがない・番号が数字でないなど）は変えず、`unmatchedCount`に数える。
 */
export function guessFromFileName(tracks: readonly Track[], patternText: string): ToolResult {
  const parsed = parseFileNamePattern(patternText);
  if (!parsed.ok) return failed(parsed.error, parsed.detail);

  const changes: TrackChange[] = [];
  let unmatchedCount = 0;
  for (const track of tracks) {
    const values = matchPath(track.filePath, parsed.pattern);
    if (values === null) {
      unmatchedCount++;
      continue;
    }
    const fields = collectChanges(track, values);
    if (fields.length > 0) changes.push({ track, fields });
  }
  return withLengthCheck({ changes, unmatchedCount, error: null });
}

// ---------- 連番の振り直し ----------

export interface RenumberOptions {
  /** 最初の曲の番号 */
  start: number;
  /** トラックの総数（最後の曲の番号）も書き込むか */
  setTotal: boolean;
}

/**
 * トラック番号を、渡した曲の順に振り直す
 *
 * 総数を書き込む場合は、すべての曲に書き込む（今の総数はライブラリに持っていないため、
 * 同じ値かどうかは分からない）。
 */
export function renumberTracks(tracks: readonly Track[], options: RenumberOptions): ToolResult {
  const { start, setTotal } = options;
  const last = start + tracks.length - 1;
  const [min, max] = NUMBER_RANGES.trackNumber;
  if (!Number.isInteger(start) || start < min || last > max) return failed('numberOutOfRange');

  const changes: TrackChange[] = [];
  tracks.forEach((track, index) => {
    const values: Partial<Record<ToolField, string>> = { trackNumber: String(start + index) };
    if (setTotal) values.trackTotal = String(last);
    const fields = collectChanges(track, values);
    if (fields.length > 0) changes.push({ track, fields });
  });
  return { changes, unmatchedCount: 0, error: null };
}

// ---------- 検索と置換 ----------

/** 大文字・小文字の変換 */
export type CaseConversion = 'none' | 'upper' | 'lower' | 'title';

export interface ReplaceOptions {
  /** 対象の項目 */
  fields: readonly TextToolField[];
  /** 検索する文字列（空なら、置換をせずに変換・空白の削除だけを行う） */
  search: string;
  /** 置き換える文字列（正規表現では、`$1`などでグループを使える） */
  replace: string;
  /** 大文字と小文字を区別するか */
  matchCase: boolean;
  /** 検索する文字列を、正規表現として扱うか */
  useRegex: boolean;
  caseConversion: CaseConversion;
  /** 前後の空白を除くか */
  trim: boolean;
}

/** 単語の先頭を大文字に、ほかを小文字にする（アポストロフィの後は単語の途中として扱う） */
function toTitleCase(value: string): string {
  return value
    .toLowerCase()
    .replace(
      /(^|[^\p{L}\p{N}'’])(\p{L})/gu,
      (_, before: string, letter: string) => before + letter.toUpperCase()
    );
}

function convertCase(value: string, conversion: CaseConversion): string {
  switch (conversion) {
    case 'upper':
      return value.toUpperCase();
    case 'lower':
      return value.toLowerCase();
    case 'title':
      return toTitleCase(value);
    case 'none':
      return value;
  }
}

/**
 * 選んだ項目の値を、検索と置換・大文字と小文字の変換・前後の空白の削除で書き換える
 *
 * 置換 → 変換 → 空白の削除の順に行う。値のない項目は変えない。結果が空になった項目は、
 * タグから取り除く。
 */
export function searchAndReplace(tracks: readonly Track[], options: ReplaceOptions): ToolResult {
  if (options.fields.length === 0) return failed('noFields');

  let regex: RegExp | null = null;
  if (options.search !== '') {
    try {
      regex = new RegExp(
        options.useRegex ? options.search : escapeRegExp(options.search),
        options.matchCase ? 'gu' : 'giu'
      );
    } catch {
      return failed('invalidRegex');
    }
  }

  const transform = (value: string) => {
    let result = value;
    if (regex) {
      // 正規表現でなければ、置き換える文字列の`$`を特別扱いしない
      result = options.useRegex
        ? result.replace(regex, options.replace)
        : result.replace(regex, () => options.replace);
    }
    result = convertCase(result, options.caseConversion);
    return options.trim ? result.trim() : result;
  };

  const changes: TrackChange[] = [];
  for (const track of tracks) {
    const fields: FieldChange[] = [];
    for (const field of options.fields) {
      const before = track[field];
      if (before === null) continue;
      const after = transform(before);
      if (after !== before) fields.push({ field, before, after });
    }
    if (fields.length > 0) changes.push({ track, fields });
  }
  return withLengthCheck({ changes, unmatchedCount: 0, error: null });
}

// ---------- 書き込み ----------

/** 変更の件数（項目の数） */
export function countFieldChanges(changes: readonly TrackChange[]): number {
  return changes.reduce((total, change) => total + change.fields.length, 0);
}

/** 確認した変更を、コマンド（`apply_metadata_changes`）へ渡す形にする */
export function toMetadataChanges(changes: readonly TrackChange[]): TrackMetadataChange[] {
  return changes.map(({ track, fields }) => {
    const metadata: Metadata = {};
    for (const { field, after } of fields) {
      if (isTextField(field)) {
        // 空の値は、項目を取り除く指定
        metadata[field] = after;
      } else {
        metadata[field] = Number(after);
      }
    }
    return { trackId: track.id, metadata };
  });
}
