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
 * グリッドに並んだ要素の列数を数える（1行目にある要素の数）
 * @param items - 表示順の要素
 */
export function countGridColumns(items: ArrayLike<HTMLElement>): number {
  if (items.length === 0) return 1;
  const firstRowTop = items[0].offsetTop;
  let columns = 0;
  while (columns < items.length && items[columns].offsetTop === firstRowTop) {
    columns++;
  }
  return Math.max(1, columns);
}

/**
 * 1つだけ選択する一覧（2ペイン表示の左のアルバム・アーティストの一覧など）のキーボード操作
 *
 * 一覧の要素の`onkeydown`から呼ぶ。項目の要素は、一覧の要素の子として表示順に並べておく。
 * 移動先の項目へフォーカスも移す（見える位置へのスクロールを兼ねる。フォーカスを前の項目に
 * 残すと、EnterやSpaceが前の項目に効いてしまう）。
 * @param event - キーイベント
 * @param current - 選択中の項目の位置（選択がない場合は-1）
 * @returns 移動先の位置（呼び出し側でその項目を選択する）。扱わないキーの場合はnull
 */
export function moveListSelection(event: KeyboardEvent, current: number): number | null {
  if (event.defaultPrevented) return null;
  if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null;

  const list = event.currentTarget;
  if (!(list instanceof HTMLElement)) return null;

  const target = navigationTarget(event.key, current, list.children.length);
  if (target === null) return null;

  event.preventDefault();
  const item = list.children[target];
  if (item instanceof HTMLElement) {
    item.focus();
  }
  return target;
}
