import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '$lib/bindings';
import type { Playlist } from '$lib/types/models';
import { handleError, showSuccess } from '$lib/stores/error';
import { queryKeys } from './keys';

/**
 * プレイリスト一覧を取得するクエリ
 */
export function usePlaylistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.playlists,
    queryFn: async () => {
      try {
        return await commands.getPlaylists();
      } catch (error) {
        handleError(error, 'プレイリスト一覧の取得');
        throw error;
      }
    }
  }));
}

/**
 * プレイリストを作成するミューテーション
 */
export function useCreatePlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async (name: string) => {
      try {
        return await commands.createPlaylist(name);
      } catch (error) {
        handleError(error, 'プレイリストの作成');
        throw error;
      }
    },
    onSuccess: () => {
      // プレイリスト一覧を再取得
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
      showSuccess('プレイリストを作成しました');
    }
  }));
}

/**
 * プレイリストにトラックを追加するミューテーション（Optimistic Update付き）
 */
export function useAddTrackToPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ playlistId, trackId }: { playlistId: string; trackId: string }) => {
      try {
        await commands.addTrackToPlaylist(playlistId, trackId);
      } catch (error) {
        handleError(error, 'トラックの追加');
        throw error;
      }
    },
    // Optimistic Update: UIを先行更新
    onMutate: async ({ playlistId, trackId }) => {
      // 進行中のクエリをキャンセル
      await queryClient.cancelQueries({ queryKey: queryKeys.playlists });

      // 前回のデータを保存（ロールバック用）
      const previousPlaylists = queryClient.getQueryData<Playlist[]>(queryKeys.playlists);

      // キャッシュを楽観的に更新
      if (previousPlaylists) {
        queryClient.setQueryData<Playlist[]>(
          queryKeys.playlists,
          previousPlaylists.map((pl) =>
            pl.id === playlistId
              ? {
                  ...pl,
                  tracks: [
                    ...pl.tracks,
                    {
                      trackId,
                      position: pl.tracks.length,
                      addedAt: new Date().toISOString()
                    }
                  ]
                }
              : pl
          )
        );
      }

      return { previousPlaylists };
    },
    // エラー時にロールバック
    onError: (_error, _variables, context) => {
      if (context?.previousPlaylists) {
        queryClient.setQueryData(queryKeys.playlists, context.previousPlaylists);
      }
    },
    // 成功・エラーに関わらず最終的にサーバーデータで同期
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
    },
    onSuccess: () => {
      showSuccess('トラックをプレイリストに追加しました');
    }
  }));
}

/**
 * プレイリストからトラックを削除するミューテーション（Optimistic Update付き）
 */
export function useRemoveTrackFromPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ playlistId, trackId }: { playlistId: string; trackId: string }) => {
      try {
        await commands.removeTrackFromPlaylist(playlistId, trackId);
      } catch (error) {
        handleError(error, 'トラックの削除');
        throw error;
      }
    },
    // Optimistic Update
    onMutate: async ({ playlistId, trackId }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.playlists });

      const previousPlaylists = queryClient.getQueryData<Playlist[]>(queryKeys.playlists);

      if (previousPlaylists) {
        queryClient.setQueryData<Playlist[]>(
          queryKeys.playlists,
          previousPlaylists.map((pl) =>
            pl.id === playlistId
              ? {
                  ...pl,
                  tracks: pl.tracks.filter((t) => t.trackId !== trackId)
                }
              : pl
          )
        );
      }

      return { previousPlaylists };
    },
    onError: (_error, _variables, context) => {
      if (context?.previousPlaylists) {
        queryClient.setQueryData(queryKeys.playlists, context.previousPlaylists);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
    },
    onSuccess: () => {
      showSuccess('トラックをプレイリストから削除しました');
    }
  }));
}

/**
 * プレイリスト内のトラックを並び替えるミューテーション
 */
export function useReorderPlaylistTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ playlistId, trackIds }: { playlistId: string; trackIds: string[] }) => {
      try {
        await commands.reorderPlaylistTracks(playlistId, trackIds);
      } catch (error) {
        handleError(error, 'トラックの並び替え');
        throw error;
      }
    },
    onSuccess: () => {
      // プレイリスト一覧を再取得
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
      showSuccess('トラックを並び替えました');
    }
  }));
}

/**
 * プレイリストの名前を変更するミューテーション（Optimistic Update付き）
 */
export function useRenamePlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ playlistId, name }: { playlistId: string; name: string }) => {
      try {
        await commands.renamePlaylist(playlistId, name);
      } catch (error) {
        handleError(error, 'プレイリスト名の変更');
        throw error;
      }
    },
    // Optimistic Update
    onMutate: async ({ playlistId, name }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.playlists });

      const previousPlaylists = queryClient.getQueryData<Playlist[]>(queryKeys.playlists);

      if (previousPlaylists) {
        queryClient.setQueryData<Playlist[]>(
          queryKeys.playlists,
          previousPlaylists.map((pl) => (pl.id === playlistId ? { ...pl, name } : pl))
        );
      }

      return { previousPlaylists };
    },
    onError: (_error, _variables, context) => {
      if (context?.previousPlaylists) {
        queryClient.setQueryData(queryKeys.playlists, context.previousPlaylists);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
    },
    onSuccess: () => {
      showSuccess('プレイリスト名を変更しました');
    }
  }));
}

/**
 * プレイリストを削除するミューテーション（Optimistic Update付き）
 */
export function useDeletePlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async (playlistId: string) => {
      try {
        await commands.deletePlaylist(playlistId);
      } catch (error) {
        handleError(error, 'プレイリストの削除');
        throw error;
      }
    },
    // Optimistic Update
    onMutate: async (playlistId) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.playlists });

      const previousPlaylists = queryClient.getQueryData<Playlist[]>(queryKeys.playlists);

      if (previousPlaylists) {
        queryClient.setQueryData<Playlist[]>(
          queryKeys.playlists,
          previousPlaylists.filter((pl) => pl.id !== playlistId)
        );
      }

      return { previousPlaylists };
    },
    onError: (_error, _variables, context) => {
      if (context?.previousPlaylists) {
        queryClient.setQueryData(queryKeys.playlists, context.previousPlaylists);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
    },
    onSuccess: () => {
      showSuccess('プレイリストを削除しました');
    }
  }));
}
