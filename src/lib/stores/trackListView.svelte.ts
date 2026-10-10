/**
 * 曲の一覧の、画面ごとの設定（表示する列・列の順番・並び順）
 *
 * 全曲・お気に入り・よく再生する曲など、画面ごとに覚える（localStorageに保存する）。
 * 列の幅は、どの画面でも共通のため、ここではなく`ui.columnWidths`に持つ。
 */
import {
  DEFAULT_TRACK_COLUMNS,
  TRACK_COLUMN_IDS,
  normalizeColumns,
  type TrackColumnId
} from '#lib/utils/trackColumns.js';
import type { TrackSort } from '#lib/utils/trackSort.js';

/** 画面の既定の設定（保存していない間と、「既定に戻す」で使う） */
export interface TrackListViewDefaults {
  /** 表示する列（省略時は`DEFAULT_TRACK_COLUMNS`） */
  columns?: readonly TrackColumnId[];
  /** 並び順（nullは、渡された順のまま。省略時は、追加した日時の新しい順） */
  sort?: TrackSort | null;
}

const KEY_PREFIX = 'muspice:trackListView:';

/** 省略時の並び順（追加した日時の新しい順） */
const DEFAULT_SORT: TrackSort = { field: 'createdAt', direction: 'desc' };

/** 保存していた並び順を読む（nullは「渡された順」。読めない値は`undefined`） */
function parseSort(value: unknown): TrackSort | null | undefined {
  if (value === null) return null;
  if (typeof value !== 'object' || value === undefined) return undefined;
  const { field, direction } = value as Record<string, unknown>;
  const isField = (TRACK_COLUMN_IDS as readonly unknown[]).includes(field);
  if (!isField || (direction !== 'asc' && direction !== 'desc')) return undefined;
  return { field: field as TrackColumnId, direction };
}

function sameSort(a: TrackSort | null, b: TrackSort | null): boolean {
  return a?.field === b?.field && a?.direction === b?.direction;
}

/** 1つの画面の、曲の一覧の設定 */
export class TrackListView {
  /** 保存先のキー（保存しない一覧はnull） */
  readonly #key: string | null;
  readonly #defaultColumns: readonly TrackColumnId[];
  readonly #defaultSort: TrackSort | null;
  #columns = $state.raw<TrackColumnId[]>([]);
  #sort = $state.raw<TrackSort | null>(null);

  /**
   * @param id - 画面を見分ける名前（nullなら保存しない）
   */
  constructor(id: string | null, defaults: TrackListViewDefaults = {}) {
    this.#key = id === null ? null : KEY_PREFIX + id;
    this.#defaultColumns = defaults.columns ?? DEFAULT_TRACK_COLUMNS;
    this.#defaultSort = defaults.sort === undefined ? DEFAULT_SORT : defaults.sort;

    const stored = this.#load();
    this.#columns = normalizeColumns(stored?.columns, this.#defaultColumns);
    const sort = parseSort(stored?.sort);
    this.#sort = sort === undefined ? this.#defaultSort : sort;
  }

  #load(): Record<string, unknown> | null {
    if (this.#key === null) return null;
    try {
      const stored: unknown = JSON.parse(localStorage.getItem(this.#key) ?? 'null');
      return typeof stored === 'object' && stored !== null
        ? (stored as Record<string, unknown>)
        : null;
    } catch {
      return null;
    }
  }

  #save(): void {
    if (this.#key === null) return;
    try {
      localStorage.setItem(this.#key, JSON.stringify({ columns: this.#columns, sort: this.#sort }));
    } catch {
      // 保存できなくても動作には影響しない
    }
  }

  /** 表示する列（左から順。変更は、配列ごと代入する） */
  get columns(): TrackColumnId[] {
    return this.#columns;
  }

  set columns(value: TrackColumnId[]) {
    this.#columns = normalizeColumns(value, this.#defaultColumns);
    this.#save();
  }

  /** 並び順（nullは、渡された順のまま） */
  get sort(): TrackSort | null {
    return this.#sort;
  }

  set sort(value: TrackSort | null) {
    this.#sort = value;
    this.#save();
  }

  /** 列が、既定のままか */
  get isDefaultColumns(): boolean {
    return (
      this.#columns.length === this.#defaultColumns.length &&
      this.#columns.every((column, index) => column === this.#defaultColumns[index])
    );
  }

  /** 列・並び順が、既定のままか */
  get isDefault(): boolean {
    return this.isDefaultSort && this.isDefaultColumns;
  }

  /** 並び順が、既定のままか */
  get isDefaultSort(): boolean {
    return sameSort(this.#sort, this.#defaultSort);
  }

  /** 並び順を、既定に戻す */
  resetSort(): void {
    this.sort = this.#defaultSort;
  }

  /** 列と並び順を、既定に戻す */
  reset(): void {
    this.#columns = [...this.#defaultColumns];
    this.#sort = this.#defaultSort;
    this.#save();
  }
}

// 設定の置き場（名前 → 設定）。一覧そのものは画面に出さないため、変更に追随させる必要はない
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const views = new Map<string, TrackListView>();

/**
 * プレイリストの画面の、並び順を覚える単位の名前
 *
 * プレイリストの画面は、列をどのプレイリストでも共通（`playlist`）にし、並び順だけを
 * プレイリストごとに覚える。
 */
export function playlistSortViewId(playlistId: string): string {
  return `playlist:${playlistId}`;
}

/**
 * 画面の設定を忘れる（保存していた内容も消す。プレイリストを削除した時など）
 */
export function forgetTrackListView(id: string): void {
  views.delete(id);
  try {
    localStorage.removeItem(KEY_PREFIX + id);
  } catch {
    // 消せなくても動作には影響しない
  }
}

/**
 * 画面の、曲の一覧の設定を返す（同じ名前の画面は、同じ設定を共有する）
 *
 * `defaults`は、その名前で最初に呼んだ時のものを使う。
 */
export function trackListView(id: string, defaults: TrackListViewDefaults = {}): TrackListView {
  let view = views.get(id);
  if (!view) {
    view = new TrackListView(id, defaults);
    views.set(id, view);
  }
  return view;
}
