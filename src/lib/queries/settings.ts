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

/**
 * 現在の設定を取得するクエリ
 *
 * 変更は保存またはイベントでキャッシュに反映するため、自動では再取得しない。
 */
export function useSettingsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.settings,
    queryFn: () => withErrorToast('設定の読み込み', () => commands.getSettings()),
    staleTime: Infinity
  }));
}

/**
 * 設定を保存するミューテーション
 */
export function useSaveSettingsMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (settings: Settings) =>
      withErrorToast('設定の保存', () => commands.saveSettings(settings)),
    onSuccess: (_, settings) => {
      queryClient.setQueryData(queryKeys.settings, settings);
    }
  }));
}
