/**
 * TanStack Queryのクエリキー定義
 *
 * キーの文字列をここに集約し、クエリ側と無効化側で同じ定義を参照する。
 * 無効化はプレフィックス一致で動作するため、階層構造がそのまま
 * 「どこまでまとめて無効化されるか」を表す。
 *
 * 例: `queryKeys.tracks.all`（`['tracks']`）の無効化は、検索・フィルタ・
 * お気に入りなど`['tracks', ...]`で始まる全クエリに波及する。
 */

import type { FilterOptions } from '$lib/types/models';

export const queryKeys = {
  /** トラック関連（一覧・検索・フィルタ・再生統計） */
  tracks: {
    all: ['tracks'] as const,
    search: (term: string) => ['tracks', 'search', term] as const,
    filter: (filters: FilterOptions) => ['tracks', 'filter', filters] as const,
    favorites: ['tracks', 'favorites'] as const,
    mostPlayed: (limit: number) => ['tracks', 'mostPlayed', limit] as const,
    recentlyPlayed: (limit: number) => ['tracks', 'recentlyPlayed', limit] as const
  },

  /** グループ化された一覧 */
  albums: {
    grouped: ['albums', 'grouped'] as const
  },
  artists: {
    grouped: ['artists', 'grouped'] as const
  },
  genres: {
    grouped: ['genres', 'grouped'] as const
  },

  /** ユニーク値一覧（フィルタの選択肢） */
  unique: {
    all: ['unique'] as const,
    artists: ['unique', 'artists'] as const,
    albums: ['unique', 'albums'] as const,
    genres: ['unique', 'genres'] as const
  },

  /** アルバムアート（トラックIDごと） */
  albumArt: (trackId: string | null) => ['albumArt', trackId] as const,

  /** プレイリスト一覧 */
  playlists: ['playlists'] as const
} as const;
