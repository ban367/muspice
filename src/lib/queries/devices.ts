/**
 * 転送先デバイス（SDカードなどのフォルダ）のクエリ・ミューテーション
 *
 * デバイスの一覧には、接続されているか（転送先のフォルダに管理ファイルがあるか）と
 * 空き容量が含まれる。どちらもアプリの外で変わるため、ウィンドウに戻ったとき
 * （`Sidebar`）と、デバイスのページを開いている間は一定の間隔で読み直す。
 */

import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { SyncDevice, SyncDeviceConfig } from '#lib/types/models.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';

/** デバイスのページを開いている間に、接続の状態を読み直す間隔（ms） */
const CONNECTION_POLL_INTERVAL_MS = 5000;

/** ロールバック用に直前のデバイス一覧を保持するコンテキスト */
interface DevicesSnapshot {
  previousDevices: SyncDevice[] | undefined;
}

/** デバイスの設定の更新内容 */
interface UpdateDeviceVariables {
  deviceId: string;
  config: SyncDeviceConfig;
}

/**
 * 転送先デバイスの一覧を取得するクエリ
 *
 * @param options.poll 一定の間隔で読み直す（デバイスの抜き差しを反映する。デバイスのページ用）
 */
export function useSyncDevicesQuery(options: { poll?: boolean } = {}) {
  return createQuery(() => ({
    queryKey: queryKeys.syncDevices,
    queryFn: () => withErrorToast(m.operations.fetchDevices, () => commands.getSyncDevices()),
    ...(options.poll
      ? { refetchInterval: CONNECTION_POLL_INTERVAL_MS, refetchOnMount: 'always' as const }
      : {})
  }));
}

/**
 * フォルダを転送先デバイスとして登録するミューテーション
 */
export function useRegisterSyncDeviceMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ folderPath, name }: { folderPath: string; name: string }) =>
      withErrorToast(m.operations.registerDevice, () =>
        commands.registerSyncDevice(folderPath, name)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices })
  }));
}

/**
 * デバイスの設定（名前と同期する対象）を更新するミューテーション（Optimistic Update付き）
 */
export function useUpdateSyncDeviceMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ deviceId, config }: UpdateDeviceVariables) =>
      withErrorToast(m.operations.updateDevice, () => commands.updateSyncDevice(deviceId, config)),
    onMutate: async ({ deviceId, config }: UpdateDeviceVariables): Promise<DevicesSnapshot> => {
      // 進行中のクエリをキャンセル（後から古い結果で上書きされるのを防ぐ）
      await queryClient.cancelQueries({ queryKey: queryKeys.syncDevices });
      const previousDevices = queryClient.getQueryData<SyncDevice[]>(queryKeys.syncDevices);
      if (previousDevices) {
        queryClient.setQueryData<SyncDevice[]>(
          queryKeys.syncDevices,
          previousDevices.map((device) =>
            device.id === deviceId ? { ...device, ...config } : device
          )
        );
      }
      return { previousDevices };
    },
    onError: (
      _error: unknown,
      _variables: UpdateDeviceVariables,
      context: DevicesSnapshot | undefined
    ) => {
      if (context?.previousDevices) {
        queryClient.setQueryData(queryKeys.syncDevices, context.previousDevices);
      }
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices })
  }));
}

/**
 * デバイスの転送先のフォルダを変えるミューテーション
 */
export function useRelinkSyncDeviceMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ deviceId, folderPath }: { deviceId: string; folderPath: string }) =>
      withErrorToast(m.operations.relinkDevice, () =>
        commands.relinkSyncDevice(deviceId, folderPath)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices })
  }));
}

/**
 * デバイスの登録を解除するミューテーション（デバイス上のファイルは消さない）
 */
export function useRemoveSyncDeviceMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (deviceId: string) =>
      withErrorToast(m.operations.removeDevice, () => commands.removeSyncDevice(deviceId)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices })
  }));
}

/**
 * 同期で行う処理の件数と必要な容量を調べるミューテーション
 *
 * 結果は同期の直前の確認にだけ使うため、キャッシュしない。エラーは呼び出し側
 * （`DeviceSyncDialog`）がダイアログの中に表示する。
 */
export function usePlanDeviceSyncMutation() {
  return createMutation(() => ({
    mutationFn: (deviceId: string) => commands.planDeviceSync(deviceId)
  }));
}

/**
 * デバイスへ同期するミューテーション
 *
 * エラーは呼び出し側（`DeviceSyncDialog`）がダイアログの中に表示する。
 */
export function useRunDeviceSyncMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (deviceId: string) => commands.runDeviceSync(deviceId),
    // 失敗・中止した場合も途中までは反映されているため、空き容量などを読み直す
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices })
  }));
}

/**
 * 実行中の同期を中止する
 */
export function cancelDeviceSync(): Promise<null> {
  return withErrorToast(m.operations.cancelDeviceSync, () => commands.cancelDeviceSync());
}
