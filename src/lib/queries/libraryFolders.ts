/**
 * ライブラリフォルダ（インポートしたフォルダ）のクエリ・ミューテーション
 *
 * 設定ウィンドウの「ライブラリ」で使う。設定ウィンドウとメインウィンドウはQueryの
 * キャッシュを共有しないため、再スキャン・削除でライブラリが変わると、Rustが送る
 * `LibraryChanged`イベントでメインウィンドウの一覧を無効化する（`(app)/+layout.svelte`）。
 */

import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * ライブラリフォルダの一覧を取得するクエリ
 *
 * メインウィンドウでのインポート・自動の再スキャン（`LibraryChanged`）と、設定ウィンドウに
 * 戻ったとき（外付けドライブの接続など）に、`LibraryFolderSettings`が無効化して読み直す
 * （staleTimeを設けない）。
 */
export function useLibraryFoldersQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.libraryFolders,
    queryFn: () =>
      withErrorToast(m.operations.fetchLibraryFolders, () => commands.getLibraryFolders())
  }));
}

/**
 * ライブラリフォルダを再スキャンするミューテーション
 */
export function useRescanLibraryFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (folderId: string) =>
      withErrorToast(m.operations.rescanLibraryFolder, () =>
        commands.rescanLibraryFolder(folderId)
      ),
    // 失敗した場合もスキャンの途中までは反映されているため、一覧を読み直す
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/**
 * ライブラリフォルダの記録を削除するミューテーション
 *
 * `removeTracks`がtrueなら、フォルダ内の曲もライブラリから外す（ファイルは消さない）。
 */
export function useRemoveLibraryFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ folderId, removeTracks }: { folderId: string; removeTracks: boolean }) =>
      withErrorToast(m.operations.removeLibraryFolder, () =>
        commands.removeLibraryFolder(folderId, removeTracks)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/**
 * 読み込まないフォルダを足すミューテーション
 *
 * フォルダはRust側のダイアログで選ぶ（WebViewからパスを渡さない）。足した除外（中にある曲の数を
 * 含む）を返し、選ばずに閉じた場合はnullを返す。中の曲はライブラリに残る（外す時は
 * `useRemoveExcludedTracksMutation`）。
 */
export function useAddLibraryFolderExclusionMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (folderId: string) =>
      withErrorToast(m.operations.addLibraryFolderExclusion, () =>
        commands.addLibraryFolderExclusion(folderId)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/**
 * 読み込まないフォルダの中にある曲を、ライブラリから外すミューテーション（ファイルは消さない）
 *
 * 外した曲数を返す。メインウィンドウの一覧は、Rustが送る`LibraryChanged`イベントで更新される。
 */
export function useRemoveExcludedTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (exclusionId: string) =>
      withErrorToast(m.operations.removeExcludedTracks, () =>
        commands.removeExcludedTracks(exclusionId)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/**
 * 読み込まないフォルダを一覧から外し、ライブラリフォルダを再スキャンするミューテーション
 *
 * 再スキャンの結果を返す（フォルダが見つからず、再スキャンしなかった場合はnull）。
 */
export function useRemoveLibraryFolderExclusionMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (exclusionId: string) =>
      withErrorToast(m.operations.removeLibraryFolderExclusion, () =>
        commands.removeLibraryFolderExclusion(exclusionId)
      ),
    // 再スキャンの途中で失敗した場合も、除外を外した分と途中までの分は反映されているため、読み直す
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/**
 * 見つからない曲（ファイルが見つからなくなったトラック）を、すべてライブラリから外すミューテーション
 *
 * 外した曲数を返す。メインウィンドウの一覧は、Rustが送る`LibraryChanged`イベントで更新される。
 */
export function useRemoveMissingTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.removeMissingTracks, () => commands.removeMissingTracks()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}
