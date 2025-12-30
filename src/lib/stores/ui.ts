import { writable, type Writable } from 'svelte/store';

/**
 * UI状態を管理するストア
 */

// 表示モード（グリッド/リスト）
export type ViewMode = 'grid' | 'list';
export const viewMode: Writable<ViewMode> = writable('grid');

// ブラウズモード（データのグループ化方法）
export type BrowseMode = 'songs' | 'albums' | 'artists' | 'genres' | 'genre-detail';
export const browseMode: Writable<BrowseMode> = writable('songs');

// 選択中のジャンル名（genre-detailモード用）
export const selectedGenreName: Writable<string | null> = writable(null);

// サイドバーで展開中のジャンル（Set型）
export const expandedGenres: Writable<Set<string>> = writable(new Set());

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

// グリッドカードサイズ (50 - 200px)
export const gridCardSize: Writable<number> = writable(120);
export const MIN_CARD_SIZE = 50;
export const MAX_CARD_SIZE = 200;

// インポートダイアログの開閉状態
export const isImportDialogOpen: Writable<boolean> = writable(false);

// ブラウズ検索クエリ（アルバム/アーティスト/ジャンル名検索用）
export const browseSearchQuery: Writable<string> = writable('');

// メタデータエディタの開閉状態
export const isMetadataEditorOpen: Writable<boolean> = writable(false);

// 列幅のデフォルト値（ピクセル単位）
const DEFAULT_COLUMN_WIDTHS = {
  checkbox: 40,     // 2.5rem
  title: 300,       // 可変幅の基準
  artist: 200,      // 可変幅の基準
  rating: 80,       // 5rem
  duration: 64      // 4rem
};

// 列幅のストア（localStorageと同期）
function createColumnWidthsStore() {
  // localStorageから読み込み
  let initialWidths = { ...DEFAULT_COLUMN_WIDTHS };
  if (typeof window !== 'undefined') {
    try {
      const stored = localStorage.getItem('muspice:columnWidths');
      if (stored) {
        const parsed = JSON.parse(stored);
        initialWidths = { ...DEFAULT_COLUMN_WIDTHS, ...parsed };
      }
    } catch {
      // パースエラー時はデフォルト値を使用
    }
  }

  const { subscribe, set, update } = writable(initialWidths);

  return {
    subscribe,
    set: (value: typeof DEFAULT_COLUMN_WIDTHS) => {
      set(value);
      if (typeof window !== 'undefined') {
        localStorage.setItem('muspice:columnWidths', JSON.stringify(value));
      }
    },
    update: (fn: (value: typeof DEFAULT_COLUMN_WIDTHS) => typeof DEFAULT_COLUMN_WIDTHS) => {
      update((current) => {
        const newValue = fn(current);
        if (typeof window !== 'undefined') {
          localStorage.setItem('muspice:columnWidths', JSON.stringify(newValue));
        }
        return newValue;
      });
    },
    reset: () => {
      set(DEFAULT_COLUMN_WIDTHS);
      if (typeof window !== 'undefined') {
        localStorage.removeItem('muspice:columnWidths');
      }
    }
  };
}

export const columnWidths = createColumnWidthsStore();
export type ColumnWidths = typeof DEFAULT_COLUMN_WIDTHS;

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
