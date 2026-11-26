/**
 * 転送先デバイスの表示用のユーティリティ
 */
import type {
  DeviceSyncPlan,
  DeviceSyncProgress,
  SyncDevice,
  SyncDeviceConfig
} from '#lib/types/models.js';

/** デバイス名の長さの上限（Rust側と同じく、文字数で数える） */
const MAX_DEVICE_NAME_LENGTH = 100;

/**
 * デバイスの今の設定（`update_sync_device`に渡す形）。一部を変える時は、これに変える項目を重ねる
 * （コマンドは設定の全体を受け取るため）
 */
export function deviceConfigOf(
  device: SyncDevice,
  patch: Partial<SyncDeviceConfig> = {}
): SyncDeviceConfig {
  return {
    name: device.name,
    syncAll: device.syncAll,
    playlistIds: device.playlistIds,
    smartPlaylistIds: device.smartPlaylistIds,
    removeUnselected: device.removeUnselected,
    pathTemplate: device.pathTemplate,
    copyExtras: device.copyExtras,
    transcode: device.transcode,
    playlistFormat: device.playlistFormat,
    ...patch
  };
}

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
  plan: Pick<
    DeviceSyncPlan,
    'copyCount' | 'deleteCount' | 'renameCount' | 'extraCopyCount' | 'extraDeleteCount'
  >
): boolean {
  return (
    plan.copyCount === 0 &&
    plan.deleteCount === 0 &&
    plan.renameCount === 0 &&
    plan.extraCopyCount === 0 &&
    plan.extraDeleteCount === 0
  );
}

/** 空き容量が足りない場合に、あと必要な容量（バイト）。足りていれば0 */
export function missingSpaceBytes(
  plan: Pick<DeviceSyncPlan, 'requiredBytes' | 'freeBytes' | 'hasEnoughSpace'>
): number {
  return plan.hasEnoughSpace ? 0 : Math.max(plan.requiredBytes - plan.freeBytes, 0);
}
