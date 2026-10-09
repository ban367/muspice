import {
  createMutation,
  createQuery,
  useQueryClient,
  type QueryClient
} from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type {
  AlbumGroup,
  Track,
  Metadata,
  DeleteResult,
  DuplicateAction,
  FilterOptions
} from '#lib/types/models.js';
import { handleError, showSuccess, showWarning } from '#lib/stores/error.svelte.js';
import { queryKeys } from './keys';
import { CACHE_POLICY, withErrorToast } from './shared';
import { patchTrackInCache, patchTracksInCache } from './trackCache';
import { m } from '#lib/i18n/i18n.svelte.js';

// 呼び出し側の利便性のため、このモジュールからも型を再エクスポートする
export type { DeleteResult, FilterOptions };

// ========== Query Invalidation ヘルパー ==========

/**
 * 曲の一覧と、アルバム・アーティスト・ジャンルの一覧を無効化（インポート・メタデータ編集時）
 *
 * `queryKeys.tracks.all`をプレフィックスに持つ全クエリ（全曲の一覧・検索・フィルタ・
 * アルバムやプレイリストの曲など）を無効化する。
 */
export function invalidateTrackListQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: queryKeys.tracks.all });
  queryClient.invalidateQueries({ queryKey: queryKeys.albums.list });
  queryClient.invalidateQueries({ queryKey: queryKeys.artists.list });
  queryClient.invalidateQueries({ queryKey: queryKeys.genres.list });
  queryClient.invalidateQueries({ queryKey: queryKeys.unique.all });
  // プレイリストは除外（プレイリストに入っている曲のIDは変わらない）
}

/**
 * 全トラック関連クエリとプレイリストを無効化（トラックの削除時、他のウィンドウでライブラリが変わった時）
 *
 * トラックを削除するとプレイリストの中からも消えるため、プレイリストも無効化する。
 */
export function invalidateAllTrackQueries(queryClient: QueryClient) {
  invalidateTrackListQueries(queryClient);
  queryClient.invalidateQueries({ queryKey: queryKeys.playlists });
}

// ========== 読み取りクエリ ==========

/**
 * 全曲の一覧を取得するクエリ
 *
 * 件数の上限はなく、ライブラリの全曲を1回で取得する（一覧は見えている行だけを描画する）。
 * 評価・お気に入りの変更では取り直さず、キャッシュの該当曲を書き換える（`./trackCache.ts`）。
 */
export function useTracksQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.list,
    queryFn: () => withErrorToast(m.operations.fetchTracks, () => commands.getAllTracks()),
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
    queryFn: () =>
      withErrorToast(m.operations.searchTracks, () => commands.searchTracks(searchTerm)),
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
    queryFn: () => withErrorToast(m.operations.filterTracks, () => commands.filterTracks(filters)),
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
 * お気に入りトラック一覧を取得するクエリ（最近お気に入りにした順）
 */
export function useFavoriteTracksQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.tracks.favorites,
    queryFn: () => withErrorToast(m.operations.fetchFavorites, () => commands.getFavoriteTracks()),
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
      withErrorToast(m.operations.fetchMostPlayed, () => commands.getMostPlayedTracks(limit)),
    ...CACHE_POLICY.playStats
  }));
}

/**
 * 再生履歴を取得するクエリ（新しい順。同じ曲が何度も出る）
 *
 * 再生した日時とトラックIDだけを返す。曲そのものは、全曲の一覧（`useTracksQuery`）から引く。
 */
export function usePlayHistoryQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.playHistory,
    queryFn: () => withErrorToast(m.operations.fetchPlayHistory, () => commands.getPlayHistory()),
    ...CACHE_POLICY.volatile
  }));
}

// ========== アルバム・アーティスト・ジャンルの一覧と、その曲 ==========
//
// 一覧（名前・曲数・代表の曲）と曲を分けて取得する。曲は、詳細を開いた時・再生する時に
// そのアルバムなどの分だけを取得する。

/** アルバム・アーティスト・ジャンルのどれかを指す種類 */
export type GroupType = 'album' | 'artist' | 'genre';

function albumTracksOptions(album: string, artist: string | null) {
  return {
    queryKey: queryKeys.tracks.album(album, artist),
    queryFn: () =>
      withErrorToast(m.operations.fetchTracks, () => commands.getAlbumTracks(album, artist)),
    ...CACHE_POLICY.detail
  };
}

function artistAlbumsOptions(artist: string) {
  return {
    queryKey: queryKeys.tracks.artistAlbums(artist),
    queryFn: () => withErrorToast(m.operations.fetchAlbums, () => commands.getArtistAlbums(artist)),
    ...CACHE_POLICY.detail
  };
}

function genreTracksOptions(genre: string) {
  return {
    queryKey: queryKeys.tracks.genre(genre),
    queryFn: () => withErrorToast(m.operations.fetchTracks, () => commands.getGenreTracks(genre)),
    ...CACHE_POLICY.detail
  };
}

/** アルバムごとの曲を、表示順の1つの一覧にする */
export function flattenAlbumTracks(albums: AlbumGroup[]): Track[] {
  return albums.flatMap((album) => album.tracks);
}

/**
 * アルバムの一覧を取得（曲は含まない）
 */
export function useAlbumsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.albums.list,
    queryFn: () => withErrorToast(m.operations.fetchAlbums, () => commands.getAlbums()),
    ...CACHE_POLICY.library
  }));
}

/**
 * アルバムの曲を取得（ディスク番号・トラック番号の順）
 * @param album - アルバム名
 * @param artist - アルバムをまとめたアーティスト（`AlbumSummary`の`artist`）
 */
export function useAlbumTracksQuery(album: string, artist: string | null) {
  return createQuery(() => albumTracksOptions(album, artist));
}

/**
 * アーティストの一覧を取得（アルバムと曲は含まない）
 */
export function useArtistsQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.artists.list,
    queryFn: () => withErrorToast(m.operations.fetchArtists, () => commands.getArtists()),
    ...CACHE_POLICY.library
  }));
}

/**
 * アーティストのアルバムと曲を取得
 */
export function useArtistAlbumsQuery(artist: string) {
  return createQuery(() => artistAlbumsOptions(artist));
}

/**
 * ジャンルの一覧を取得（曲は含まない）
 */
export function useGenresQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.genres.list,
    queryFn: () => withErrorToast(m.operations.fetchGenres, () => commands.getGenres()),
    ...CACHE_POLICY.library
  }));
}

/**
 * アルバム・アーティスト・ジャンルの曲を、表示順の1つの一覧で取得するクエリ
 *
 * アーティストは、アルバムごとの曲（`useArtistAlbumsQuery`と同じキャッシュ）を1つの一覧にする。
 * @param type - 種類
 * @param name - アルバム・アーティスト・ジャンルの名前
 * @param albumArtist - アルバムの場合の、アルバムをまとめたアーティスト（`AlbumSummary`の`artist`）
 */
export function useGroupTracksQuery(
  type: GroupType,
  name: string,
  albumArtist: string | null = null
) {
  switch (type) {
    case 'album':
      return createQuery(() => albumTracksOptions(name, albumArtist));
    case 'artist':
      return createQuery(() => ({ ...artistAlbumsOptions(name), select: flattenAlbumTracks }));
    case 'genre':
      return createQuery(() => genreTracksOptions(name));
  }
}

/**
 * アルバム・アーティスト・ジャンルの曲を取得する（再生などの操作で使う。キャッシュがあれば使う）
 */
export async function fetchGroupTracks(
  queryClient: QueryClient,
  type: GroupType,
  name: string,
  albumArtist: string | null = null
): Promise<Track[]> {
  switch (type) {
    case 'album':
      return queryClient.fetchQuery(albumTracksOptions(name, albumArtist));
    case 'artist':
      return flattenAlbumTracks(await queryClient.fetchQuery(artistAlbumsOptions(name)));
    case 'genre':
      return queryClient.fetchQuery(genreTracksOptions(name));
  }
}

// ========== 再生統計ミューテーション ==========

/**
 * 曲をお気に入りにする・お気に入りから外すミューテーション（複数の曲をまとめて指定できる）
 *
 * 成功したら、キャッシュにあるその曲のお気に入りの状態を書き換える（曲の一覧は取り直さない）。
 */
export function useSetFavoriteMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackIds, favorite }: { trackIds: string[]; favorite: boolean }) => {
      await withErrorToast(m.operations.setFavorite, () =>
        commands.setFavorite(trackIds, favorite)
      );
    },
    onSuccess: (_result, { trackIds, favorite }) => {
      patchTracksInCache(queryClient, trackIds, { isFavorite: favorite });
      // お気に入りの一覧は、入っている曲と並びが変わるため取り直す
      queryClient.invalidateQueries({ queryKey: queryKeys.tracks.favorites });
    }
  }));
}

/**
 * レーティングを設定するミューテーション
 *
 * 成功したら、キャッシュにあるその曲の評価を書き換える（曲の一覧は取り直さない）。
 */
export function useSetRatingMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackId, rating }: { trackId: string; rating: number }) => {
      await withErrorToast(m.operations.setRating, () => commands.setRating(trackId, rating));
    },
    onSuccess: (_result, { trackId, rating }) => {
      patchTrackInCache(queryClient, trackId, { rating });
    }
  }));
}

/**
 * 再生回数に数える（再生コントローラーが、曲の半分か4分を聴いた時に呼ぶ）
 *
 * 再生を止めないよう、失敗は通知しない。`queryClient`を渡すと、キャッシュにあるその曲の
 * 再生回数を書き換え、再生履歴と「よく再生する曲」を取り直す。
 */
export async function recordPlay(trackId: string, queryClient?: QueryClient): Promise<void> {
  try {
    const playCount = await commands.incrementPlayCount(trackId);
    if (!queryClient) return;
    patchTrackInCache(queryClient, trackId, {
      playCount,
      lastPlayedAt: new Date().toISOString()
    });
    void queryClient.invalidateQueries({ queryKey: queryKeys.playHistory });
    void queryClient.invalidateQueries({ queryKey: queryKeys.tracks.mostPlayedAll });
  } catch (error) {
    console.debug('再生回数の記録に失敗しました:', error);
  }
}

/**
 * スキップ回数に数える（再生コントローラーが、再生回数に数える前に別の曲へ移った時に呼ぶ）
 *
 * 再生を止めないよう、失敗は通知しない。`queryClient`を渡すと、キャッシュにあるその曲の
 * スキップ回数を書き換える。
 */
export async function recordSkip(trackId: string, queryClient?: QueryClient): Promise<void> {
  try {
    const skipCount = await commands.incrementSkipCount(trackId);
    if (queryClient) patchTrackInCache(queryClient, trackId, { skipCount });
  } catch (error) {
    console.debug('スキップ回数の記録に失敗しました:', error);
  }
}

// ========== メタデータ更新ミューテーション ==========

/**
 * 単一トラックのメタデータを更新するミューテーション
 *
 * 常にファイルのタグへ書き込み、同じ内容をDBに記録する。
 * 失敗は編集画面の中に表示するため、トースト通知はしない。
 */
export function useUpdateTrackMetadataMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: async ({ trackId, metadata }: { trackId: string; metadata: Metadata }) => {
      await commands.updateTrackMetadata(trackId, metadata);
    },
    onSuccess: () => {
      invalidateTrackListQueries(queryClient);
    }
  }));
}

/**
 * 複数トラックのメタデータを一括更新するミューテーション
 *
 * 値がある項目だけを、各トラックのファイルのタグへ書き込む。書き込めなかったトラックは
 * 結果の`failedCount`・`errors`で返る（残りのトラックは更新される）。
 */
export function useUpdateMultipleTracksMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({ trackIds, metadata }: { trackIds: string[]; metadata: Metadata }) =>
      commands.updateMultipleTracksMetadata(trackIds, metadata),
    onSuccess: () => {
      invalidateTrackListQueries(queryClient);
    }
  }));
}

/**
 * アプリ内（DB）だけにある編集内容・評価を、ファイルのタグへ書き出すミューテーション
 *
 * 書き出した後、ファイルを読み直してDBに反映する（終わるとDBはファイルの内容と一致する）。
 */
export function useWriteLibraryMetadataToFilesMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.writeMetadataToFiles, () =>
        commands.writeLibraryMetadataToFiles()
      ),
    onSuccess: () => {
      invalidateTrackListQueries(queryClient);
    }
  }));
}

/**
 * ライブラリのXML（iTunes形式）を選んで取り込むミューテーション
 *
 * ほかのプレーヤーの再生回数・最終再生日・追加日・お気に入りと、プレイリストを取り込む。
 * ファイルはRust側のダイアログで選ぶ。選ばなかった場合はnullが返る。
 * 結果の表示は、呼び出し側が行う。
 */
export function useImportLibraryXmlMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: (includePlaylists: boolean) =>
      withErrorToast(m.operations.importLibraryXml, () =>
        commands.importLibraryXml(includePlaylists)
      ),
    onSuccess: (result) => {
      if (result) invalidateAllTrackQueries(queryClient);
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
      return withErrorToast(m.operations.deleteTracks, () =>
        commands.deleteTracksCommand(trackIds)
      );
    },
    onSuccess: (deletedCount) => {
      invalidateAllTrackQueries(queryClient);
      showSuccess(m.notices.tracksRemovedFromLibrary(deletedCount));
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
      return withErrorToast(m.operations.deleteTracksAndFiles, () =>
        commands.deleteTracksWithFilesCommand(trackIds)
      );
    },
    onSuccess: (result) => {
      invalidateAllTrackQueries(queryClient);

      if (result.failedCount === 0) {
        showSuccess(m.notices.tracksAndFilesDeleted(result.successCount));
      } else if (result.successCount === 0) {
        handleError(new Error(m.notices.allTracksDeleteFailed), m.operations.deleteTracks);
      } else {
        showWarning(m.notices.tracksPartiallyDeleted(result.successCount, result.failedCount));
      }
    }
  }));
}

// ========== ライブラリ管理ミューテーション ==========

/**
 * フォルダから音楽ファイルをインポートするミューテーション
 *
 * 進捗は`events.importProgress`で受け取る。失敗はインポートダイアログ内に
 * 表示するため、トースト通知はしない。
 */
export function useImportFolderMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: ({
      folderPath,
      duplicateAction
    }: {
      folderPath: string;
      duplicateAction: DuplicateAction;
    }) => commands.importFolder(folderPath, duplicateAction),
    onSuccess: () => {
      invalidateTrackListQueries(queryClient);
    }
  }));
}

/**
 * ライブラリ全体のメタデータ（タグの内容・評価）をファイルから読み直すミューテーション
 */
export function useRefreshLibraryMetadataMutation() {
  const queryClient = useQueryClient();

  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.refreshMetadata, () => commands.refreshLibraryMetadata()),
    onSuccess: () => {
      invalidateTrackListQueries(queryClient);
    }
  }));
}

/**
 * トラックのファイルをファイルマネージャーで表示するミューテーション
 *
 * キャッシュには影響しないが、失敗をトーストで通知するためミューテーションとして扱う。
 */
export function useShowInFolderMutation() {
  return createMutation(() => ({
    mutationFn: (trackId: string) =>
      withErrorToast(m.operations.showInFolder, () => commands.showInFolder(trackId))
  }));
}
