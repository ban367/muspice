/**
 * UI状態
 *
 * `ui`の各プロパティを直接読み書きする（例: `ui.isSidebarOpen = false`）。
 * `$state`のため、コンポーネント・`$derived`・`$effect`からの読み取りは変更に追随する。
 * localStorageに保存する状態はsetterで保存するため、オブジェクトの中身を書き換えず代入する。
 */

import { TRACK_COLUMN_IDS, TRACK_COLUMNS, type TrackColumnId } from '#lib/utils/trackColumns.js';

// 右サイドバーのパネル（queue: 再生キュー, equalizer: イコライザ）
export type RightSidebarPanel = 'queue' | 'equalizer';

// グリッドカードサイズの範囲（px）
export const MIN_CARD_SIZE = 50;
export const MAX_CARD_SIZE = 200;

/** 幅を持つ列（番号の列と、曲の一覧に出せる列） */
type WidthColumn = 'number' | TrackColumnId;

/** 曲の一覧の列の幅（px。どの画面の一覧でも共通） */
export type ColumnWidths = Record<WidthColumn, number>;

const WIDTH_COLUMNS: readonly WidthColumn[] = ['number', ...TRACK_COLUMN_IDS];

/** 番号の列（行の番号・再生中の印）の幅 */
const NUMBER_COLUMN_WIDTH = 48;

/** 列の幅の既定値 */
function defaultColumnWidth(column: WidthColumn): number {
  return column === 'number' ? NUMBER_COLUMN_WIDTH : TRACK_COLUMNS[column].width;
}

const RIGHT_SIDEBAR_PINNED_KEY = 'muspice:rightSidebarPinned';
const COLUMN_WIDTHS_KEY = 'muspice:columnWidths';
const PLAYLIST_EXPORT_RELATIVE_KEY = 'muspice:playlistExportRelative';

/** localStorageのJSONを読む（ない・読めない・壊れている場合はnull） */
function loadJson(key: string): unknown {
  try {
    const stored = localStorage.getItem(key);
    return stored === null ? null : JSON.parse(stored);
  } catch {
    return null;
  }
}

/** localStorageにJSONで保存する（保存できなくても動作には影響しない） */
function saveJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 保存できなくても動作には影響しない
  }
}

function loadRightSidebarPinned(): boolean {
  return loadJson(RIGHT_SIDEBAR_PINNED_KEY) === true;
}

/** 保存済みの列幅を読み込む（古いキーは捨て、ない列・不正な値は既定値で補う） */
function loadColumnWidths(): ColumnWidths {
  const stored = loadJson(COLUMN_WIDTHS_KEY);
  const parsed =
    typeof stored === 'object' && stored !== null ? (stored as Record<string, unknown>) : null;

  const widths = Object.fromEntries(
    WIDTH_COLUMNS.map((column) => {
      const value = parsed?.[column];
      const isValid = typeof value === 'number' && Number.isFinite(value) && value > 0;
      return [column, isValid ? value : defaultColumnWidth(column)];
    })
  ) as ColumnWidths;
  // 古い不要なキー（checkbox・status）を除いた形で保存し直す
  if (parsed !== null) saveJson(COLUMN_WIDTHS_KEY, widths);
  return widths;
}

class UiState {
  /** 左サイドバーの開閉 */
  isSidebarOpen = $state(true);

  /** 右サイドバーの展開 */
  isRightSidebarExpanded = $state(false);

  /** 右サイドバーで表示中のパネル */
  activeRightSidebarPanel = $state<RightSidebarPanel>('queue');

  /** グリッドカードのサイズ（MIN_CARD_SIZE〜MAX_CARD_SIZE） */
  gridCardSize = $state(120);

  /** ブラウズ画面（アルバム/アーティスト/ジャンル）の名前検索 */
  browseSearchQuery = $state('');

  /** インポートダイアログの開閉 */
  isImportDialogOpen = $state(false);

  /** Aboutダイアログの開閉 */
  isAboutDialogOpen = $state(false);

  /**
   * 一覧の中の位置へ移動させたい曲（再生中の曲へのジャンプ。なければnull）
   *
   * その曲を含む曲の一覧（`TrackList`）が、その行までスクロールして選び、nullに戻す。
   */
  revealTrackId = $state<string | null>(null);

  #isRightSidebarPinned = $state(loadRightSidebarPinned());
  #columnWidths = $state.raw<ColumnWidths>(loadColumnWidths());
  #playlistExportRelative = $state(loadJson(PLAYLIST_EXPORT_RELATIVE_KEY) === true);

  /** プレイリストの書き出しで、曲の場所を相対パスで書くか（前回の選択。localStorageに保存） */
  get playlistExportRelative(): boolean {
    return this.#playlistExportRelative;
  }

  set playlistExportRelative(value: boolean) {
    this.#playlistExportRelative = value;
    saveJson(PLAYLIST_EXPORT_RELATIVE_KEY, value);
  }

  /** 右サイドバーの固定（固定時は明示的に閉じるまで表示され続ける。localStorageに保存） */
  get isRightSidebarPinned(): boolean {
    return this.#isRightSidebarPinned;
  }

  set isRightSidebarPinned(value: boolean) {
    this.#isRightSidebarPinned = value;
    saveJson(RIGHT_SIDEBAR_PINNED_KEY, value);
    // 固定した時は展開する
    if (value) {
      this.isRightSidebarExpanded = true;
    }
  }

  /** トラック一覧の列幅（localStorageに保存。変更は列を書き換えず、オブジェクトごと代入する） */
  get columnWidths(): ColumnWidths {
    return this.#columnWidths;
  }

  set columnWidths(value: ColumnWidths) {
    this.#columnWidths = value;
    saveJson(COLUMN_WIDTHS_KEY, value);
  }
}

export const ui = new UiState();

// ---------- 前回開いていた画面（起動時の復元に使う） ----------

const LAST_PAGE_KEY = 'muspice:lastPage';

/** 起動時に復元してよい画面（メインウィンドウのライブラリ・プレイリスト） */
function isRestorablePage(path: string): boolean {
  return path.startsWith('/library/') || path === '/playlists' || path.startsWith('/playlists/');
}

/**
 * 前回開いていた画面のパスを返す（記録がない・復元できない画面の場合はnull）
 */
export function getLastPage(): string | null {
  try {
    const path = localStorage.getItem(LAST_PAGE_KEY);
    return path !== null && isRestorablePage(path) ? path : null;
  } catch {
    return null;
  }
}

/**
 * 開いている画面のパスを記録する（復元できない画面は記録しない）
 */
export function saveLastPage(path: string): void {
  if (!isRestorablePage(path)) return;
  try {
    localStorage.setItem(LAST_PAGE_KEY, path);
  } catch {
    // 保存できなくても動作には影響しない
  }
}
