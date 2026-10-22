/**
 * 一覧のキーボード操作（矢印キーなどでの移動）
 *
 * 矢印キーを押した時に一覧をスクロールさせず、選択している項目から隣の項目へ移すために使う。
 */

/**
 * キーを押した時の移動先の位置を求める
 *
 * - リスト（`columns`がnull）: ↑↓で前後へ。←→は扱わない
 * - グリッド（`columns`が列数）: ←→で前後へ、↑↓で上下の行へ
 * - Home / End: 先頭・末尾へ
 * - 何も選択していない（`current`が範囲外）場合: 進む向きのキーは先頭、戻る向きのキーは末尾へ
 * @param key - 押したキー（`KeyboardEvent.key`）
 * @param current - 現在の位置（選択がない場合は-1）
 * @param count - 項目の数
 * @param columns - グリッドの列数（リストではnull）
 * @returns 移動先の位置（端で動けない場合は現在の位置）。扱わないキー・項目がない場合はnull
 */
export function navigationTarget(
  key: string,
  current: number,
  count: number,
  columns: number | null = null
): number | null {
  if (count <= 0) return null;

  const last = count - 1;
  if (key === 'Home') return 0;
  if (key === 'End') return last;

  const isGrid = columns !== null;
  const rowSize = isGrid ? Math.max(1, columns) : 1;
  let step: number;
  switch (key) {
    case 'ArrowUp':
      step = -rowSize;
      break;
    case 'ArrowDown':
      step = rowSize;
      break;
    case 'ArrowLeft':
      if (!isGrid) return null;
      step = -1;
      break;
    case 'ArrowRight':
      if (!isGrid) return null;
      step = 1;
      break;
    default:
      return null;
  }

  if (current < 0 || current > last) {
    return step > 0 ? 0 : last;
  }

  const target = current + step;
  if (target >= 0 && target <= last) return target;

  // 上下の行がない場合、最初の行・最後の行ではその場にとどまる
  // （最後の行が途中までしかないグリッドで、下の行に項目がない列からは最後の項目へ）
  if (isGrid && Math.abs(step) > 1) {
    if (step < 0) return current;
    return Math.floor(current / rowSize) < Math.floor(last / rowSize) ? last : current;
  }
  return step < 0 ? 0 : last;
}

/**
 * 1つだけ選択する一覧（2ペイン表示の左のアルバム・アーティストの一覧など）のキーボード操作
 *
 * 一覧の要素の`onkeydown`から呼ぶ。修飾キーを押している場合・扱わないキーの場合は何もしない。
 * 移動先を扱う場合は、既定の動作（一覧のスクロール）を止める。
 * @param event - キーイベント
 * @param current - 選択中の項目の位置（選択がない場合は-1）
 * @param count - 項目の数
 * @returns 移動先の位置（呼び出し側でその項目を選択し、見える位置へ出す）。扱わないキーの場合はnull
 */
export function listSelectionTarget(
  event: KeyboardEvent,
  current: number,
  count: number
): number | null {
  if (event.defaultPrevented) return null;
  if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null;

  const target = navigationTarget(event.key, current, count);
  if (target === null) return null;

  event.preventDefault();
  return target;
}

/** 項目を開ける一覧で、キーを押した時に行う操作（`openableListAction`の結果） */
export type OpenableListAction = { type: 'open'; index: number } | { type: 'move'; index: number };

/**
 * 現在位置の項目を開ける一覧（ライブラリのグリッドなど）のキーボード操作
 *
 * 一覧の要素の`onkeydown`から呼ぶ。矢印キー（グリッドでは上下左右、リストでは上下）・Home・Endで
 * 現在位置を移し、Enterで現在位置の項目を開く。修飾キーを押している場合・扱わないキーの場合は
 * 何もしない。項目の中のボタン・リンク（再生ボタンなど）でのEnterは、そのボタンの操作を優先する。
 * 扱うキーでは、既定の動作（一覧のスクロールなど）を止める。
 * @param event - キーイベント
 * @param current - 現在位置（ない場合は-1）
 * @param count - 項目の数
 * @param columns - グリッドの列数（リストではnull）
 * @returns 行う操作（呼び出し側で、その位置の項目を開く・現在位置にして見える位置へ出す）。
 *   扱わないキーの場合はnull
 */
export function openableListAction(
  event: KeyboardEvent,
  current: number,
  count: number,
  columns: number | null
): OpenableListAction | null {
  if (event.defaultPrevented) return null;
  if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null;

  if (event.key === 'Enter') {
    if (event.target instanceof Element && event.target.closest('button, a')) return null;
    if (current < 0 || current >= count) return null;
    event.preventDefault();
    return { type: 'open', index: current };
  }

  const target = navigationTarget(event.key, current, count, columns);
  if (target === null) return null;

  event.preventDefault();
  return { type: 'move', index: target };
}

/** 木の一覧の行（`treeKeyAction`が使う分だけ） */
export interface TreeKeyRow {
  /** 字下げの段（根の項目が0） */
  depth: number;
  hasChildren: boolean;
  isExpanded: boolean;
}

/** 木の一覧で、キーを押した時に行う操作（`treeKeyAction`の結果） */
export type TreeKeyAction =
  | { type: 'select'; index: number }
  | { type: 'expand'; index: number }
  | { type: 'collapse'; index: number };

/**
 * 木の一覧（フォルダ別・年代別の画面の左の一覧）のキーボード操作
 *
 * 一覧の要素の`onkeydown`から呼ぶ（WAI-ARIAの木の一覧の操作に合わせる）。
 * - ↑↓・Home・End: 前後・先頭・末尾の行を選ぶ
 * - →: 閉じている項目は開き、開いている項目は最初の子を選ぶ
 * - ←: 開いている項目は閉じ、閉じている項目（子のない項目を含む）は親を選ぶ
 * - Enter・Space: 現在位置の行を選ぶ（何も選んでいない時に、Tabで入った先頭の行を選ぶため）
 *
 * 修飾キーを押している場合・扱わないキーの場合は何もしない。扱うキーでは、既定の動作（一覧の
 * スクロール）を止める。
 * @param event - キーイベント
 * @param rows - 表示している行（開いている項目の子だけを並べたもの）
 * @param current - 現在位置（選んでいる行。ない場合は-1）
 * @returns 行う操作（呼び出し側で、その行を選ぶ・開く・閉じる）。扱わないキーの場合はnull
 */
export function treeKeyAction(
  event: KeyboardEvent,
  rows: readonly TreeKeyRow[],
  current: number
): TreeKeyAction | null {
  if (event.defaultPrevented) return null;
  if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null;
  if (rows.length === 0) return null;

  const action = treeKeyActionFor(event.key, rows, current);
  if (action !== null) event.preventDefault();
  return action;
}

function treeKeyActionFor(
  key: string,
  rows: readonly TreeKeyRow[],
  current: number
): TreeKeyAction | null {
  const row = current >= 0 && current < rows.length ? rows[current] : null;
  switch (key) {
    case 'Enter':
    case ' ':
      return { type: 'select', index: row ? current : 0 };
    case 'ArrowRight':
      if (!row) return { type: 'select', index: 0 };
      if (!row.hasChildren) return null;
      return row.isExpanded
        ? { type: 'select', index: current + 1 }
        : { type: 'expand', index: current };
    case 'ArrowLeft': {
      if (!row) return { type: 'select', index: 0 };
      if (row.isExpanded) return { type: 'collapse', index: current };
      // 親は、上にある1段浅い行のうち、いちばん近いもの
      for (let index = current - 1; index >= 0; index--) {
        if (rows[index].depth < row.depth) return { type: 'select', index };
      }
      return null;
    }
    default: {
      const target = navigationTarget(key, current, rows.length);
      return target === null ? null : { type: 'select', index: target };
    }
  }
}
