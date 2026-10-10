import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { Playlist } from '#lib/types/models.js';
import { showSuccess } from '#lib/stores/error.svelte.js';
import { queryKeys } from './keys';
import { CACHE_POLICY, withErrorToast } from './shared';
import { m } from '#lib/i18n/i18n.svelte.js';
import { forgetTrackListView, playlistSortViewId } from '#lib/stores/trackListView.svelte.js';

/**
 * プレイリストの一覧と、プレイリストの曲を無効化（プレイリストの内容を変えた時）
 */
function invalidatePlaylistQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
  queryClient.invalidateQueries({ queryKey: queryKeys.tracks.playlists });
}

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
      invalidatePlaylistQueries(queryClient);
    }
  };
}

/**
 * プレイリスト一覧を取得するクエリ
 */
export function usePlaylistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.playlists,
    queryFn: () => withErrorToast(m.operations.fetchPlaylists, () => commands.getPlaylists())
  }));
}

/**
 * プレイリストの曲を取得するクエリ（プレイリストの中の並び順）
 *
 * 曲の情報は全曲の一覧からではなく、プレイリストごとにバックエンドから取得する。
 */
export function usePlaylistTracksQuery(playlistId: string) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.playlist(playlistId),
    queryFn: () =>
      withErrorToast(m.operations.fetchTracks, () => commands.getPlaylistTracks(playlistId)),
    ...CACHE_POLICY.detail
  }));
}

/**
 * プレイリストを作成するミューテーション
 */
export function useCreatePlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (name: string) =>
      withErrorToast(m.operations.createPlaylist, () => commands.createPlaylist(name)),
    onSuccess: () => {
      // プレイリスト一覧を再取得
      queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
      showSuccess(m.notices.playlistCreated);
    }
  }));
}

/**
 * プレイリストにトラックを追加するミューテーション（Optimistic Update付き）
 *
 * 複数のトラックを、渡した順に1回で追加する。すでに入っているトラックは追加されない。
 * 通知は追加した曲数で1回だけ出す。
 */
export function useAddTracksToPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ playlistId, trackIds }: { playlistId: string; trackIds: string[] }) =>
      withErrorToast(m.operations.addTrackToPlaylist, () =>
        commands.addTracksToPlaylist(playlistId, trackIds)
      ),
    ...optimisticPlaylistUpdate<{ playlistId: string; trackIds: string[] }>(
      queryClient,
      (playlists, { playlistId, trackIds }) =>
        playlists.map((pl) => {
          if (pl.id !== playlistId) return pl;

          const existing = new Set(pl.tracks.map((entry) => entry.trackId));
          const addedAt = new Date().toISOString();
          const added = [...new Set(trackIds)]
            .filter((trackId) => !existing.has(trackId))
            .map((trackId, index) => ({ trackId, position: pl.tracks.length + index, addedAt }));
          return { ...pl, tracks: [...pl.tracks, ...added] };
        })
    ),
    onSuccess: (addedCount) => {
      showSuccess(
        addedCount > 0
          ? m.notices.tracksAddedToPlaylist(addedCount)
          : m.notices.tracksAlreadyInPlaylist
      );
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
      withErrorToast(m.operations.removeTrackFromPlaylist, () =>
        commands.removeTrackFromPlaylist(playlistId, trackId)
      ),
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
      showSuccess(m.notices.trackRemovedFromPlaylist);
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
      withErrorToast(m.operations.reorderPlaylistTracks, () =>
        commands.reorderPlaylistTracks(playlistId, trackIds)
      ),
    onSuccess: () => {
      // プレイリスト一覧と、プレイリストの曲を再取得
      invalidatePlaylistQueries(queryClient);
      showSuccess(m.notices.tracksReordered);
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
      withErrorToast(m.operations.renamePlaylist, () => commands.renamePlaylist(playlistId, name)),
    ...optimisticPlaylistUpdate<{ playlistId: string; name: string }>(
      queryClient,
      (playlists, { playlistId, name }) =>
        playlists.map((pl) => (pl.id === playlistId ? { ...pl, name } : pl))
    ),
    onSuccess: () => {
      showSuccess(m.notices.playlistRenamed);
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
      withErrorToast(m.operations.deletePlaylist, () => commands.deletePlaylist(playlistId)),
    ...optimisticPlaylistUpdate<string>(queryClient, (playlists, playlistId) =>
      playlists.filter((pl) => pl.id !== playlistId)
    ),
    onSuccess: (_result, playlistId) => {
      // このプレイリストの並び順の設定は、もう使わない
      forgetTrackListView(playlistSortViewId(playlistId));
      showSuccess(m.notices.playlistDeleted);
    }
  }));
}

/**
 * M3U（M3U8）のファイルを選んで読み込み、プレイリストを作るミューテーション
 *
 * ファイルはRust側のダイアログで選ぶ。選ばなかった場合は、空の一覧が返る。
 * 結果（作ったプレイリスト・対応が付かなかった行）の表示は、呼び出し側が行う。
 */
export function useImportM3uMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () => withErrorToast(m.operations.importM3u, () => commands.importM3uPlaylists()),
    onSuccess: (results) => {
      if (results.some((result) => result.playlistId !== null)) {
        queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
      }
    }
  }));
}

/**
 * プレイリストを、保存先を選んでM3U8へ書き出すミューテーション
 *
 * 保存先はRust側のダイアログで選ぶ。選ばなかった場合は、何も通知しない。
 */
export function useExportPlaylistM3uMutation() {
  return createMutation(() => ({
    mutationFn: ({ playlistId, relativePaths }: { playlistId: string; relativePaths: boolean }) =>
      withErrorToast(m.operations.exportM3u, () =>
        commands.exportPlaylistM3u(playlistId, relativePaths)
      ),
    onSuccess: (result) => {
      if (result) showSuccess(m.notices.m3uExported(result.fileName, result.trackCount));
    }
  }));
}
