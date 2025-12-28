import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
import { invoke } from '@tauri-apps/api/core';
import type { Playlist } from '$lib/types/models';
import { handleError, showSuccess } from '$lib/stores/error';

/**
 * プレイリスト一覧を取得するクエリ
 */
export function usePlaylistsQuery() {
	return createQuery(() => ({
		queryKey: ['playlists'],
		queryFn: async () => {
			try {
				return await invoke<Playlist[]>('get_playlists');
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
				return await invoke<Playlist>('create_playlist', { name });
			} catch (error) {
				handleError(error, 'プレイリストの作成');
				throw error;
			}
		},
		onSuccess: () => {
			// プレイリスト一覧を再取得
			queryClient.invalidateQueries({ queryKey: ['playlists'] });
			showSuccess('プレイリストを作成しました');
		}
	}));
}

/**
 * プレイリストにトラックを追加するミューテーション
 */
export function useAddTrackToPlaylistMutation() {
	const queryClient = useQueryClient();

	return createMutation(() => ({
		mutationFn: async ({ playlistId, trackId }: { playlistId: string; trackId: string }) => {
			try {
				await invoke('add_track_to_playlist', { playlistId, trackId });
			} catch (error) {
				handleError(error, 'トラックの追加');
				throw error;
			}
		},
		onSuccess: () => {
			// プレイリスト一覧を再取得
			queryClient.invalidateQueries({ queryKey: ['playlists'] });
			showSuccess('トラックをプレイリストに追加しました');
		}
	}));
}

/**
 * プレイリストからトラックを削除するミューテーション
 */
export function useRemoveTrackFromPlaylistMutation() {
	const queryClient = useQueryClient();

	return createMutation(() => ({
		mutationFn: async ({ playlistId, trackId }: { playlistId: string; trackId: string }) => {
			try {
				await invoke('remove_track_from_playlist', { playlistId, trackId });
			} catch (error) {
				handleError(error, 'トラックの削除');
				throw error;
			}
		},
		onSuccess: () => {
			// プレイリスト一覧を再取得
			queryClient.invalidateQueries({ queryKey: ['playlists'] });
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
				await invoke('reorder_playlist_tracks', { playlistId, trackIds });
			} catch (error) {
				handleError(error, 'トラックの並び替え');
				throw error;
			}
		},
		onSuccess: () => {
			// プレイリスト一覧を再取得
			queryClient.invalidateQueries({ queryKey: ['playlists'] });
			showSuccess('トラックを並び替えました');
		}
	}));
}
