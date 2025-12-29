import { writable, type Writable } from 'svelte/store';

/**
 * UI状態を管理するストア
 */

// 表示モード（グリッド/リスト）
export type ViewMode = 'grid' | 'list';
export const viewMode: Writable<ViewMode> = writable('grid');

// ブラウズモード（データのグループ化方法）
export type BrowseMode = 'songs' | 'albums' | 'artists' | 'genres';
export const browseMode: Writable<BrowseMode> = writable('songs');

// 選択中のトラックID
export const selectedTracks: Writable<string[]> = writable([]);

// サイドバーの開閉状態
export const isSidebarOpen: Writable<boolean> = writable(true);

// 現在のビュー（ライブラリ/プレイリスト）
export type CurrentView = 'library' | 'playlist';
export const currentView: Writable<CurrentView> = writable('library');

// 選択中のプレイリストID
export const selectedPlaylistId: Writable<string | null> = writable(null);

// 検索クエリ
export const searchQuery: Writable<string> = writable('');

// インポートダイアログの開閉状態
export const isImportDialogOpen: Writable<boolean> = writable(false);

// メタデータエディタの開閉状態
export const isMetadataEditorOpen: Writable<boolean> = writable(false);

/**
 * トラックの選択状態をトグル
 */
export function toggleTrackSelection(trackId: string): void {
  selectedTracks.update((tracks) => {
    const index = tracks.indexOf(trackId);
    if (index === -1) {
      return [...tracks, trackId];
    } else {
      return tracks.filter((id) => id !== trackId);
    }
  });
}

/**
 * すべてのトラックを選択解除
 */
export function clearTrackSelection(): void {
  selectedTracks.set([]);
}

/**
 * 複数のトラックを選択
 */
export function selectTracks(trackIds: string[]): void {
  selectedTracks.set(trackIds);
}

/**
 * トラックが選択されているかチェック
 */
export function isTrackSelected(trackId: string, selected: string[]): boolean {
  return selected.includes(trackId);
}
