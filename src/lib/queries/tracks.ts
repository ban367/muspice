import { createQuery } from '@tanstack/svelte-query';
import { invoke } from '@tauri-apps/api/core';
import type { Track, AlbumArt } from '$lib/types/models';
import { handleError } from '$lib/stores/error';

export interface FilterOptions {
  artist?: string;
  album?: string;
  genre?: string;
}

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
    refetchOnWindowFocus: false
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
    refetchOnWindowFocus: false
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
