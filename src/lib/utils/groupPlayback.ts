/**
 * アルバム・アーティスト・ジャンルの再生
 *
 * 一覧（カード・行）は曲を持たないため、再生する時にその分の曲を取得する。
 */
import type { QueryClient } from '@tanstack/svelte-query';
import { fetchGroupTracks, type GroupType } from '#lib/queries/tracks.js';
import { playNow } from '#lib/stores/player.svelte.js';

/**
 * アルバム・アーティスト・ジャンルの曲を取得して、再生中の曲の次に入れ、最初の曲から再生する
 *
 * キューのほかの曲は残す。取得に失敗した場合は再生しない（失敗はクエリ側がトーストで通知する）。
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
    playNow(tracks);
  } catch (error) {
    console.error('Failed to fetch tracks:', error);
  }
}
