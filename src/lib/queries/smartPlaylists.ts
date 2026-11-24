/**
 * オートプレイリスト（条件で曲を集めるプレイリスト）のクエリとミューテーション（ADR-042・ADR-127）
 *
 * 通常のプレイリスト（`./playlists.ts`）とは別の枠組み。曲は条件から求めるため、曲の追加・削除・
 * 並べ替え・フォルダはない。
 */
import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { SmartPlaylist, SmartRules } from '#lib/types/models.js';
import { showSuccess } from '#lib/stores/error.svelte.js';
import { forgetTrackListView, smartPlaylistSortViewId } from '#lib/stores/trackListView.svelte.js';
import { queryKeys } from './keys.js';
import { CACHE_POLICY, withErrorToast } from './shared.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * オートプレイリスト一覧を取得するクエリ（手動の並び順。表示の並び順は画面が選ぶ）
 */
export function useSmartPlaylistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.smartPlaylists,
    queryFn: () =>
      withErrorToast(m.operations.fetchSmartPlaylists, () => commands.getSmartPlaylists())
  }));
}

/**
 * オートプレイリストの曲を取得するクエリ（条件に合う曲を、条件の並び順で）
 *
 * 曲は条件から求めるため、開くたびに取り直す（評価・再生回数などが変わると、合う曲が変わる。
 * 開いている間は、聴いている途中で曲が消えないよう、取り直さない。ADR-042）。
 */
export function useSmartPlaylistTracksQuery(smartPlaylistId: string) {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.smartPlaylist(smartPlaylistId),
    queryFn: () =>
      withErrorToast(m.operations.fetchTracks, () =>
        commands.getSmartPlaylistTracks(smartPlaylistId)
      ),
    ...CACHE_POLICY.detail,
    staleTime: 0,
    refetchOnMount: 'always' as const
  }));
}

/**
 * 条件に合う曲数を取得するクエリ（条件の編集画面で、保存する前に出す）
 */
export function useSmartPlaylistCountQuery(rules: SmartRules) {
  return createQuery(() => ({
    queryKey: queryKeys.smartPlaylistCount(rules),
    queryFn: () =>
      withErrorToast(m.operations.countSmartPlaylistTracks, () =>
        commands.countSmartPlaylistTracks(rules)
      ),
    // 編集画面を開いている間だけ使う（開き直したら、数え直す）
    staleTime: 0,
    gcTime: 0
  }));
}

/**
 * オートプレイリストを作成するミューテーション
 */
export function useCreateSmartPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ name, rules }: { name: string; rules: SmartRules }) =>
      withErrorToast(m.operations.createSmartPlaylist, () =>
        commands.createSmartPlaylist(name, rules)
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.smartPlaylists });
      showSuccess(m.notices.smartPlaylistCreated);
    }
  }));
}

/**
 * オートプレイリストの名前と条件を変更するミューテーション
 */
export function useUpdateSmartPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({
      smartPlaylist,
      name,
      rules
    }: {
      smartPlaylist: SmartPlaylist;
      name: string;
      rules: SmartRules;
    }) =>
      withErrorToast(m.operations.updateSmartPlaylist, () =>
        commands.updateSmartPlaylist(smartPlaylist.id, name, rules)
      ),
    onSuccess: (_result, { smartPlaylist }) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.smartPlaylists });
      // 条件を変えたオートプレイリストの曲は、すぐに求め直す
      void queryClient.invalidateQueries({
        queryKey: queryKeys.tracks.smartPlaylist(smartPlaylist.id)
      });
      // デバイスが同期するオートプレイリストの曲も変わる
      void queryClient.invalidateQueries({ queryKey: queryKeys.syncSelectionSummaries });
      showSuccess(m.notices.smartPlaylistUpdated);
    }
  }));
}

/**
 * オートプレイリストの名前と説明を変更するミューテーション
 */
export function useUpdateSmartPlaylistInfoMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({
      smartPlaylist,
      name,
      description
    }: {
      smartPlaylist: SmartPlaylist;
      name: string;
      /** 説明（空の文字列は、説明なし） */
      description: string;
    }) =>
      withErrorToast(m.operations.updatePlaylistInfo, () =>
        commands.updateSmartPlaylistInfo(smartPlaylist.id, name, description || null)
      ),
    onSuccess: () => {
      showSuccess(m.notices.playlistInfoUpdated);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.smartPlaylists });
    }
  }));
}

/**
 * オートプレイリストを、手動で並べ替えるミューテーション（Optimistic Update付き）
 */
export function useReorderSmartPlaylistsMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (smartPlaylistIds: string[]) =>
      withErrorToast(m.operations.movePlaylist, () =>
        commands.reorderSmartPlaylists(smartPlaylistIds)
      ),
    onMutate: async (smartPlaylistIds: string[]) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.smartPlaylists });
      const previous = queryClient.getQueryData<SmartPlaylist[]>(queryKeys.smartPlaylists);
      if (previous) {
        queryClient.setQueryData<SmartPlaylist[]>(
          queryKeys.smartPlaylists,
          previous.map((playlist) => {
            const index = smartPlaylistIds.indexOf(playlist.id);
            return index >= 0 ? { ...playlist, position: index } : playlist;
          })
        );
      }
      return { previous };
    },
    onError: (_error, _ids, context) => {
      if (context?.previous) queryClient.setQueryData(queryKeys.smartPlaylists, context.previous);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.smartPlaylists });
    }
  }));
}

/**
 * オートプレイリストを削除するミューテーション
 */
export function useDeleteSmartPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (smartPlaylistId: string) =>
      withErrorToast(m.operations.deleteSmartPlaylist, () =>
        commands.deleteSmartPlaylist(smartPlaylistId)
      ),
    onSuccess: (_result, smartPlaylistId) => {
      // このオートプレイリストの並び順の設定と曲のキャッシュは、もう使わない
      forgetTrackListView(smartPlaylistSortViewId(smartPlaylistId));
      queryClient.removeQueries({ queryKey: queryKeys.tracks.smartPlaylist(smartPlaylistId) });
      showSuccess(m.notices.smartPlaylistDeleted);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.smartPlaylists });
      // デバイスが同期するオートプレイリストから外れる
      void queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices });
      void queryClient.invalidateQueries({ queryKey: queryKeys.syncSelectionSummaries });
    }
  }));
}

/**
 * ランダムな並びのオートプレイリストを、選び直すミューテーション
 *
 * ランダムな並びは、選び直すまで変わらない（すべてのオートプレイリストの並びが変わる）。
 */
export function useReshuffleSmartPlaylistsMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.reshuffleSmartPlaylists, () =>
        commands.reshuffleSmartPlaylists()
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.tracks.smartPlaylists });
      void queryClient.invalidateQueries({ queryKey: queryKeys.syncSelectionSummaries });
    }
  }));
}

/**
 * オートプレイリストの今の曲（並び順のまま）で、通常のプレイリストを作るミューテーション
 */
export function useSaveSmartPlaylistAsPlaylistMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ smartPlaylistId, name }: { smartPlaylistId: string; name: string }) =>
      withErrorToast(m.operations.saveSmartPlaylistAsPlaylist, () =>
        commands.saveSmartPlaylistAsPlaylist(smartPlaylistId, name)
      ),
    onSuccess: (playlist) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
      showSuccess(m.notices.playlistCreatedWithTracks(playlist.name, playlist.tracks.length));
    }
  }));
}
