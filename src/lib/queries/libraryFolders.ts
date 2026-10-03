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

/**
 * ライブラリフォルダの一覧を取得するクエリ
 *
 * メインウィンドウでインポートするとフォルダが増えるため、ウィンドウに戻った時に
 * 再取得させる（staleTimeを設けない）。
 */
export function useLibraryFoldersQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.libraryFolders,
    queryFn: () => withErrorToast('ライブラリフォルダの取得', () => commands.getLibraryFolders())
  }));
}

/**
 * ライブラリフォルダを再スキャンするミューテーション
 */
export function useRescanLibraryFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (folderId: string) =>
      withErrorToast('再スキャン', () => commands.rescanLibraryFolder(folderId)),
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
      withErrorToast('ライブラリフォルダの削除', () =>
        commands.removeLibraryFolder(folderId, removeTracks)
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}
