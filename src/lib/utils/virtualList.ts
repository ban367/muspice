/**
 * 仮想スクロールの計算
 *
 * 長い一覧で、見えている行（とその前後の少しの行）だけを描画するための範囲を求める。
 * 行の高さはすべて同じとして扱う。グリッド表示では、1行に`columns`個の項目が並ぶ。
 * 描画は`#lib/components/ui/VirtualList.svelte`が行う。
 */

export interface VirtualRangeInput {
  /** 項目の数 */
  count: number;
  /** 1行に並ぶ項目の数（リスト表示は1） */
  columns: number;
  /** 1行の高さ（行の間隔を含む。px） */
  rowStride: number;
  /** 一覧の先頭からのスクロール位置（px） */
  scrollTop: number;
  /** 一覧が見えている範囲の高さ（px） */
  viewportHeight: number;
  /** 見えている範囲の前後に、余分に描画する行の数 */
  overscan: number;
}

export interface VirtualRange {
  /** 描画する最初の項目の位置 */
  start: number;
  /** 描画する最後の項目の次の位置 */
  end: number;
  /** 描画する最初の行の、一覧の先頭からの位置（px） */
  offsetTop: number;
}

/** 行の数（最後の行は途中までのことがある） */
export function countRows(count: number, columns: number): number {
  return Math.ceil(Math.max(0, count) / Math.max(1, columns));
}

/**
 * 描画する項目の範囲を求める
 *
 * 見えている行に、前後`overscan`行を加えた範囲を返す。
 */
export function computeVirtualRange(input: VirtualRangeInput): VirtualRange {
  const columns = Math.max(1, input.columns);
  const rows = countRows(input.count, columns);
  if (rows === 0 || input.rowStride <= 0) {
    return { start: 0, end: 0, offsetTop: 0 };
  }

  const scrollTop = Math.max(0, input.scrollTop);
  // 末尾を越えたスクロール位置（一覧が短くなった直後など）でも、最後の行は描画する
  const firstVisible = Math.min(rows - 1, Math.floor(scrollTop / input.rowStride));
  const lastVisible = Math.min(
    rows - 1,
    Math.floor((scrollTop + Math.max(0, input.viewportHeight)) / input.rowStride)
  );

  const firstRow = Math.max(0, firstVisible - input.overscan);
  const lastRow = Math.min(rows - 1, Math.max(firstVisible, lastVisible) + input.overscan);

  return {
    start: firstRow * columns,
    end: Math.min(input.count, (lastRow + 1) * columns),
    offsetTop: firstRow * input.rowStride
  };
}

export interface RevealInput {
  /** 見える位置へ出す項目の位置 */
  index: number;
  columns: number;
  rowStride: number;
  /** 行の間隔（px。行の高さは`rowStride - gap`） */
  gap: number;
  scrollTop: number;
  viewportHeight: number;
}

/**
 * 項目が見える位置になるスクロール位置を求める
 *
 * すでに全体が見えている場合は、今の位置を返す。上に隠れている場合は上端に、
 * 下に隠れている場合は下端に、その項目の行がそろう位置を返す。
 */
export function scrollTopToReveal(input: RevealInput): number {
  const row = Math.floor(Math.max(0, input.index) / Math.max(1, input.columns));
  const top = row * input.rowStride;
  const bottom = top + input.rowStride - input.gap;

  if (top < input.scrollTop) return top;
  if (bottom > input.scrollTop + input.viewportHeight) {
    return Math.max(0, bottom - input.viewportHeight);
  }
  return input.scrollTop;
}

/**
 * グリッド表示で、幅に入る列の数を求める
 *
 * CSSの`repeat(auto-fill, minmax(minColumnWidth, 1fr))`と同じ数になる。
 * @param width - 一覧の幅（px）
 * @param minColumnWidth - 1列の最小の幅（px）
 * @param gap - 列の間隔（px）
 */
export function countColumns(width: number, minColumnWidth: number, gap: number): number {
  if (minColumnWidth <= 0) return 1;
  return Math.max(1, Math.floor((width + gap) / (minColumnWidth + gap)));
}
