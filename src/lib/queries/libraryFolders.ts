/**
 * ライブラリフォルダ（インポートしたフォルダ）のクエリ・ミューテーション
 *
 * 設定ウィンドウの「ライブラリ」と、メインウィンドウのフォルダ別の画面で使う。設定ウィンドウとメイン
 * ウィンドウはQueryのキャッシュを共有しないため、ライブラリが変わると、Rustが送る`LibraryChanged`
 * イベントで、それぞれのウィンドウが無効化する（メインウィンドウは`invalidateLibraryQueries`、
 * 設定ウィンドウは`LibraryFolderSettings`）。
 */

import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * ライブラリフォルダの一覧を取得するクエリ
 *
 * ライブラリが変わった時（`LibraryChanged`）に、どちらのウィンドウも無効化して読み直す。設定ウィンドウは、
 * ウィンドウに戻ったとき（外付けドライブの接続など）にも読み直す（`LibraryFolderSettings`。staleTimeを
 * 設けない）。
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
 * ライブラリフォルダの付け替え先を選ぶミューテーション
 *
 * 場所はRust側のダイアログで選び、付け替えられる場所ならRust側が預かる（WebViewからパスを渡さない）。
 * 付け替えられるか（付け替えられない場合は、その理由）と、新しい場所でファイルが見つかる曲の数を返す
 * （選ばずに閉じた場合はnull）。付け替える時は`useRelocateLibraryFolderMutation`、やめる時は
 * `cancelLibraryFolderRelocation`。
 */
export function usePlanLibraryFolderRelocationMutation() {
  return createMutation(() => ({
    mutationFn: (folderId: string) =>
      withErrorToast(m.operations.planLibraryFolderRelocation, () =>
        commands.planLibraryFolderRelocation(folderId)
      )
  }));
}

/**
 * 預かっている付け替え先へ、ライブラリフォルダを付け替えるミューテーション
 *
 * 中の曲のパスを書き換えてから、新しい場所を再スキャンする（`LibraryScanProgress`が届く）。
 * メインウィンドウの一覧は、Rustが送る`LibraryChanged`イベントで更新される。
 */
export function useRelocateLibraryFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (folderId: string) =>
      withErrorToast(m.operations.relocateLibraryFolder, () =>
        commands.relocateLibraryFolder(folderId)
      ),
    // 再スキャンで失敗した場合も、付け替えは反映されているため、読み直す
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders })
  }));
}

/** 預かっている付け替え先を手放す（付け替えをやめた時。失敗しても画面は続ける） */
export function cancelLibraryFolderRelocation(): void {
  commands.cancelLibraryFolderRelocation().catch((error: unknown) => {
    console.error('預かった付け替え先を手放せませんでした:', error);
  });
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
