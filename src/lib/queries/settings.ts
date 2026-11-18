/**
 * アプリケーション設定のクエリ・ミューテーション
 *
 * 設定はRust側（アプリデータ配下のsettings.json）に保存される。
 * 設定ウィンドウとメインウィンドウはQueryのキャッシュを共有しないため、
 * 保存時にRustが送る`SettingsChanged`イベントで他のウィンドウのキャッシュを更新する
 * （`SettingsSync`コンポーネント）。
 */

import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { Settings } from '#lib/types/models.js';
import { queryKeys } from './keys.js';
import { withErrorToast } from './shared.js';
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
 * グローバルホットキーの割り当てと、OSに登録できているかを取得するクエリ（設定画面に出す）
 *
 * 登録の状態は、設定を保存するたびに変わるため、キャッシュを使い回さない。
 * @param enabled 一覧を表示しているかを返す（リアクティブな値を読む）
 */
export function useGlobalShortcutsQuery(enabled: () => boolean) {
  return createQuery(() => ({
    queryKey: queryKeys.globalShortcuts,
    queryFn: () =>
      withErrorToast(m.operations.loadGlobalShortcuts, () => commands.getGlobalShortcuts()),
    enabled: enabled(),
    staleTime: 0
  }));
}

/**
 * ほかのウィンドウで保存された設定（`SettingsChanged`の値）を、キャッシュへ入れる
 *
 * 取得中の設定は取り消してから入れる。通知より前の値を読んだ取得（ウィンドウを開いた直後の、最初の取得
 * など）が後から終わって、古い値で上書きしないようにする。
 */
export async function receiveSavedSettings(
  queryClient: QueryClient,
  settings: Settings
): Promise<void> {
  await queryClient.cancelQueries({ queryKey: queryKeys.settings });
  queryClient.setQueryData(queryKeys.settings, settings);
}

/**
 * メインウィンドウで変える設定（設定ウィンドウには項目がない）
 *
 * - Auto DJ（プレーヤーの再生の補助のメニュー）
 * - リプレイゲイン・ギャップレス再生・クロスフェード・クロスフィード（右サイドバーのサウンドのパネル）
 *
 * 設定ウィンドウは、開いている間にこれらが変えられたら（`SettingsChanged`でキャッシュが更新される）、
 * 編集中の設定へ取り込む（「適用」で元に戻さない）。
 */
export const SETTINGS_EDITED_IN_MAIN_WINDOW = [
  'autoDj',
  'autoDjPlaylistId',
  'volumeNormalization',
  'normalizationPreampDb',
  'normalizationUntaggedPreampDb',
  'normalizationPreventClipping',
  'gaplessPlayback',
  'crossfadeEnabled',
  'crossfadeSeconds',
  'crossfadeSkipAlbumTracks',
  'crossfeedEnabled',
  'crossfeedLevel'
] as const satisfies readonly (keyof Settings)[];

/**
 * 設定の一部を変えて、すぐに保存する（右サイドバーのサウンドのパネル・プレーヤーの再生の補助のメニュー。
 * 設定ウィンドウは、編集してから「適用」で保存する）
 *
 * 続けて変えた時に前の変更を消さないよう、キャッシュを先に書き換えてから保存し、保存は順に1つずつ行う
 * （後に送った保存が先に終わって、古い内容で上書きされないようにする）。保存に失敗したら（トーストで
 * 知らせる）、保存済みの設定を取り直して表示を戻す。
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient();
  const settingsQuery = useSettingsQuery();
  const mutation = createMutation(() => ({
    mutationFn: (settings: Settings) =>
      withErrorToast(m.operations.saveSettings, () => commands.saveSettings(settings)),
    scope: { id: 'update-settings' },
    onError: () => queryClient.invalidateQueries({ queryKey: queryKeys.settings })
  }));

  return {
    /** 保存済みの設定（読み込み前はnull） */
    get current(): Settings | null {
      return settingsQuery.data ?? null;
    },
    /** 設定の一部を変えて保存する（読み込み前は何もしない） */
    update(patch: Partial<Settings>) {
      const current = queryClient.getQueryData<Settings>(queryKeys.settings);
      if (!current) return;
      const next = { ...current, ...patch };
      queryClient.setQueryData(queryKeys.settings, next);
      mutation.mutate(next);
    }
  };
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
