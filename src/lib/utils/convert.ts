/**
 * 形式の変換の画面の計算
 *
 * 出力先を決めるのは、バックエンド（Rustの`transcode.rs`）。ここでは、返った計画の集計・
 * 音質が戻らない変換の注意・前回の指定の記憶だけを扱う。
 */
import { DEFAULT_AAC_BITRATE_KBPS } from '#lib/bindings.js';
import type {
  ConvertEntry,
  ConvertFormat,
  ConvertMode,
  ConvertOptions,
  ConvertStatus
} from '#lib/types/models.js';
import { DEFAULT_FILE_ORGANIZE_TEMPLATE } from './fileOrganize.js';

/** 非可逆の形式（変換すると、元の音には戻らない） */
const LOSSY_FORMATS: readonly ConvertFormat[] = ['aac'];

/** 変換先が、非可逆の形式か */
export function isLossyFormat(format: ConvertFormat): boolean {
  return LOSSY_FORMATS.includes(format);
}

/** 変換しない理由（`convert`を除いたもの） */
export type ConvertBlockReason = Exclude<ConvertStatus, 'convert'>;

/** 変換しない曲 */
export interface BlockedConvertEntry extends ConvertEntry {
  status: ConvertBlockReason;
}

/** 音質が戻らない変換の注意 */
export interface ConvertWarnings {
  /** 非可逆の形式の曲を、可逆の形式にする曲の数（音質は戻らず、ファイルが大きくなるだけ） */
  lossyToLossless: number;
  /** 非可逆の形式の曲を、もう一度非可逆の形式にする曲の数（音質が下がる） */
  lossyToLossy: number;
  /** 可逆の形式の曲を、非可逆の形式に置き換える曲の数（元の音質には戻せない） */
  losslessReplaced: number;
}

/** 計画の集計 */
export interface ConvertSummary {
  /** 変換する曲 */
  conversions: ConvertEntry[];
  /** 変換しない曲（理由のあるもの） */
  blocked: BlockedConvertEntry[];
  warnings: ConvertWarnings;
}

const isBlocked = (entry: ConvertEntry): entry is BlockedConvertEntry => entry.status !== 'convert';

/** 計画を、変換する曲・変換しない曲に分け、音質が戻らない変換を数える */
export function summarizeConvertPlan(
  entries: readonly ConvertEntry[],
  format: ConvertFormat,
  mode: ConvertMode
): ConvertSummary {
  const conversions = entries.filter((entry) => entry.status === 'convert');
  const lossy = isLossyFormat(format);
  // 可逆かどうかが分からない曲は、数えない
  const count = (sourceLossless: boolean) =>
    conversions.filter((entry) => entry.sourceLossless === sourceLossless).length;
  return {
    conversions,
    blocked: entries.filter(isBlocked),
    warnings: {
      lossyToLossless: lossy ? 0 : count(false),
      lossyToLossy: lossy ? count(false) : 0,
      losslessReplaced: lossy && mode === 'replace' ? count(true) : 0
    }
  };
}

/** 既定の指定（AACへ書き出す） */
export const DEFAULT_CONVERT_OPTIONS: ConvertOptions = {
  format: 'aac',
  aacBitrateKbps: DEFAULT_AAC_BITRATE_KBPS,
  mode: 'export',
  template: DEFAULT_FILE_ORGANIZE_TEMPLATE,
  addToLibrary: false
};

const OPTIONS_KEY = 'muspice:convertOptions';

/**
 * 前回の指定（なければ、既定の指定）
 *
 * 出力先（書き出し・置き換え）は覚えず、いつも書き出しから始める（置き換えは、元のファイルを
 * ゴミ箱へ移す操作のため、毎回選ばせる）。
 * @param formats - この環境で変換先にできる形式（覚えていた形式が使えなければ、最初の形式にする）
 */
export function loadConvertOptions(formats: readonly ConvertFormat[]): ConvertOptions {
  let stored: Partial<ConvertOptions> = {};
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? 'null');
    if (typeof parsed === 'object' && parsed !== null) stored = parsed;
  } catch {
    // 読めなければ、既定の指定を使う
  }
  const format =
    formats.find((candidate) => candidate === stored.format) ??
    formats.find((candidate) => candidate === DEFAULT_CONVERT_OPTIONS.format) ??
    formats[0] ??
    DEFAULT_CONVERT_OPTIONS.format;
  return {
    format,
    aacBitrateKbps:
      typeof stored.aacBitrateKbps === 'number'
        ? stored.aacBitrateKbps
        : DEFAULT_CONVERT_OPTIONS.aacBitrateKbps,
    mode: 'export',
    template:
      typeof stored.template === 'string' && stored.template.trim() !== ''
        ? stored.template
        : DEFAULT_CONVERT_OPTIONS.template,
    addToLibrary: stored.addToLibrary === true
  };
}

/** 使った指定を覚える（保存できなくても動作には影響しない） */
export function saveConvertOptions(options: ConvertOptions): void {
  try {
    localStorage.setItem(OPTIONS_KEY, JSON.stringify(options));
  } catch {
    // 保存できなくても動作には影響しない
  }
}
