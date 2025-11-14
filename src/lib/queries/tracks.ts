import { createQuery } from '@tanstack/svelte-query';
import { invoke } from '@tauri-apps/api/core';
import type { Track } from '$lib/types/models';

/**
 * トラック一覧を取得するクエリ
 */
export function useTracksQuery() {
  return createQuery(() => ({
    queryKey: ['tracks'],
    queryFn: async () => {
      return await invoke<Track[]>('get_all_tracks');
    },
    staleTime: 5 * 60 * 1000 // 5分間キャッシュ
  }));
}

/**
 * 検索クエリ
 */
export function useSearchQuery(searchTerm: string) {
  return createQuery(() => ({
    queryKey: ['tracks', 'search', searchTerm],
    queryFn: async () => {
      return await invoke<Track[]>('search_tracks', { query: searchTerm });
    },
    enabled: searchTerm.length > 0
  }));
}
