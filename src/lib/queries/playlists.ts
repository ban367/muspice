import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '$lib/bindings';
import type { Playlist } from '$lib/types/models';
import { showSuccess } from '$lib/stores/error';
import { queryKeys } from './keys';
import { withErrorToast } from './shared';

/** ロールバック用に直前のプレイリスト一覧を保持するコンテキスト */
interface PlaylistSnapshot {
  previousPlaylists: Playlist[] | undefined;
}

/**
 * プレイリスト一覧を楽観的に更新するミューテーションオプションを生成する
 *
 * 「進行中クエリのキャンセル → スナップショット退避 → 楽観的更新」と、
 * 失敗時のロールバック、完了後のサーバー同期をまとめて提供する。
 *
 * @param updater 現在の一覧と変数から、更新後の一覧を返す関数
 */
function optimisticPlaylistUpdate<TVariables>(
  queryClient: QueryClient,
  updater: (playlists: Playlist[], variables: TVariables) => Playlist[]
) {
  return {
    onMutate: async (variables: TVariables): Promise<PlaylistSnapshot> => {
      // 進行中のクエリをキャンセル（後から古い結果で上書きされるのを防ぐ）
      await queryClient.cancelQueries({ queryKey: queryKeys.playlists });

      // 前回のデータを保存（ロールバック用）
      const previousPlaylists = queryClient.getQueryData<Playlist[]>(queryKeys.playlists);

      // キャッシュを楽観的に更新
      if (previousPlaylists) {
        queryClient.setQueryData<Playlist[]>(
          queryKeys.playlists,
          updater(previousPlaylists, variables)
        );
      }

      return { previousPlaylists };
    },
    // エラー時にロールバック
    onError: (_error: unknown, _variables: TVariables, context: PlaylistSnapshot | undefined) => {
      if (context?.previousPlaylists) {
        queryClient.setQueryData(queryKeys.playlists, context.previousPlaylists);
      }
    },
    // 成功・エラーに関わらず最終的にサーバーデータで同期
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
    }
  };
}

/**
 * プレイリスト一覧を取得するクエリ
 */
export function usePlaylistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.playlists,
    queryFn: () => withErrorToast('プレイリスト一覧の取得', () => commands.getPlaylists())
  }));
}

/**
 * プレイリストを作成するミューテーション
 */
export function useCreatePlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (name: string) =>
      withErrorToast('プレイリストの作成', () => commands.createPlaylist(name)),
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
    mutationFn: ({ playlistId, trackId }: { playlistId: string; trackId: string }) =>
      withErrorToast('トラックの追加', () => commands.addTrackToPlaylist(playlistId, trackId)),
    ...optimisticPlaylistUpdate<{ playlistId: string; trackId: string }>(
      queryClient,
      (playlists, { playlistId, trackId }) =>
        playlists.map((pl) =>
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
    ),
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
    mutationFn: ({ playlistId, trackId }: { playlistId: string; trackId: string }) =>
      withErrorToast('トラックの削除', () => commands.removeTrackFromPlaylist(playlistId, trackId)),
    ...optimisticPlaylistUpdate<{ playlistId: string; trackId: string }>(
      queryClient,
      (playlists, { playlistId, trackId }) =>
        playlists.map((pl) =>
          pl.id === playlistId
            ? { ...pl, tracks: pl.tracks.filter((t) => t.trackId !== trackId) }
            : pl
        )
    ),
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
    mutationFn: ({ playlistId, trackIds }: { playlistId: string; trackIds: string[] }) =>
      withErrorToast('トラックの並び替え', () =>
        commands.reorderPlaylistTracks(playlistId, trackIds)
      ),
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
    mutationFn: ({ playlistId, name }: { playlistId: string; name: string }) =>
      withErrorToast('プレイリスト名の変更', () => commands.renamePlaylist(playlistId, name)),
    ...optimisticPlaylistUpdate<{ playlistId: string; name: string }>(
      queryClient,
      (playlists, { playlistId, name }) =>
        playlists.map((pl) => (pl.id === playlistId ? { ...pl, name } : pl))
    ),
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
    mutationFn: (playlistId: string) =>
      withErrorToast('プレイリストの削除', () => commands.deletePlaylist(playlistId)),
    ...optimisticPlaylistUpdate<string>(queryClient, (playlists, playlistId) =>
      playlists.filter((pl) => pl.id !== playlistId)
    ),
    onSuccess: () => {
      showSuccess('プレイリストを削除しました');
    }
  }));
}
