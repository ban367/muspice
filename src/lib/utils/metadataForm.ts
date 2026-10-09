/**
 * メタデータの編集画面のフォーム
 *
 * 入力欄の値（文字列）と、コマンドへ渡す`Metadata`の間の変換と、入力の検証を行う。
 * バックエンドも同じ検証（数値の範囲・文字数）を行うため、ここでは保存の前に分かる誤りを
 * 入力中に示す目的で検証する。
 *
 * - 1曲の編集: すべての項目の値を渡す。空にした項目は、ファイルのタグから取り除かれる
 * - 一括編集: 入力した項目だけを渡す（空の項目は変えない）。タイトル・トラック番号・歌詞は扱わない
 */
import type { Metadata } from '#lib/types/models.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/** 文字列の項目 */
export const TEXT_FIELDS = [
  'title',
  'artist',
  'album',
  'albumArtist',
  'composer',
  'genre',
  'grouping',
  'comment',
  'lyrics'
] as const;
export type TextField = (typeof TEXT_FIELDS)[number];

/** 数値の項目（入力欄の値は文字列で持つ。空は値なし） */
export const NUMBER_FIELDS = [
  'year',
  'trackNumber',
  'trackTotal',
  'discNumber',
  'discTotal',
  'bpm'
] as const;
export type NumberField = (typeof NUMBER_FIELDS)[number];

/** コンピレーションの印（一括編集では、変えない・付ける・外すを選ぶ） */
export type CompilationChoice = 'unchanged' | 'on' | 'off';

export type MetadataForm = Record<TextField | NumberField, string> & {
  compilation: CompilationChoice;
};

/** 文字列の項目の長さの上限（文字数。Rustの`validate_metadata_input`と同じ） */
export const TEXT_LIMITS: Record<TextField, number> = {
  title: 255,
  artist: 255,
  album: 255,
  albumArtist: 255,
  composer: 255,
  genre: 100,
  grouping: 255,
  comment: 2000,
  lyrics: 50000
};

/** 数値の項目の範囲（Rustの`validate_metadata`と同じ） */
export const NUMBER_RANGES: Record<NumberField, readonly [number, number]> = {
  year: [1000, 9999],
  trackNumber: [1, 999],
  trackTotal: [1, 999],
  discNumber: [1, 999],
  discTotal: [1, 999],
  bpm: [1, 999]
};

/** 一括編集で扱わない項目（曲ごとに違う値になる項目） */
const SINGLE_ONLY_FIELDS: ReadonlySet<TextField | NumberField> = new Set([
  'title',
  'trackNumber',
  'lyrics'
]);

/** 何も入力していないフォーム（一括編集の初期値） */
export function emptyForm(): MetadataForm {
  const form = { compilation: 'unchanged' } as MetadataForm;
  for (const field of [...TEXT_FIELDS, ...NUMBER_FIELDS]) form[field] = '';
  return form;
}

/** ファイルから読んだタグを、フォームの値にする（1曲の編集の初期値） */
export function formFromTags(tags: Metadata): MetadataForm {
  const form = emptyForm();
  for (const field of TEXT_FIELDS) form[field] = tags[field] ?? '';
  for (const field of NUMBER_FIELDS) form[field] = tags[field]?.toString() ?? '';
  form.compilation = tags.compilation ? 'on' : 'off';
  return form;
}

/** 数値の入力欄の値を読む（空は値なし。整数でなければNaN） */
function parseNumber(value: string): number | undefined {
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  return /^\d+$/.test(trimmed) ? Number(trimmed) : NaN;
}

/**
 * フォームの入力を検証し、誤りの文言を返す（誤りがなければ空）
 * @param labels 項目の表示名（誤りの文言に使う）
 */
export function validateForm(
  form: MetadataForm,
  labels: Record<TextField | NumberField, string>
): string[] {
  const errors: string[] = [];
  for (const field of TEXT_FIELDS) {
    // 文字数は、Rust側と同じくコードポイントで数える（絵文字などを2文字と数えない）
    const length = [...form[field].trim()].length;
    if (length > TEXT_LIMITS[field]) {
      errors.push(m.validation.tooLong(labels[field], TEXT_LIMITS[field], length));
    }
  }
  for (const field of NUMBER_FIELDS) {
    const value = parseNumber(form[field]);
    const [min, max] = NUMBER_RANGES[field];
    if (value !== undefined && !(value >= min && value <= max)) {
      errors.push(m.validation.numberOutOfRange(labels[field], min, max));
    }
  }
  return errors;
}

/**
 * フォームの値を、コマンドへ渡す`Metadata`にする（検証を通った入力を渡す）
 *
 * - `single`: すべての項目を入れる。空の項目は値なし（タグから取り除かれる）
 * - `bulk`: 入力した項目だけを入れる（空の項目は変えない）。1曲ごとの項目は入れない
 */
export function metadataFromForm(form: MetadataForm, mode: 'single' | 'bulk'): Metadata {
  const metadata: Metadata = {};
  const included = (field: TextField | NumberField) =>
    mode === 'single' || !SINGLE_ONLY_FIELDS.has(field);

  for (const field of TEXT_FIELDS) {
    // 歌詞・コメントは、行頭の字下げなどを残すため、前後の空白を除かない
    const value = field === 'lyrics' || field === 'comment' ? form[field] : form[field].trim();
    if (included(field) && value.trim() !== '') metadata[field] = value;
  }
  for (const field of NUMBER_FIELDS) {
    const value = parseNumber(form[field]);
    if (included(field) && value !== undefined) metadata[field] = value;
  }
  if (mode === 'single') {
    // 印がなければ値なしにして、タグから取り除く
    if (form.compilation === 'on') metadata.compilation = true;
  } else if (form.compilation !== 'unchanged') {
    metadata.compilation = form.compilation === 'on';
  }
  return metadata;
}

/**
 * 入力中の値に対する候補を返す（ライブラリにある値のうち、入力を含むもの）
 *
 * 候補が多いと一覧の描画が重くなるため、先頭の一部だけを返す。入力が空・入力と同じ値だけの
 * 場合は、候補を出さない。
 */
export function suggestionsFor(
  values: readonly string[] | undefined,
  input: string,
  limit = 30
): string[] {
  const needle = input.trim().toLowerCase();
  if (!values || needle === '') return [];

  const matches: string[] = [];
  for (const value of values) {
    const lower = value.toLowerCase();
    if (lower !== needle && lower.includes(needle)) {
      matches.push(value);
      if (matches.length >= limit) break;
    }
  }
  return matches;
}
