/**
 * ライブラリフォルダの再スキャン結果の集計と通知文
 */
import { m } from '#lib/i18n/i18n.svelte.js';
import type { RescanResult } from '#lib/types/models.js';

/** 複数フォルダの再スキャン結果の合計 */
export interface RescanTotals {
  addedCount: number;
  updatedCount: number;
  removedCount: number;
  errorCount: number;
  /** 音楽ファイルが見つからず、ライブラリから曲を外さなかったフォルダのパス */
  removalSkippedPaths: string[];
}

/** フォルダごとの再スキャン結果を合計する */
export function sumRescanResults(results: { path: string; result: RescanResult }[]): RescanTotals {
  const totals: RescanTotals = {
    addedCount: 0,
    updatedCount: 0,
    removedCount: 0,
    errorCount: 0,
    removalSkippedPaths: []
  };
  for (const { path, result } of results) {
    totals.addedCount += result.addedCount;
    totals.updatedCount += result.updatedCount;
    totals.removedCount += result.removedCount;
    totals.errorCount += result.errorCount;
    if (result.removalSkipped) totals.removalSkippedPaths.push(path);
  }
  return totals;
}

/** 再スキャンの結果の通知文（変更がなければ「変更はありませんでした」） */
export function describeRescan(totals: RescanTotals): string {
  const messages = m.libraryFolders;
  const changes = [
    totals.addedCount > 0 ? messages.added(totals.addedCount) : null,
    totals.updatedCount > 0 ? messages.updated(totals.updatedCount) : null,
    totals.removedCount > 0 ? messages.removedTracks(totals.removedCount) : null
  ].filter((change) => change !== null);
  return messages.rescanned(changes);
}
