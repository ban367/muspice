/**
 * 曲の画面の絞り込みの状態（カラムブラウザの表示と選択・フィルタ）
 *
 * カラムブラウザを出すかどうかは、localStorageに保存する。選択とフィルタは、アプリを開いている
 * 間だけ覚える（ほかの画面へ移って戻っても残る。起動し直すと、すべての曲から始まる）。
 */
import {
  EMPTY_FILTERS,
  EMPTY_SELECTION,
  countActiveFilters,
  hasBrowserSelection,
  type BrowserSelection,
  type TrackFilters
} from '#lib/utils/trackFilter.js';

const VISIBLE_KEY = 'muspice:columnBrowserVisible';

function loadVisible(): boolean {
  try {
    return localStorage.getItem(VISIBLE_KEY) === 'true';
  } catch {
    return false;
  }
}

class LibraryBrowserState {
  #isVisible = $state(loadVisible());

  /** カラムブラウザで選んでいる項目（変更は、オブジェクトごと代入する） */
  selection = $state.raw<BrowserSelection>(EMPTY_SELECTION);

  /** フィルタ（評価・年・お気に入り。変更は、オブジェクトごと代入する） */
  filters = $state.raw<TrackFilters>(EMPTY_FILTERS);

  /** カラムブラウザを出すか（localStorageに保存） */
  get isVisible(): boolean {
    return this.#isVisible;
  }

  set isVisible(value: boolean) {
    this.#isVisible = value;
    // 隠している間の選択は見えないため、隠す時に解除する
    if (!value) this.selection = EMPTY_SELECTION;
    try {
      localStorage.setItem(VISIBLE_KEY, String(value));
    } catch {
      // 保存できなくても動作には影響しない
    }
  }

  /** カラムブラウザの選択か、フィルタで絞り込んでいるか */
  get isActive(): boolean {
    return hasBrowserSelection(this.selection) || countActiveFilters(this.filters) > 0;
  }

  /** 選択とフィルタを、すべて解除する */
  clear(): void {
    this.selection = EMPTY_SELECTION;
    this.filters = EMPTY_FILTERS;
  }
}

export const libraryBrowser = new LibraryBrowserState();
