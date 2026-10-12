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
