/**
 * アルバムアートキャッシュストア
 * 全コンポーネントで共有されるアルバムアート画像のキャッシュ
 * Svelte 5のリアクティビティに対応
 */

import { writable, get } from 'svelte/store';
import { getAlbumArt } from '$lib/queries/tracks';

// キャッシュデータ型
interface AlbumArtCacheData {
  [trackId: string]: string | null;
}

// リアクティブなキャッシュストア
const cacheStore = writable<AlbumArtCacheData>({});

// 現在読み込み中のトラックID
const loading = new Set<string>();

// キャッシュストアをエクスポート（コンポーネント内で購読するため）
export const albumArtCache = {
  subscribe: cacheStore.subscribe
};

// 後方互換性のためのバージョンストア（非推奨、cacheStoreを使用してください）
export const albumArtCacheVersion = writable(0);

/**
 * アルバムアートを取得（キャッシュがあれば即座に返す）
 * @param trackId - トラックID
 * @returns キャッシュされたアルバムアートのdata URL、またはnull
 */
export function getCachedAlbumArt(trackId: string): string | null {
  const cache = get(cacheStore);
  return cache[trackId] ?? null;
}

/**
 * アルバムアートがキャッシュされているかチェック
 * キャッシュにnull（アルバムアートなし）が設定されている場合はfalseを返す
 * @param trackId - トラックID
 * @returns キャッシュに有効なアルバムアートが存在するかどうか
 */
export function isCached(trackId: string): boolean {
  const cache = get(cacheStore);
  const value = cache[trackId];
  return value !== null && value !== undefined;
}

/**
 * アルバムアートを読み込み、キャッシュに追加
 * 重複読み込みを防止し、非同期で実行
 * @param trackId - トラックID
 */
export async function loadAlbumArt(trackId: string): Promise<void> {
  const cache = get(cacheStore);
  
  // 既にキャッシュ済みまたは読み込み中の場合はスキップ
  if (trackId in cache || loading.has(trackId)) {
    return;
  }

  loading.add(trackId);

  try {
    const art = await getAlbumArt(trackId);
    let dataUrl: string | null = null;
    
    if (art?.data && art?.mimeType) {
      dataUrl = `data:${art.mimeType};base64,${art.data}`;
    }

    // キャッシュを更新（リアクティブに通知）
    cacheStore.update((c) => ({ ...c, [trackId]: dataUrl }));
    albumArtCacheVersion.update((v) => v + 1);
  } catch {
    // エラー時はnullをキャッシュ
    cacheStore.update((c) => ({ ...c, [trackId]: null }));
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
  const cache = get(cacheStore);
  const promises = trackIds
    .filter((id) => !(id in cache) && !loading.has(id))
    .map((id) => loadAlbumArt(id));

  await Promise.all(promises);
}

/**
 * キャッシュをクリア
 */
export function clearAlbumArtCache(): void {
  cacheStore.set({});
  loading.clear();
  albumArtCacheVersion.update((v) => v + 1);
}

/**
 * 特定のトラックのキャッシュを削除
 * @param trackId - トラックID
 */
export function invalidateAlbumArt(trackId: string): void {
  cacheStore.update((c) => {
    const newCache = { ...c };
    delete newCache[trackId];
    return newCache;
  });
  albumArtCacheVersion.update((v) => v + 1);
}

/**
 * キャッシュサイズを取得
 * @returns キャッシュされているアイテム数
 */
export function getAlbumArtCacheSize(): number {
  const cache = get(cacheStore);
  return Object.keys(cache).length;
}
