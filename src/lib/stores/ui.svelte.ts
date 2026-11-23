/**
 * UI状態
 *
 * `ui`の各プロパティを直接読み書きする（例: `ui.isSidebarOpen = false`）。
 * `$state`のため、コンポーネント・`$derived`・`$effect`からの読み取りは変更に追随する。
 * localStorageに保存する状態はsetterで保存するため、オブジェクトの中身を書き換えず代入する。
 */

import { untrack } from 'svelte';
import type { Track } from '#lib/types/models.js';
import type { RevealGroup } from '#lib/utils/libraryReveal.js';
import { TRACK_COLUMN_IDS, TRACK_COLUMNS, type TrackColumnId } from '#lib/utils/trackColumns.js';

// 右サイドバーのパネル（queue: 再生キュー, sound: イコライザなどの音の加工の設定,
// album: 再生中の曲とそのアルバム, track: 一覧で選んだ曲の情報（選択中の曲））
export type RightSidebarPanel = 'queue' | 'sound' | 'album' | 'track';

/** 一覧で選んだ曲（選択中の曲のパネルに出す） */
export interface SelectedTrack {
  /** 選んでいる曲のうち、最後に選んだ曲（選択を外した後は、最後に選んでいた曲） */
  track: Track;
  /** 選んでいる曲の数（選択を外した・一覧がなくなった後は0） */
  count: number;
  /**
   * 選んでいる曲（一覧の順。メニューバーの「曲」メニューが使う。ADR-109）
   *
   * 選択を外した・一覧がなくなった後（`count`が0）は持たない。
   */
  tracks?: readonly Track[];
  /**
   * 一覧のすべての曲（表示の順。1曲の情報の編集画面で、前後の曲へ移るのに使う。ADR-119）
   *
   * 選択を外した・一覧がなくなった後（`count`が0）は持たない。
   */
  listTracks?: readonly Track[];
}

// グリッドカードサイズの範囲と既定値（px。既定値はスライダーの中央にする）
export const MIN_CARD_SIZE = 50;
export const MAX_CARD_SIZE = 150;
const DEFAULT_CARD_SIZE = 100;

/** グリッドカードの枠の内側の余白（px。`app.css`の`.grid-card`の余白と合わせる） */
const GRID_CARD_PADDING = 6;

/** グリッドカードの間隔（px） */
export const GRID_CARD_GAP = 8;

/** グリッドの下に出す、選んだ項目の中身の領域の高さ（px。既定値と最小値。ADR-097） */
export const DEFAULT_GRID_DETAIL_HEIGHT = 320;
export const MIN_GRID_DETAIL_HEIGHT = 120;

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
const AUTO_SHOW_SELECTED_TRACK_KEY = 'muspice:autoShowSelectedTrack';
const GRID_DETAIL_HEIGHT_KEY = 'muspice:gridDetailHeight';
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

/** 右サイドバーの固定（既定は固定。解除した時だけfalseを保存してある） */
function loadRightSidebarPinned(): boolean {
  return loadJson(RIGHT_SIDEBAR_PINNED_KEY) !== false;
}

/** 保存済みの、グリッドの下の中身の領域の高さ（ない・不正な値は既定値） */
function loadGridDetailHeight(): number {
  const stored = loadJson(GRID_DETAIL_HEIGHT_KEY);
  return typeof stored === 'number' && Number.isFinite(stored) && stored >= MIN_GRID_DETAIL_HEIGHT
    ? stored
    : DEFAULT_GRID_DETAIL_HEIGHT;
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

  /** 右サイドバーのパネルを、本文に重ねずに横へ並べるか（固定した時。並べる間は、本文をパネルの幅だけ寄せる） */
  get isRightSidebarDocked(): boolean {
    return this.isRightSidebarExpanded && this.isRightSidebarPinned;
  }

  /** グリッドカードのサイズ（ジャケット画像の1辺。MIN_CARD_SIZE〜MAX_CARD_SIZE） */
  gridCardSize = $state(DEFAULT_CARD_SIZE);

  /** グリッドカードの幅（px。ジャケット画像に、枠の内側の余白を足した幅） */
  get gridCardWidth(): number {
    return this.gridCardSize + GRID_CARD_PADDING * 2;
  }

  /** ブラウズ画面（アルバム/アーティスト/ジャンル）の名前検索 */
  browseSearchQuery = $state('');

  /** インポートダイアログの開閉 */
  isImportDialogOpen = $state(false);

  /** Aboutダイアログの開閉 */
  isAboutDialogOpen = $state(false);

  /** Now Playingの画面（再生中の曲を大きく表示し、歌詞を出す）を開いているか */
  isNowPlayingOpen = $state(false);

  /**
   * 曲の情報の編集画面（別のウィンドウ）を開いているか（開いている間は、画面に覆いを重ねて操作させない。
   * Rustが送る`TrackEditorChanged`で書き換える。ADR-094）
   */
  isTrackEditorOpen = $state(false);

  /**
   * 一覧の中の位置へ移動させたい曲（再生中の曲へのジャンプ。なければnull）
   *
   * その曲を含む曲の一覧（`TrackList`）が、その行までスクロールして選び、nullに戻す。
   */
  revealTrackId = $state<string | null>(null);

  /**
   * 曲から移った先の画面で選ぶアルバム・アーティスト（「アルバムへ移動」など。なければnull。ADR-110）
   *
   * アルバム・アーティストの画面（グリッド表示は`LibraryGrid`、リスト表示は`LibraryBrowsePage`）が、
   * その項目を選んで見える位置へ出し、nullに戻す。
   */
  revealGroup = $state.raw<RevealGroup | null>(null);

  /**
   * 一覧で最後に選んだ曲（選択中の曲のパネルに出す。まだ選んでいなければnull）
   *
   * 曲を選べる一覧が、`followSelectedTrack`で書き換える。選択を外した後も、最後に選んでいた曲を残す。
   */
  selectedTrack = $state.raw<SelectedTrack | null>(null);

  #isRightSidebarPinned = $state(loadRightSidebarPinned());
  #autoShowSelectedTrack = $state(loadJson(AUTO_SHOW_SELECTED_TRACK_KEY) === true);
  #gridDetailHeight = $state(loadGridDetailHeight());
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

  /**
   * メインビューで曲を選んだら、右サイドバーを選択中の曲のパネルに切り替えるか（既定はオフ。
   * localStorageに保存）
   */
  get autoShowSelectedTrack(): boolean {
    return this.#autoShowSelectedTrack;
  }

  set autoShowSelectedTrack(value: boolean) {
    this.#autoShowSelectedTrack = value;
    saveJson(AUTO_SHOW_SELECTED_TRACK_KEY, value);
  }

  /**
   * グリッドの下に出す、選んだ項目の中身の領域の高さ（px。境目のドラッグで変える。localStorageに保存）
   *
   * 画面に入らない高さの時は、表示する側が収まる高さに縮めて出す（保存した値は変えない）。
   */
  get gridDetailHeight(): number {
    return this.#gridDetailHeight;
  }

  set gridDetailHeight(value: number) {
    const height = Math.max(MIN_GRID_DETAIL_HEIGHT, Math.round(value));
    this.#gridDetailHeight = height;
    saveJson(GRID_DETAIL_HEIGHT_KEY, height);
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

/** `ui.selectedTrack`を出している一覧（`followSelectedTrack`ごとの印。リアクティブにしない） */
let selectedTrackOwner: symbol | null = null;

/**
 * 一覧で選んだ曲を、選択中の曲のパネルに出す（曲を選べる一覧のコンポーネントの初期化で呼ぶ。ADR-078・ADR-096）
 *
 * 最後に選択を変えた一覧の選択を出す。その一覧の選択が空になった時と、一覧がなくなった時（別の画面へ
 * 移った時）は、最後に選んでいた曲を残し、選んでいる数を0にする。出している一覧の曲を取り直した時
 * （タグの編集・評価の変更など）は、新しい内容に入れ替える。ほかの一覧が出している間は、曲を取り直した
 * だけ（選択は同じ）では上書きしない。
 *
 * パネルを閉じている間も書き換える（開いた時に、選んだ曲がすぐ出るように）。選択を変えた時、
 * `ui.autoShowSelectedTrack`がオンで右サイドバーを開いていれば、選択中の曲のパネルに切り替える
 * （閉じていれば開かない）。
 *
 * @param selected - 選んでいる曲のうち最後に選んだ曲と、選んでいる曲の数を返す関数（選んでいなければnull）
 */
export function followSelectedTrack(selected: () => SelectedTrack | null): void {
  const owner = Symbol('selectedTrack');
  const current = $derived.by(selected);
  /** 前に見た選択（曲と数。曲を取り直しただけか、選択を変えたかを見分ける） */
  let lastKey: string | null = null;

  $effect(() => {
    const value = current;
    const key = value === null ? null : `${value.track.id}:${value.count}`;
    const isChanged = key !== lastKey;
    lastKey = key;
    untrack(() => {
      if (value === null) {
        if (selectedTrackOwner === owner) releaseSelectedTrack();
      } else if (isChanged || selectedTrackOwner === owner) {
        selectedTrackOwner = owner;
        ui.selectedTrack = value;
        if (isChanged) showSelectedTrackPanel();
      }
    });
  });

  // 一覧がなくなったら、出していた曲を残したまま手放す
  $effect(() => () => {
    if (selectedTrackOwner === owner) releaseSelectedTrack();
  });
}

/** 出していた曲を残したまま、一覧から手放す（選んでいる数を0にする） */
function releaseSelectedTrack(): void {
  selectedTrackOwner = null;
  if (ui.selectedTrack !== null && ui.selectedTrack.count !== 0) {
    ui.selectedTrack = { track: ui.selectedTrack.track, count: 0 };
  }
}

/** 選択を変えた時に、右サイドバーを選択中の曲のパネルに切り替える（設定がオンで、開いている時だけ） */
function showSelectedTrackPanel(): void {
  if (ui.autoShowSelectedTrack && ui.isRightSidebarExpanded) {
    ui.activeRightSidebarPanel = 'track';
  }
}

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
