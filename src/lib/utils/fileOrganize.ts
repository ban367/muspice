/**
 * ファイルの整理（タグに基づくリネーム・移動）の画面の計算
 *
 * 整理後の場所を決めるのは、バックエンド（Rustの`file_organize.rs`）。ここでは、書式の候補と、
 * 返った計画の集計・前回の書式の記憶だけを扱う。
 */
import type { OrganizeEntry, OrganizeStatus } from '#lib/types/models.js';

/** 書式の候補（画面の選択肢に出す。先頭が既定） */
export const FILE_ORGANIZE_PRESETS = [
  '%albumartist%/%album%/%track% %title%',
  '%albumartist%/%album%/[%disc%-]%track% %title%',
  '%albumartist%/[%year% ]%album%/%track% %title%',
  '%albumartist%/%album%/%track% - %title%',
  '%albumartist% - %album%/%track% %title%',
  '%genre%/%albumartist%/%album%/%track% %title%',
  '[%track% ]%title%',
  '%artist% - %title%'
] as const;

export const DEFAULT_FILE_ORGANIZE_TEMPLATE: string = FILE_ORGANIZE_PRESETS[0];

/** 動かさない理由（`Move`・`Unchanged`を除いたもの） */
export type OrganizeBlockReason = Exclude<OrganizeStatus, 'move' | 'unchanged'>;

/** 動かさない曲 */
export interface BlockedEntry extends OrganizeEntry {
  status: OrganizeBlockReason;
}

/** 計画の集計 */
export interface OrganizeSummary {
  /** 動かす曲 */
  moves: OrganizeEntry[];
  /** 動かさない曲（理由のあるもの） */
  blocked: BlockedEntry[];
  /** すでに書式どおりの場所にある曲の数 */
  unchangedCount: number;
}

const isBlocked = (entry: OrganizeEntry): entry is BlockedEntry =>
  entry.status !== 'move' && entry.status !== 'unchanged';

/** 計画を、動かす曲・動かさない曲・そのままの曲に分ける */
export function summarizeOrganizePlan(entries: readonly OrganizeEntry[]): OrganizeSummary {
  return {
    moves: entries.filter((entry) => entry.status === 'move'),
    blocked: entries.filter(isBlocked),
    unchangedCount: entries.filter((entry) => entry.status === 'unchanged').length
  };
}

const TEMPLATE_KEY = 'muspice:fileOrganizeTemplate';

/** 前回使った書式（なければ、既定の書式） */
export function loadOrganizeTemplate(): string {
  try {
    const stored = localStorage.getItem(TEMPLATE_KEY);
    return stored !== null && stored.trim() !== '' ? stored : DEFAULT_FILE_ORGANIZE_TEMPLATE;
  } catch {
    return DEFAULT_FILE_ORGANIZE_TEMPLATE;
  }
}

/** 使った書式を覚える（保存できなくても動作には影響しない） */
export function saveOrganizeTemplate(template: string): void {
  try {
    localStorage.setItem(TEMPLATE_KEY, template);
  } catch {
    // 保存できなくても動作には影響しない
  }
}
