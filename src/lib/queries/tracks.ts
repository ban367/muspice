import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '$lib/bindings';
import type { Track, Metadata, AlbumArt, DeleteResult, FilterOptions } from '$lib/types/models';
import { handleError, showSuccess, showWarning } from '$lib/stores/error';
import { queryKeys } from './keys';
import { CACHE_POLICY, withErrorToast } from './shared';

// 呼び出し側の利便性のため、このモジュールからも型を再エクスポートする
export type { DeleteResult, FilterOptions };

// ========== Query Invalidation ヘルパー ==========

/**
 * トラック一覧と関連グループクエリを無効化（トラック削除・インポート時）
 */
export function invalidateTrackListQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: queryKeys.tracks.all });
  queryClient.invalidateQueries({ queryKey: queryKeys.albums.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.artists.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.genres.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.unique.all });
  // プレイリストは除外（トラック削除でプレイリスト自体は変わらない）
}

/**
 * メタデータ変更に関連するクエリを無効化（メタデータ編集時）
 */
export function invalidateTrackMetadataQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: queryKeys.tracks.all });
  queryClient.invalidateQueries({ queryKey: queryKeys.albums.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.artists.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.genres.grouped });
  queryClient.invalidateQueries({ queryKey: queryKeys.unique.all });
}

/**
 * 再生統計関連のクエリを無効化（お気に入り・レーティング・再生回数変更時）
 *
 * queryKeys.tracks.allをプレフィックスに持つ全クエリ（一覧、検索、フィルタ、
 * お気に入り等）を無効化する。
 * exact: trueを使用しないことで、検索/フィルタ結果でもisFavorite/rating表示が更新される。
 */
export function invalidatePlayStatsQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: queryKeys.tracks.all });
}

/**
 * 全トラック関連クエリを無効化（後方互換性のため残す）
 */
function invalidateAllTrackQueries(queryClient: QueryClient) {
  invalidateTrackListQueries(queryClient);
  queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
}

// ========== 読み取りクエリ ==========

/**
 * トラック一覧を取得するクエリ（パフォーマンス最適化版）
 */
export function useTracksQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.all,
    queryFn: () => withErrorToast('トラック一覧の取得', () => commands.getAllTracks()),
    ...CACHE_POLICY.library,
    refetchOnWindowFocus: false, // ウィンドウフォーカス時の自動再取得を無効化
    refetchOnMount: false // マウント時の自動再取得を無効化（キャッシュがあれば使用）
  }));
}

/**
 * 検索クエリ（パフォーマンス最適化版）
 */
export function useSearchQuery(searchTerm: string) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.search(searchTerm),
    queryFn: () => withErrorToast('トラック検索', () => commands.searchTracks(searchTerm)),
    enabled: searchTerm.length > 0,
    ...CACHE_POLICY.search,
    refetchOnWindowFocus: false,
    // 検索中に前回結果を表示し続ける（ちらつき防止）
    placeholderData: (previousData: Track[] | undefined) => previousData
  }));
}

/**
 * フィルタリングクエリ（パフォーマンス最適化版）
 */
export function useFilterQuery(filters: FilterOptions) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.filter(filters),
    queryFn: () => withErrorToast('トラックフィルタリング', () => commands.filterTracks(filters)),
    enabled: !!(filters.artist || filters.album || filters.genre),
    ...CACHE_POLICY.search,
    refetchOnWindowFocus: false,
    // フィルタリング中に前回結果を表示し続ける
    placeholderData: (previousData: Track[] | undefined) => previousData
  }));
}

/**
 * ユニークなアーティスト一覧を取得
 */
export function useUniqueArtistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.unique.artists,
    queryFn: async () => {
      return await commands.getUniqueArtists();
    },
    ...CACHE_POLICY.library
  }));
}

/**
 * ユニークなアルバム一覧を取得
 */
export function useUniqueAlbumsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.unique.albums,
    queryFn: async () => {
      return await commands.getUniqueAlbums();
    },
    ...CACHE_POLICY.library
  }));
}

/**
 * ユニークなジャンル一覧を取得
 */
export function useUniqueGenresQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.unique.genres,
    queryFn: async () => {
      return await commands.getUniqueGenres();
    },
    ...CACHE_POLICY.library
  }));
}

/**
 * アルバムアートを取得するクエリ
 */
export function useAlbumArtQuery(trackId: string | null) {
  return createQuery(() => ({
    queryKey: queryKeys.albumArt(trackId),
    queryFn: async () => {
      if (!trackId) return null;
      try {
        return await commands.getAlbumArt(trackId);
      } catch (error) {
        // アルバムアートがない場合はエラーを無視
        console.debug('アルバムアート取得エラー:', error);
        return null;
      }
    },
    enabled: !!trackId,
    ...CACHE_POLICY.albumArt
  }));
}

/**
 * アルバムアートを直接取得する関数（キャッシュなし）
 */
export async function getAlbumArt(trackId: string): Promise<AlbumArt | null> {
  try {
    return await commands.getAlbumArt(trackId);
  } catch {
    return null;
  }
}

/**
 * お気に入りトラック一覧を取得するクエリ
 */
export function useFavoriteTracksQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.favorites,
    queryFn: () => withErrorToast('お気に入り一覧の取得', () => commands.getFavoriteTracks()),
    ...CACHE_POLICY.playStats
  }));
}

/**
 * よく再生するトラック一覧を取得するクエリ
 */
export function useMostPlayedTracksQuery(limit: number = 50) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.mostPlayed(limit),
    queryFn: () =>
      withErrorToast('よく再生するトラック一覧の取得', () => commands.getMostPlayedTracks(limit)),
    ...CACHE_POLICY.playStats
  }));
}

/**
 * 最近再生したトラック一覧を取得するクエリ
 */
export function useRecentlyPlayedTracksQuery(limit: number = 50) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.recentlyPlayed(limit),
    queryFn: () =>
      withErrorToast('最近再生したトラック一覧の取得', () =>
        commands.getRecentlyPlayedTracks(limit)
      ),
    ...CACHE_POLICY.volatile
  }));
}

// ========== グループ化データ取得クエリ ==========

/**
 * アルバムごとにグループ化されたトラックを取得
 */
export function useAlbumsGroupedQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.albums.grouped,
    queryFn: () => withErrorToast('アルバム一覧の取得', () => commands.getAlbumsGrouped()),
    ...CACHE_POLICY.library
  }));
}

/**
 * アーティストごとにグループ化されたトラックを取得
 */
export function useArtistsGroupedQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.artists.grouped,
    queryFn: () => withErrorToast('アーティスト一覧の取得', () => commands.getArtistsGrouped()),
    ...CACHE_POLICY.library
  }));
}

/**
 * ジャンルごとにグループ化されたトラックを取得
 */
export function useGenresGroupedQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.genres.grouped,
    queryFn: () => withErrorToast('ジャンル一覧の取得', () => commands.getGenresGrouped()),
    ...CACHE_POLICY.library
  }));
}

// ========== 再生統計ミューテーション ==========

/**
 * お気に入りをトグルするミューテーション
 */
export function useToggleFavoriteMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async (trackId: string) => {
      return withErrorToast('お気に入りの切り替え', () => commands.toggleFavorite(trackId));
    },
    onSuccess: () => {
      invalidatePlayStatsQueries(queryClient);
    }
  }));
}

/**
 * レーティングを設定するミューテーション
 */
export function useSetRatingMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackId, rating }: { trackId: string; rating: number }) => {
      await withErrorToast('レーティングの設定', () => commands.setRating(trackId, rating));
    },
    onSuccess: () => {
      invalidatePlayStatsQueries(queryClient);
    }
  }));
}

/**
 * 再生回数をインクリメントする関数（fire-and-forget、UIブロック不要）
 */
export async function incrementPlayCount(trackId: string): Promise<void> {
  try {
    await commands.incrementPlayCount(trackId);
  } catch (error) {
    // 再生回数の更新エラーは静かに処理
    console.debug('再生回数更新エラー:', error);
  }
}

// ========== メタデータ更新ミューテーション ==========

/**
 * 単一トラックのメタデータを更新するミューテーション（DBのみ）
 */
export function useUpdateTrackMetadataMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackId, metadata }: { trackId: string; metadata: Metadata }) => {
      await commands.updateTrackMetadata(trackId, metadata);
    },
    onSuccess: () => {
      invalidateTrackMetadataQueries(queryClient);
    }
  }));
}

/**
 * 単一トラックのメタデータを更新するミューテーション（DB + ファイル）
 */
export function useUpdateTrackMetadataWithFileMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackId, metadata }: { trackId: string; metadata: Metadata }) => {
      await commands.updateTrackMetadataWithFile(trackId, metadata);
    },
    onSuccess: () => {
      invalidateTrackMetadataQueries(queryClient);
    }
  }));
}

/**
 * 複数トラックのメタデータを一括更新するミューテーション
 */
export function useUpdateMultipleTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackIds, metadata }: { trackIds: string[]; metadata: Metadata }) => {
      await commands.updateMultipleTracksMetadata(trackIds, metadata);
    },
    onSuccess: () => {
      invalidateTrackMetadataQueries(queryClient);
    }
  }));
}

// ========== トラック削除ミューテーション ==========

/**
 * トラックをライブラリから削除するミューテーション（データベースのみ）
 * ファイルは削除せず、データベースからのみ削除
 */
export function useDeleteTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async (trackIds: string[]) => {
      return withErrorToast('トラックの削除', () => commands.deleteTracksCommand(trackIds));
    },
    onSuccess: (deletedCount) => {
      invalidateAllTrackQueries(queryClient);
      showSuccess(
        deletedCount === 1
          ? 'トラックをライブラリから削除しました'
          : `${deletedCount}曲をライブラリから削除しました`
      );
    }
  }));
}

/**
 * トラックをライブラリとファイルシステムから削除するミューテーション
 * データベースとファイル両方を削除
 */
export function useDeleteTracksWithFilesMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async (trackIds: string[]) => {
      return withErrorToast('トラックとファイルの削除', () =>
        commands.deleteTracksWithFilesCommand(trackIds)
      );
    },
    onSuccess: (result) => {
      invalidateAllTrackQueries(queryClient);

      if (result.failedCount === 0) {
        showSuccess(
          result.successCount === 1
            ? 'トラックとファイルを削除しました'
            : `${result.successCount}曲とファイルを削除しました`
        );
      } else if (result.successCount === 0) {
        handleError(new Error('すべてのトラックの削除に失敗しました'), 'トラックの削除');
      } else {
        showWarning(
          `${result.successCount}曲を削除しました（${result.failedCount}曲は削除に失敗）`
        );
      }
    }
  }));
}
