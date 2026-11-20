/**
 * アルバムアートの情報の取得と、編集（画像の選択・埋め込み・取り除き）
 *
 * 画像はRust側が音楽ファイルのタグへ書き込む。埋め込む画像もRust側のダイアログで選び、Rust側が預かるため、
 * WebViewはパスも画像データも渡さない（編集画面は、保存する時に預かった画像の印を渡す。ADR-094）。
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
import { queryKeys } from './keys.js';
import { withErrorToast } from './shared.js';
import { invalidateTrackListQueries } from './tracks.js';

/**
 * 曲のアルバムアートの情報（埋め込みの画像か、フォルダの画像か・種類・大きさ）を取得するクエリ
 *
 * 編集画面を開くたびに、ファイルの今の内容を読む（キャッシュを使い回さない）。
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
 * アルバムアートにする画像を選ぶミューテーション（編集画面の「画像を選ぶ」。ファイルへは書き込まない）
 *
 * 画像はRust側のダイアログで選び、Rust側が預かる。見本の画像（data URL）と、預かった画像を指す印が
 * 返る。選ばなかった場合はnull。失敗（JPEG・PNGでない・大きすぎる）は、呼び出し側が画面の中に出す。
 */
export function usePickAlbumArtMutation() {
  return createMutation(() => ({
    mutationFn: () => commands.pickAlbumArt()
  }));
}

/**
 * 選んだ画像（`usePickAlbumArtMutation`の印）を、曲のファイルにアルバムアートとして埋め込むミューテーション
 *
 * 書き込めなかった曲は、結果の`failedCount`・`errors`で返る（残りの曲は書き込まれる）。
 * 結果の表示は呼び出し側が行う。
 */
export function useSetAlbumArtMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ trackIds, token }: { trackIds: string[]; token: string }) =>
      commands.setAlbumArt(trackIds, token),
    onSuccess: (result, { trackIds }) => {
      if (result.updatedCount > 0) afterAlbumArtChange(queryClient, trackIds);
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
