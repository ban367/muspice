import { writable, type Writable } from 'svelte/store';

/**
 * UI状態を管理するストア
 */

// 表示モード（グリッド/リスト）
export type ViewMode = 'grid' | 'list';
export const viewMode: Writable<ViewMode> = writable('grid');

// 選択中のトラックID
export const selectedTracks: Writable<string[]> = writable([]);

// サイドバーの開閉状態
export const isSidebarOpen: Writable<boolean> = writable(true);

// 右サイドバーの展開状態
export const isRightSidebarExpanded: Writable<boolean> = writable(false);

// 右サイドバーのアクティブパネル（queue: 再生キュー, equalizer: イコライザ）
export type RightSidebarPanel = 'queue' | 'equalizer';
export const activeRightSidebarPanel: Writable<RightSidebarPanel> = writable('queue');

// 右サイドバーの固定状態（固定時は明示的に閉じるまで表示され続ける）
function createPinnedStore() {
  let initialValue = false;
  if (typeof window !== 'undefined') {
    try {
      const stored = localStorage.getItem('muspice:rightSidebarPinned');
      if (stored !== null) {
        initialValue = JSON.parse(stored);
      }
    } catch {
      // パースエラー時はデフォルト値を使用
    }
  }

  const { subscribe, set, update } = writable(initialValue);

  return {
    subscribe,
    set: (value: boolean) => {
      set(value);
      if (typeof window !== 'undefined') {
        localStorage.setItem('muspice:rightSidebarPinned', JSON.stringify(value));
      }
      // 固定化した時は自動的に展開状態にする
      if (value) {
        isRightSidebarExpanded.set(true);
      }
    },
    toggle: () => {
      update((current) => {
        const newValue = !current;
        if (typeof window !== 'undefined') {
          localStorage.setItem('muspice:rightSidebarPinned', JSON.stringify(newValue));
        }
        if (newValue) {
          isRightSidebarExpanded.set(true);
        }
        return newValue;
      });
    }
  };
}

export const isRightSidebarPinned = createPinnedStore();

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

// Aboutダイアログの開閉状態
export const isAboutDialogOpen: Writable<boolean> = writable(false);

// 列幅のデフォルト値（ピクセル単位）
const DEFAULT_COLUMN_WIDTHS = {
  number: 48, // トラック番号（再生中アイコンも表示）
  title: 300, // 可変幅の基準
  artist: 200, // 可変幅の基準
  rating: 80, // 5rem
  duration: 64 // 4rem
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
        // 古い不要なキーを削除（マイグレーション）
        if ('checkbox' in parsed) {
          delete parsed.checkbox;
        }
        if ('status' in parsed) {
          delete parsed.status;
        }
        // 新しいキーがない場合はデフォルト値を使用
        initialWidths = {
          number: parsed.number ?? DEFAULT_COLUMN_WIDTHS.number,
          title: parsed.title ?? DEFAULT_COLUMN_WIDTHS.title,
          artist: parsed.artist ?? DEFAULT_COLUMN_WIDTHS.artist,
          rating: parsed.rating ?? DEFAULT_COLUMN_WIDTHS.rating,
          duration: parsed.duration ?? DEFAULT_COLUMN_WIDTHS.duration
        };
        // 更新された値を保存
        localStorage.setItem('muspice:columnWidths', JSON.stringify(initialWidths));
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
