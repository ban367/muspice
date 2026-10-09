/**
 * アルバムアートの情報の取得と、編集（埋め込み・取り除き）
 *
 * 画像はRust側が音楽ファイルのタグへ書き込む。埋め込む画像もRust側のダイアログで選ぶため、
 * WebViewはパスも画像データも渡さない。
 */

import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { refreshAlbumArt } from '#lib/stores/albumArt.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';
import { invalidateTrackListQueries } from './tracks';

/**
 * 曲のアルバムアートの情報（埋め込みの画像か、フォルダの画像か・種類・大きさ）を取得するクエリ
 *
 * アルバムアートの画面を開くたびに、ファイルの今の内容を読む（キャッシュを使い回さない）。
 * アートがなければnull。
 */
export function useAlbumArtInfoQuery(trackId: string) {
  return createQuery(() => ({
    queryKey: queryKeys.albumArtInfo.track(trackId),
    queryFn: () =>
      withErrorToast(m.operations.fetchAlbumArtInfo, () => commands.getAlbumArtInfo(trackId)),
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false
  }));
}

/** アルバムアートを書き換えた後、表示とキャッシュを更新する */
function afterAlbumArtChange(queryClient: QueryClient, trackIds: string[]) {
  // 表示中の画像を読み直させる
  refreshAlbumArt(trackIds);
  queryClient.invalidateQueries({ queryKey: queryKeys.albumArtInfo.all });
  // ファイルのサイズが変わる
  invalidateTrackListQueries(queryClient);
}

/**
 * 画像ファイルを選び、曲のファイルにアルバムアートとして埋め込むミューテーション
 *
 * 画像はRust側のダイアログで選ぶ。選ばなかった場合はnullが返る。書き込めなかった曲は、
 * 結果の`failedCount`・`errors`で返る（残りの曲は書き込まれる）。結果の表示は呼び出し側が行う。
 */
export function useSetAlbumArtMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (trackIds: string[]) => commands.setAlbumArt(trackIds),
    onSuccess: (result, trackIds) => {
      if (result && result.updatedCount > 0) afterAlbumArtChange(queryClient, trackIds);
    }
  }));
}

/**
 * 曲のファイルから、埋め込みの画像をすべて取り除くミューテーション
 *
 * 埋め込みの画像がない曲は書き換えず、結果の件数にも数えない。
 */
export function useRemoveAlbumArtMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (trackIds: string[]) => commands.removeAlbumArt(trackIds),
    onSuccess: (result, trackIds) => {
      if (result.updatedCount > 0) afterAlbumArtChange(queryClient, trackIds);
    }
  }));
}
