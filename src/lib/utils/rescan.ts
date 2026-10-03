/**
 * ライブラリフォルダの再スキャン結果の集計と通知文
 */
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
  const changes = [
    totals.addedCount > 0 ? `追加 ${totals.addedCount}曲` : null,
    totals.updatedCount > 0 ? `更新 ${totals.updatedCount}曲` : null,
    totals.removedCount > 0 ? `削除 ${totals.removedCount}曲` : null
  ].filter((change) => change !== null);
  return changes.length > 0
    ? `再スキャンしました（${changes.join('・')}）`
    : '再スキャンしました（変更はありませんでした）';
}
