/**
 * アルバムアートキャッシュストア
 * 全コンポーネントで共有されるアルバムアート画像のキャッシュ
 */

import { writable } from 'svelte/store';
import { getAlbumArt } from '$lib/queries/tracks';

// グローバルキャッシュマップ（trackId -> data URL）
const cache = new Map<string, string | null>();

// 現在読み込み中のトラックID
const loading = new Set<string>();

// キャッシュ更新を通知するためのストア
export const albumArtCacheVersion = writable(0);

/**
 * アルバムアートを取得（キャッシュがあれば即座に返す）
 * @param trackId - トラックID
 * @returns キャッシュされたアルバムアートのdata URL、またはnull
 */
export function getCachedAlbumArt(trackId: string): string | null {
  return cache.get(trackId) ?? null;
}

/**
 * アルバムアートがキャッシュされているかチェック
 * @param trackId - トラックID
 * @returns キャッシュに存在するかどうか
 */
export function hasAlbumArt(trackId: string): boolean {
  return cache.has(trackId);
}

/**
 * アルバムアートを読み込み、キャッシュに追加
 * 重複読み込みを防止し、非同期で実行
 * @param trackId - トラックID
 */
export async function loadAlbumArt(trackId: string): Promise<void> {
  // 既にキャッシュ済みまたは読み込み中の場合はスキップ
  if (cache.has(trackId) || loading.has(trackId)) {
    return;
  }

  loading.add(trackId);

  try {
    const art = await getAlbumArt(trackId);
    if (art?.data && art?.mimeType) {
      const dataUrl = `data:${art.mimeType};base64,${art.data}`;
      cache.set(trackId, dataUrl);
    } else {
      cache.set(trackId, null);
    }

    // キャッシュ更新を通知
    albumArtCacheVersion.update((v) => v + 1);
  } catch {
    cache.set(trackId, null);
    albumArtCacheVersion.update((v) => v + 1);
  } finally {
    loading.delete(trackId);
  }
}

/**
 * 複数のトラックのアルバムアートを一括読み込み
 * @param trackIds - トラックIDの配列
 */
export async function loadAlbumArts(trackIds: string[]): Promise<void> {
  const promises = trackIds
    .filter((id) => !cache.has(id) && !loading.has(id))
    .map((id) => loadAlbumArt(id));

  await Promise.all(promises);
}

/**
 * キャッシュをクリア
 */
export function clearAlbumArtCache(): void {
  cache.clear();
  loading.clear();
  albumArtCacheVersion.update((v) => v + 1);
}

/**
 * 特定のトラックのキャッシュを削除
 * @param trackId - トラックID
 */
export function invalidateAlbumArt(trackId: string): void {
  cache.delete(trackId);
  albumArtCacheVersion.update((v) => v + 1);
}

/**
 * キャッシュサイズを取得
 * @returns キャッシュされているアイテム数
 */
export function getAlbumArtCacheSize(): number {
  return cache.size;
}
