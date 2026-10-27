/**
 * UI状態
 *
 * `ui`の各プロパティを直接読み書きする（例: `ui.isSidebarOpen = false`）。
 * `$state`のため、コンポーネント・`$derived`・`$effect`からの読み取りは変更に追随する。
 * localStorageに保存する状態はsetterで保存するため、オブジェクトの中身を書き換えず代入する。
 */

import { untrack } from 'svelte';
import type { AlbumSummary, ArtistSummary, Track } from '#lib/types/models.js';
import { groupArtist } from '#lib/utils/albumKey.js';
import { groupArtistSortName } from '#lib/utils/nameSort.js';
import { TRACK_COLUMN_IDS, TRACK_COLUMNS, type TrackColumnId } from '#lib/utils/trackColumns.js';

// 右サイドバーのパネル（queue: 再生キュー, equalizer: イコライザ, info: 選んだアルバム・アーティストの曲）
export type RightSidebarPanel = 'queue' | 'equalizer' | 'info';

/**
 * 情報パネルに表示する項目
 *
 * - album: グリッド・アルバムの見出しで選んだアルバム、または曲の一覧で選んだ曲のアルバム（`trackId`は
 *   その曲。パネルの一覧の中で、その曲の行を選んで示す）
 * - artist: グリッドで選んだアーティスト
 * - track: 曲の一覧・最近追加した曲で選んだ、アルバムのない曲
 */
export type InfoTarget =
  | { type: 'album'; album: AlbumSummary; trackId?: string }
  | { type: 'artist'; artist: ArtistSummary }
  | { type: 'track'; track: Track };

/**
 * 曲の一覧で選んだ曲から、情報パネルに表示する項目を作る（曲のアルバム。アルバムがなければ曲そのもの）
 *
 * アルバムは「アルバムアーティスト（なければ曲のアーティスト）＋アルバム名」でまとめる（Rustの
 * `find_album_summaries`と同じ）。曲数・合計の時間は、パネルが曲を取得するまでわからないため0にする
 * （パネルは、曲を取得するまで曲数を出さない）。
 */
export function trackInfoTarget(track: Track): InfoTarget {
  if (track.album === null) return { type: 'track', track };
  return {
    type: 'album',
    album: {
      name: track.album,
      sortName: track.sortTags.album,
      artist: groupArtist(track),
      artistSortName: groupArtistSortName(track),
      trackCount: 0,
      totalDuration: 0,
      representativeTrackId: track.id
    },
    trackId: track.id
  };
}

/**
 * 情報パネルの追随を続けて呼んだ時に、まとめる間隔（ms）
 *
 * 矢印キーを押し続けると、選択が毎秒十数回以上動く。そのたびに曲・ジャケット画像を取得しないよう、
 * この間隔より短く続いた呼び出しは、最後の1回だけを反映する。
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export const FOLLOW_INFO_INTERVAL = 200;

// グリッドカードサイズの範囲（px）
export const MIN_CARD_SIZE = 50;
export const MAX_CARD_SIZE = 200;

/** グリッドカードの枠の内側の余白（px。`app.css`の`.grid-card`の余白と合わせる） */
const GRID_CARD_PADDING = 6;

/** グリッドカードの間隔（px） */
export const GRID_CARD_GAP = 8;

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

  /**
   * 情報パネルに表示する項目（まだ選んでいなければnull）
   *
   * 一覧の項目をそのまま持つ（Proxyで包まない）。曲は、パネルが項目の名前で取得する。
   */
  infoTarget = $state.raw<InfoTarget | null>(null);

  /** 情報パネルを開いているか */
  get isInfoPanelOpen(): boolean {
    return this.isRightSidebarExpanded && this.activeRightSidebarPanel === 'info';
  }

  /**
   * 右サイドバーのパネルを、本文に重ねずに横へ並べるか（並べる間は、本文をパネルの幅だけ寄せる）
   *
   * 固定した時と、情報パネルを開いている時。情報パネルは一覧を見ながら項目を選び直すため、
   * 重ねると（パネルの外を押すと閉じるため）別の項目を選べない。
   */
  get isRightSidebarDocked(): boolean {
    return this.isRightSidebarExpanded && (this.isRightSidebarPinned || this.isInfoPanelOpen);
  }

  /** 追随をまとめている間のタイマー（まとめていなければnull） */
  #followTimer: ReturnType<typeof setTimeout> | null = null;
  /** まとめている間に、最後に追随させた項目（なければnull） */
  #pendingFollow: InfoTarget | null = null;

  /**
   * 情報パネルを開き、項目を表示する（グリッドのカード・アルバムの見出し・曲の行を押した時）
   */
  showInfo(target: InfoTarget): void {
    // まとめている追随は捨てる（押した項目を、後から古い追随で上書きしない）
    this.#cancelFollow();
    this.infoTarget = target;
    this.activeRightSidebarPanel = 'info';
    this.isRightSidebarExpanded = true;
  }

  /**
   * 情報パネルを開いている時だけ、表示する項目を入れ替える（グリッド・曲の一覧のキーボードでの移動などに
   * 追随させる。閉じている時は開かない）
   *
   * 間を空けた呼び出しはすぐに反映する。`FOLLOW_INFO_INTERVAL`より短い間隔で続いた呼び出しは、
   * 続いている間は反映せず、止まった後に最後の項目だけを反映する（矢印キーを押し続けた時）。
   */
  followInfo(target: InfoTarget): void {
    if (!this.isInfoPanelOpen) return;
    if (this.#followTimer === null) {
      this.infoTarget = target;
    } else {
      clearTimeout(this.#followTimer);
      this.#pendingFollow = target;
    }
    this.#followTimer = setTimeout(() => {
      const pending = this.#pendingFollow;
      this.#followTimer = null;
      this.#pendingFollow = null;
      // 待っている間にパネルを閉じた・別のパネルに切り替えた時は、反映しない
      if (pending !== null && this.isInfoPanelOpen) this.infoTarget = pending;
    }, FOLLOW_INFO_INTERVAL);
  }

  #cancelFollow(): void {
    if (this.#followTimer !== null) clearTimeout(this.#followTimer);
    this.#followTimer = null;
    this.#pendingFollow = null;
  }

  /** グリッドカードのサイズ（ジャケット画像の1辺。MIN_CARD_SIZE〜MAX_CARD_SIZE） */
  gridCardSize = $state(120);

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

/**
 * 一覧で最後に選んだ曲を、情報パネルへ反映する（コンポーネントの初期化の中で呼ぶ）
 *
 * `track`は、最後に選んだ曲（`TrackSelection.active`の曲。まだ選んでいなければnull）を返す関数。
 * 返す関数は、行をクリックして選んだ時に呼ぶ（パネルを閉じていても開き、その曲を表示する。グリッドの
 * カードのクリックと同じ）。キーボードでの移動など、ほかの操作で選んだ曲が変わった時と、パネルを開いた時は、
 * 開いている間だけ`followInfo`で反映する（まだ選んでいない時は、表示している項目をそのまま残す）。選択を
 * 解除した後（タグの編集を保存した後など）も、その曲のタグの変更に追随させる。曲を選べるどの一覧でも
 * 同じ動きにする（ADR-066・ADR-076）。
 */
export function followTrackInfo(track: () => Track | null): () => void {
  // 曲が同じ（一覧を取り直しただけ）なら、反映し直さない
  const selected = $derived.by(track);
  // パネルへ最後に反映した曲（クリックで表示した曲を、追随でもう一度反映しないため。パネルを閉じたら
  // 忘れ、開き直した時は反映し直す）。リアクティブにしない
  let shown: Track | null = null;
  $effect(() => {
    if (!ui.isInfoPanelOpen) {
      shown = null;
      return;
    }
    if (selected === null || selected === shown) return;
    shown = selected;
    const target = trackInfoTarget(selected);
    untrack(() => ui.followInfo(target));
  });
  return () => {
    if (selected === null) return;
    shown = selected;
    ui.showInfo(trackInfoTarget(selected));
  };
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
