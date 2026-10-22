/**
 * 転送先デバイスの表示用のユーティリティ
 */
import type { DeviceSyncPlan, DeviceSyncProgress } from '#lib/types/models.js';

/** デバイス名の長さの上限（Rust側と同じく、文字数で数える） */
const MAX_DEVICE_NAME_LENGTH = 100;

/** デバイス名が長さの上限を超えているか */
export function isDeviceNameTooLong(name: string): boolean {
  return [...name.trim()].length > MAX_DEVICE_NAME_LENGTH;
}

/** 同期の進捗の割合（0〜100の整数。バイト数で数え、サイズが分からなければファイル数で数える） */
export function syncProgressPercent(
  progress: Pick<DeviceSyncProgress, 'current' | 'total' | 'bytesDone' | 'bytesTotal'>
): number {
  const ratio =
    progress.bytesTotal > 0
      ? progress.bytesDone / progress.bytesTotal
      : progress.total > 0
        ? progress.current / progress.total
        : 1;
  return Math.round(Math.min(Math.max(ratio, 0), 1) * 100);
}

/** 同期しても、デバイス上の曲が変わらないか（コピー・削除・名前の変更がない） */
export function isDeviceUpToDate(
  plan: Pick<DeviceSyncPlan, 'copyCount' | 'deleteCount' | 'renameCount'>
): boolean {
  return plan.copyCount === 0 && plan.deleteCount === 0 && plan.renameCount === 0;
}

/** 空き容量が足りない場合に、あと必要な容量（バイト）。足りていれば0 */
export function missingSpaceBytes(
  plan: Pick<DeviceSyncPlan, 'requiredBytes' | 'freeBytes' | 'hasEnoughSpace'>
): number {
  return plan.hasEnoughSpace ? 0 : Math.max(plan.requiredBytes - plan.freeBytes, 0);
}
