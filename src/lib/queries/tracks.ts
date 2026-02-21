import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { invoke } from '@tauri-apps/api/core';
import type {
  Track,
  Metadata,
  AlbumArt,
  AlbumGroup,
  ArtistGroup,
  GenreGroup
} from '$lib/types/models';
import { handleError, showSuccess, showWarning } from '$lib/stores/error';

/**
 * トラック削除の結果
 */
export interface DeleteResult {
  successCount: number;
  failedCount: number;
  failedTracks: Array<{
    trackId: string;
    filePath: string;
    reason: string;
  }>;
}

export interface FilterOptions {
  artist?: string;
  album?: string;
  genre?: string;
}

// ========== Query Invalidation ヘルパー ==========

/**
 * トラック一覧と関連グループクエリを無効化（トラック削除・インポート時）
 */
export function invalidateTrackListQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: ['tracks'] });
  queryClient.invalidateQueries({ queryKey: ['albums', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['artists', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['genres', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['unique'] });
  // プレイリストは除外（トラック削除でプレイリスト自体は変わらない）
}

/**
 * メタデータ変更に関連するクエリを無効化（メタデータ編集時）
 */
export function invalidateTrackMetadataQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: ['tracks'] });
  queryClient.invalidateQueries({ queryKey: ['albums', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['artists', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['genres', 'grouped'] });
  queryClient.invalidateQueries({ queryKey: ['unique'] });
}

/**
 * 再生統計関連のクエリを無効化（お気に入り・レーティング・再生回数変更時）
 */
export function invalidatePlayStatsQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: ['tracks', 'favorites'] });
  queryClient.invalidateQueries({ queryKey: ['tracks', 'mostPlayed'] });
  queryClient.invalidateQueries({ queryKey: ['tracks', 'recentlyPlayed'] });
  // メイントラック一覧も更新（isFavorite, rating, playCount表示のため）
  queryClient.invalidateQueries({ queryKey: ['tracks'], exact: true });
}

/**
 * 全トラック関連クエリを無効化（後方互換性のため残す）
 */
function invalidateAllTrackQueries(queryClient: QueryClient) {
  invalidateTrackListQueries(queryClient);
  queryClient.invalidateQueries({ queryKey: ['playlists'] });
}

// ========== 読み取りクエリ ==========

/**
 * トラック一覧を取得するクエリ（パフォーマンス最適化版）
 */
export function useTracksQuery() {
  return createQuery(() => ({
    queryKey: ['tracks'],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('get_all_tracks');
      } catch (error) {
        handleError(error, 'トラック一覧の取得');
        throw error;
      }
    },
    staleTime: 10 * 60 * 1000, // 10分間キャッシュ（新鮮とみなす時間）
    gcTime: 30 * 60 * 1000, // 30分間メモリに保持
    refetchOnWindowFocus: false, // ウィンドウフォーカス時の自動再取得を無効化
    refetchOnMount: false // マウント時の自動再取得を無効化（キャッシュがあれば使用）
  }));
}

/**
 * 検索クエリ（パフォーマンス最適化版）
 */
export function useSearchQuery(searchTerm: string) {
  return createQuery(() => ({
    queryKey: ['tracks', 'search', searchTerm],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('search_tracks', { query: searchTerm });
      } catch (error) {
        handleError(error, 'トラック検索');
        throw error;
      }
    },
    enabled: searchTerm.length > 0,
    staleTime: 5 * 60 * 1000, // 5分間キャッシュ
    gcTime: 15 * 60 * 1000, // 15分間メモリに保持
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
    queryKey: ['tracks', 'filter', filters],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('filter_tracks', { filters });
      } catch (error) {
        handleError(error, 'トラックフィルタリング');
        throw error;
      }
    },
    enabled: !!(filters.artist || filters.album || filters.genre),
    staleTime: 5 * 60 * 1000, // 5分間キャッシュ
    gcTime: 15 * 60 * 1000, // 15分間メモリに保持
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
    queryKey: ['unique', 'artists'],
    queryFn: async () => {
      return await invoke<string[]>('get_unique_artists');
    },
    staleTime: 10 * 60 * 1000 // 10分間キャッシュ
  }));
}

/**
 * ユニークなアルバム一覧を取得
 */
export function useUniqueAlbumsQuery() {
  return createQuery(() => ({
    queryKey: ['unique', 'albums'],
    queryFn: async () => {
      return await invoke<string[]>('get_unique_albums');
    },
    staleTime: 10 * 60 * 1000 // 10分間キャッシュ
  }));
}

/**
 * ユニークなジャンル一覧を取得
 */
export function useUniqueGenresQuery() {
  return createQuery(() => ({
    queryKey: ['unique', 'genres'],
    queryFn: async () => {
      return await invoke<string[]>('get_unique_genres');
    },
    staleTime: 10 * 60 * 1000 // 10分間キャッシュ
  }));
}

/**
 * アルバムアートを取得するクエリ
 */
export function useAlbumArtQuery(trackId: string | null) {
  return createQuery(() => ({
    queryKey: ['albumArt', trackId],
    queryFn: async () => {
      if (!trackId) return null;
      try {
        return await invoke<AlbumArt | null>('get_album_art', { trackId });
      } catch (error) {
        // アルバムアートがない場合はエラーを無視
        console.debug('アルバムアート取得エラー:', error);
        return null;
      }
    },
    enabled: !!trackId,
    staleTime: 30 * 60 * 1000, // 30分間キャッシュ
    gcTime: 60 * 60 * 1000 // 1時間メモリに保持
  }));
}

/**
 * アルバムアートを直接取得する関数（キャッシュなし）
 */
export async function getAlbumArt(trackId: string): Promise<AlbumArt | null> {
  try {
    return await invoke<AlbumArt | null>('get_album_art', { trackId });
  } catch {
    return null;
  }
}

/**
 * お気に入りトラック一覧を取得するクエリ
 */
export function useFavoriteTracksQuery() {
  return createQuery(() => ({
    queryKey: ['tracks', 'favorites'],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('get_favorite_tracks');
      } catch (error) {
        handleError(error, 'お気に入り一覧の取得');
        throw error;
      }
    },
    staleTime: 5 * 60 * 1000,
    gcTime: 15 * 60 * 1000
  }));
}

/**
 * よく再生するトラック一覧を取得するクエリ
 */
export function useMostPlayedTracksQuery(limit: number = 50) {
  return createQuery(() => ({
    queryKey: ['tracks', 'mostPlayed', limit],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('get_most_played_tracks', { limit });
      } catch (error) {
        handleError(error, 'よく再生するトラック一覧の取得');
        throw error;
      }
    },
    staleTime: 5 * 60 * 1000,
    gcTime: 15 * 60 * 1000
  }));
}

/**
 * 最近再生したトラック一覧を取得するクエリ
 */
export function useRecentlyPlayedTracksQuery(limit: number = 50) {
  return createQuery(() => ({
    queryKey: ['tracks', 'recentlyPlayed', limit],
    queryFn: async () => {
      try {
        return await invoke<Track[]>('get_recently_played_tracks', { limit });
      } catch (error) {
        handleError(error, '最近再生したトラック一覧の取得');
        throw error;
      }
    },
    staleTime: 1 * 60 * 1000, // 1分間キャッシュ（頻繁に更新される）
    gcTime: 5 * 60 * 1000
  }));
}

// ========== グループ化データ取得クエリ ==========

/**
 * アルバムごとにグループ化されたトラックを取得
 */
export function useAlbumsGroupedQuery() {
  return createQuery(() => ({
    queryKey: ['albums', 'grouped'],
    queryFn: async () => {
      try {
        return await invoke<AlbumGroup[]>('get_albums_grouped');
      } catch (error) {
        handleError(error, 'アルバム一覧の取得');
        throw error;
      }
    },
    staleTime: 10 * 60 * 1000, // 10分間キャッシュ
    gcTime: 30 * 60 * 1000
  }));
}

/**
 * アーティストごとにグループ化されたトラックを取得
 */
export function useArtistsGroupedQuery() {
  return createQuery(() => ({
    queryKey: ['artists', 'grouped'],
    queryFn: async () => {
      try {
        return await invoke<ArtistGroup[]>('get_artists_grouped');
      } catch (error) {
        handleError(error, 'アーティスト一覧の取得');
        throw error;
      }
    },
    staleTime: 10 * 60 * 1000, // 10分間キャッシュ
    gcTime: 30 * 60 * 1000
  }));
}

/**
 * ジャンルごとにグループ化されたトラックを取得
 */
export function useGenresGroupedQuery() {
  return createQuery(() => ({
    queryKey: ['genres', 'grouped'],
    queryFn: async () => {
      try {
        return await invoke<GenreGroup[]>('get_genres_grouped');
      } catch (error) {
        handleError(error, 'ジャンル一覧の取得');
        throw error;
      }
    },
    staleTime: 10 * 60 * 1000, // 10分間キャッシュ
    gcTime: 30 * 60 * 1000
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
      try {
        return await invoke<boolean>('toggle_favorite', { trackId });
      } catch (error) {
        handleError(error, 'お気に入りの切り替え');
        throw error;
      }
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
      try {
        await invoke('set_rating', { trackId, rating });
      } catch (error) {
        handleError(error, 'レーティングの設定');
        throw error;
      }
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
    await invoke('increment_play_count', { trackId });
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
      await invoke('update_track_metadata', { trackId, metadata });
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
      await invoke('update_track_metadata_with_file', { trackId, metadata });
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
      await invoke('update_multiple_tracks_metadata', { trackIds, metadata });
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
      try {
        return await invoke<number>('delete_tracks_command', { trackIds });
      } catch (error) {
        handleError(error, 'トラックの削除');
        throw error;
      }
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
      try {
        return await invoke<DeleteResult>('delete_tracks_with_files_command', { trackIds });
      } catch (error) {
        handleError(error, 'トラックとファイルの削除');
        throw error;
      }
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
