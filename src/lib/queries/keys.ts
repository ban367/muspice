/**
 * TanStack Queryのクエリキー定義
 *
 * キーの文字列をここに集約し、クエリ側と無効化側で同じ定義を参照する。
 * 無効化はプレフィックス一致で動作するため、階層構造がそのまま
 * 「どこまでまとめて無効化されるか」を表す。
 *
 * 例: `queryKeys.tracks.all`（`['tracks']`）の無効化は、全曲の一覧・検索・フィルタ・
 * アルバムやプレイリストの曲など`['tracks', ...]`で始まる全クエリに波及する。
 */

import type { FilterOptions } from '#lib/types/models.js';

export const queryKeys = {
  /**
   * 曲を返すクエリ（全曲の一覧・検索・フィルタ・再生統計・アルバムなどの曲）
   *
   * 曲そのものを返すクエリはすべてここに置く（評価などの変更を、`['tracks']`以下の
   * キャッシュの書き換えでまとめて反映するため。`./trackCache.ts`）。
   */
  tracks: {
    all: ['tracks'] as const,
    list: ['tracks', 'list'] as const,
    search: (term: string) => ['tracks', 'search', term] as const,
    filter: (filters: FilterOptions) => ['tracks', 'filter', filters] as const,
    favorites: ['tracks', 'favorites'] as const,
    mostPlayedAll: ['tracks', 'mostPlayed'] as const,
    mostPlayed: (limit: number) => ['tracks', 'mostPlayed', limit] as const,
    album: (album: string, artist: string | null) => ['tracks', 'album', album, artist] as const,
    artistAlbums: (artist: string) => ['tracks', 'artistAlbums', artist] as const,
    genre: (genre: string) => ['tracks', 'genre', genre] as const,
    playlists: ['tracks', 'playlist'] as const,
    playlist: (playlistId: string) => ['tracks', 'playlist', playlistId] as const
  },

  /** アルバム・アーティスト・ジャンルの一覧（名前・曲数・代表の曲。曲は含まない） */
  albums: {
    list: ['albums', 'list'] as const
  },
  artists: {
    list: ['artists', 'list'] as const
  },
  genres: {
    list: ['genres', 'list'] as const
  },

  /** ユニーク値一覧（フィルタの選択肢） */
  unique: {
    all: ['unique'] as const,
    artists: ['unique', 'artists'] as const,
    albums: ['unique', 'albums'] as const,
    genres: ['unique', 'genres'] as const
  },

  /**
   * 再生履歴（再生した日時とトラックIDの一覧。曲そのものは含まず、全曲の一覧から引く）
   */
  playHistory: ['playHistory'] as const,

  /** プレイリスト一覧 */
  playlists: ['playlists'] as const,

  /** アプリケーション設定 */
  settings: ['settings'] as const,
  outputDevices: ['outputDevices'] as const,

  /** ライブラリフォルダ（インポートしたフォルダ） */
  libraryFolders: ['libraryFolders'] as const,

  /** 転送先デバイス（SDカードなどのフォルダ） */
  syncDevices: ['syncDevices'] as const
} as const;
