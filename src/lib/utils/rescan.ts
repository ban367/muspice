/**
 * ライブラリフォルダの再スキャン結果の集計と通知文
 */
import { m } from '#lib/i18n/i18n.svelte.js';
import type { RescanResult } from '#lib/types/models.js';

/** 複数フォルダの再スキャン結果の合計 */
export interface RescanTotals {
  addedCount: number;
  updatedCount: number;
  /** 移動・改名されたファイルに結び付けたトラック数 */
  relinkedCount: number;
  /** ファイルが見つからなくなり、見つからない曲にしたトラック数 */
  missingCount: number;
  /** ファイルが同じ場所に戻り、見つかる曲に戻したトラック数 */
  restoredCount: number;
  errorCount: number;
  /** 音楽ファイルが1件も見つからず、見つからない曲にしなかったフォルダのパス */
  missingSkippedPaths: string[];
}

/** フォルダごとの再スキャン結果を合計する */
export function sumRescanResults(results: { path: string; result: RescanResult }[]): RescanTotals {
  const totals: RescanTotals = {
    addedCount: 0,
    updatedCount: 0,
    relinkedCount: 0,
    missingCount: 0,
    restoredCount: 0,
    errorCount: 0,
    missingSkippedPaths: []
  };
  for (const { path, result } of results) {
    totals.addedCount += result.addedCount;
    totals.updatedCount += result.updatedCount;
    totals.relinkedCount += result.relinkedCount;
    totals.missingCount += result.missingCount;
    totals.restoredCount += result.restoredCount;
    totals.errorCount += result.errorCount;
    if (result.missingSkipped) totals.missingSkippedPaths.push(path);
  }
  return totals;
}

/** 再スキャンの結果の通知文（変更がなければ「変更はありませんでした」） */
export function describeRescan(totals: RescanTotals): string {
  const messages = m.libraryFolders;
  const changes = [
    totals.addedCount > 0 ? messages.added(totals.addedCount) : null,
    totals.updatedCount > 0 ? messages.updated(totals.updatedCount) : null,
    totals.relinkedCount > 0 ? messages.relinked(totals.relinkedCount) : null,
    totals.restoredCount > 0 ? messages.restored(totals.restoredCount) : null,
    totals.missingCount > 0 ? messages.markedMissing(totals.missingCount) : null
  ].filter((change) => change !== null);
  return messages.rescanned(changes);
}
