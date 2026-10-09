/**
 * アルバム・アーティスト・ジャンルの再生
 *
 * 一覧（カード・行）は曲を持たないため、再生する時にその分の曲を取得する。
 */
import type { QueryClient } from '@tanstack/svelte-query';
import { fetchGroupTracks, type GroupType } from '#lib/queries/tracks.js';
import { playTrackFromQueue } from '#lib/stores/player.svelte.js';

/**
 * アルバム・アーティスト・ジャンルの曲を取得して、最初の曲から再生する
 *
 * 取得に失敗した場合は再生しない（失敗はクエリ側がトーストで通知する）。
 * @param albumArtist - アルバムの場合の、アルバムをまとめたアーティスト（`AlbumSummary`の`artist`）
 */
export async function playGroup(
  queryClient: QueryClient,
  type: GroupType,
  name: string,
  albumArtist: string | null = null
): Promise<void> {
  try {
    const tracks = await fetchGroupTracks(queryClient, type, name, albumArtist);
    if (tracks.length > 0) {
      playTrackFromQueue(tracks, 0);
    }
  } catch (error) {
    console.error('曲の取得に失敗しました:', error);
  }
}
