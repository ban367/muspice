/**
 * 音楽フォルダ（新しい曲を置く宛先。ライブラリフォルダの1つ）のクエリ・ミューテーション
 *
 * 音楽フォルダを変えると、Rustが`LibraryChanged`イベントを送る。メインウィンドウ（ライブラリが
 * 空の画面）と設定ウィンドウは、それぞれのイベントの受け取りで、このクエリを無効化して読み直す。
 */

import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { MusicFolderSource } from '#lib/types/models.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * 音楽フォルダの設定と、OSの音楽フォルダ（候補）を取得するクエリ
 *
 * ライブラリが空の画面は、曲がある間は表示しない（クエリを使う部品がない）。その間に無効化された
 * 場合も、次に表示した時に読み直す（メインウィンドウの既定は、表示した時に読み直さない）。
 */
export function useMusicFolderQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.musicFolder,
    queryFn: () => withErrorToast(m.operations.fetchMusicFolder, () => commands.getMusicFolder()),
    refetchOnMount: true
  }));
}

/**
 * 音楽フォルダを選ぶミューテーション（設定ウィンドウ）
 *
 * フォルダは、OSの音楽フォルダか、Rust側のダイアログで選ぶ（WebViewからパスを渡さない）。
 * 結果を返し、ダイアログで選ばずに閉じた場合はnullを返す。
 */
export function useSetMusicFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (source: MusicFolderSource) =>
      withErrorToast(m.operations.setMusicFolder, () => commands.setMusicFolder(source)),
    // 読み込みの途中で失敗した場合も、途中までの曲は登録されているため、読み直す
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.musicFolder });
      void queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders });
    }
  }));
}

/**
 * 音楽フォルダの設定を外すミューテーション（ライブラリフォルダの記録は残る）
 */
export function useClearMusicFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.clearMusicFolder, () => commands.clearMusicFolder()),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.musicFolder });
      void queryClient.invalidateQueries({ queryKey: queryKeys.libraryFolders });
    }
  }));
}

/**
 * 音楽フォルダの書式の例を取得する（最近追加したアルバムの曲を、書式に当てはめた場所）
 *
 * 書式を入力するたびに呼ぶため、キャッシュしない。書式が読めない場合は、エラーをそのまま投げる
 * （画面に出す。トーストは出さない）。
 */
export function fetchMusicFolderTemplatePreview(template: string): Promise<string[]> {
  return commands.previewMusicFolderTemplate(template);
}
