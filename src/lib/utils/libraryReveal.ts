/**
 * 曲から、そのアルバム・アーティストの画面へ移る（ADR-110）
 *
 * 移る先の画面（アルバム・アーティストの画面）は、`ui.revealGroup`に置いた項目を選び、
 * 見える位置へ出して、`null`に戻す（グリッド表示は`LibraryGrid`、リスト表示は`LibraryBrowsePage`）。
 * 曲の一覧の再生中の曲へのジャンプ（`ui.revealTrackId`）と同じ形。
 */

import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { AlbumSummary, ArtistSummary, Track } from '#lib/types/models.js';
import { ui } from '#lib/stores/ui.svelte.js';
import { albumArtistKey } from './albumArtists.js';
import { albumKey, groupArtist, VARIOUS_ARTISTS } from './albumKey.js';
import { splitValues } from './multiValue.js';

/** 移る先の項目（アルバムは名前とまとめたアーティスト、アーティストは名前） */
export type RevealGroup =
  { kind: 'album'; name: string; artist: string | null } | { kind: 'artist'; name: string };

/** 曲のアルバム（アルバムのない曲はnull） */
export function albumOf(
  track: Pick<Track, 'album' | 'albumArtist' | 'artist' | 'compilation'>
): Extract<RevealGroup, { kind: 'album' }> | null {
  if (!track.album) return null;
  return { kind: 'album', name: track.album, artist: groupArtist(track) };
}

/**
 * 曲をアーティストの画面でまとめるアーティスト（複数ある曲は、それぞれ。ADR-022・ADR-093）
 *
 * アルバムアーティストがあれば、それだけ。なければ、コンピレーションの曲は`Various Artists`、
 * ほかの曲は曲のアーティストのそれぞれの値（アーティストの画面の一覧と同じまとめ方）。
 */
export function artistsOf(track: Pick<Track, 'albumArtist' | 'artist' | 'compilation'>): string[] {
  if (track.albumArtist !== null) return [track.albumArtist];
  if (track.compilation) return [VARIOUS_ARTISTS];
  return splitValues(track.artist);
}

/** アルバムの画面のグリッドの項目のキー（`AlbumGrid`の`albumKey`） */
export function revealAlbumKey(request: Extract<RevealGroup, { kind: 'album' }>): string {
  return albumKey(request);
}

/** アルバムの画面のリスト表示の、左の一覧の項目（アルバムアーティスト）のキー */
export function revealAlbumArtistKey(request: Extract<RevealGroup, { kind: 'album' }>): string {
  return albumArtistKey(request.artist);
}

/** グリッドの項目が、移る先の項目か */
export function matchesReveal(
  request: RevealGroup,
  item: Pick<AlbumSummary, 'name' | 'artist'> | Pick<ArtistSummary, 'name'>
): boolean {
  if (request.kind === 'album') {
    return 'artist' in item && item.name === request.name && item.artist === request.artist;
  }
  return item.name === request.name;
}

/** アルバムの画面を開いて、曲のアルバムを選ぶ（アルバムのない曲では何もしない） */
export async function goToAlbum(
  track: Pick<Track, 'album' | 'albumArtist' | 'artist' | 'compilation'>
): Promise<void> {
  const album = albumOf(track);
  if (album === null) return;
  await reveal(album, 'library/albums');
}

/** アーティストの画面を開いて、そのアーティストを選ぶ */
export async function goToArtist(name: string): Promise<void> {
  await reveal({ kind: 'artist', name }, 'library/artists');
}

async function reveal(
  request: RevealGroup,
  path: 'library/albums' | 'library/artists'
): Promise<void> {
  // Now Playingの画面は閉じ、検索で絞り込んでいれば外す（選ぶ項目が一覧から外れないように）
  ui.isNowPlayingOpen = false;
  ui.browseSearchQuery = '';
  ui.revealGroup = request;
  await goto(resolve(path));
}
