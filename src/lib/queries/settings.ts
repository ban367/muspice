/**
 * アプリケーション設定のクエリ・ミューテーション
 *
 * 設定はRust側（アプリデータ配下のsettings.json）に保存される。
 * 設定ウィンドウとメインウィンドウはQueryのキャッシュを共有しないため、
 * 保存時にRustが送る`SettingsChanged`イベントで他のウィンドウのキャッシュを更新する
 * （`SettingsSync`コンポーネント）。
 */

import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { Settings } from '#lib/types/models.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * 現在の設定を取得するクエリ
 *
 * 変更は保存またはイベントでキャッシュに反映するため、自動では再取得しない。
 */
export function useSettingsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.settings,
    queryFn: () => withErrorToast(m.operations.loadSettings, () => commands.getSettings()),
    staleTime: Infinity
  }));
}

/**
 * 出力デバイスの一覧を取得するクエリ（設定の「出力デバイス」の選択肢）
 *
 * デバイスはつないだり外したりで変わるため、キャッシュを使い回さず、表示するたび・
 * ウィンドウに戻るたびに取得し直す。
 * @param enabled 一覧を表示しているかを返す（リアクティブな値を読む）
 */
export function useOutputDevicesQuery(enabled: () => boolean) {
  return createQuery(() => ({
    queryKey: queryKeys.outputDevices,
    queryFn: () =>
      withErrorToast(m.operations.loadOutputDevices, () => commands.getOutputDevices()),
    enabled: enabled(),
    staleTime: 0,
    refetchOnWindowFocus: true
  }));
}

/**
 * 設定を保存するミューテーション
 */
export function useSaveSettingsMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (settings: Settings) =>
      withErrorToast(m.operations.saveSettings, () => commands.saveSettings(settings)),
    onSuccess: (_, settings) => {
      queryClient.setQueryData(queryKeys.settings, settings);
    }
  }));
}
