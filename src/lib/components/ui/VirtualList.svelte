<!--
  @component VirtualList
  仮想スクロールの一覧。見えている行（とその前後の少しの行）だけを描画する。

  - このコンポーネントがスクロールする領域になる（親で高さを決めておく）
  - 行の高さはすべて同じとして扱う。高さは描画した行を実測して求める
  - `minColumnWidth`を渡すとグリッド表示になり、幅に入るだけ列を並べる
  - `row`は、項目ごとに要素を1つだけ描画する（行の数を要素の数から数えるため）
  - `header`は、スクロールしても上に固定して表示する（列の見出しなど）
  - そのほかの属性（`role`・`tabindex`・`onkeydown`など）は、項目を並べる要素に渡す
-->
<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import {
    computeVirtualRange,
    countColumns,
    countRows,
    scrollTopToReveal
  } from '#lib/utils/virtualList.js';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'children'> {
    /** 表示順の項目 */
    items: readonly T[];
    /** 項目を識別するキー（並び替え・再取得で要素を使い回すために使う） */
    getKey: (item: T) => unknown;
    /** 項目の描画（項目と、一覧の中の位置を受け取る） */
    row: Snippet<[T, number]>;
    /** 上に固定して表示する見出し */
    header?: Snippet;
    /** 行の高さの見積もり（px）。行を描画した後は実測した高さを使う */
    estimatedRowHeight: number;
    /** グリッド表示での、1列の最小の幅（px）。省略するとリスト表示（1列） */
    minColumnWidth?: number;
    /** 項目の間隔（px） */
    gap?: number;
    /** 見えている範囲の前後に、余分に描画する行の数 */
    overscan?: number;
    /** スクロールする領域に付けるクラス */
    scrollerClass?: string;
  }

  let {
    items,
    getKey,
    row,
    header,
    estimatedRowHeight,
    minColumnWidth,
    gap = 0,
    overscan = 6,
    scrollerClass = '',
    class: listClass = '',
    style: listStyle = '',
    ...listAttributes
  }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let list = $state<HTMLDivElement>();

  let scrollTop = $state(0);
  let scrollerHeight = $state(0);
  let headerHeight = $state(0);
  let listWidth = $state(0);
  // スクロールする領域の上の余白（`scrollerClass`で付けた場合。一覧はその分だけ下から始まる）
  let paddingTop = $state(0);
  // 実測した行の高さ（行の間隔は含まない）。行を描画するまではnull
  let measuredRowHeight = $state<number | null>(null);

  const isGrid = $derived(minColumnWidth !== undefined);
  const columns = $derived(
    minColumnWidth === undefined ? 1 : countColumns(listWidth, minColumnWidth, gap)
  );
  const rowStride = $derived((measuredRowHeight ?? estimatedRowHeight) + gap);
  // 見出しは上に固定されるため、一覧が見えるのはその下の範囲
  const viewportHeight = $derived(Math.max(0, scrollerHeight - headerHeight));

  const range = $derived(
    computeVirtualRange({
      count: items.length,
      columns,
      rowStride,
      scrollTop: scrollTop - paddingTop,
      viewportHeight,
      overscan
    })
  );
  const visibleItems = $derived(items.slice(range.start, range.end));
  // 最後の行の下には間隔がない
  const totalHeight = $derived(Math.max(0, countRows(items.length, columns) * rowStride - gap));

  // 描画した行の高さを測る（カードの大きさの変更などで高さが変わった時も測り直す）
  $effect(() => {
    const element = list;
    if (!element) return;

    const measure = () => {
      const rows = countRows(element.childElementCount, columns);
      if (rows === 0) return;
      const height = (element.getBoundingClientRect().height - gap * (rows - 1)) / rows;
      // 小数の誤差で測り直しを繰り返さないよう、わずかな差は無視する
      if (height > 0 && Math.abs(height - (measuredRowHeight ?? -1)) > 0.05) {
        measuredRowHeight = height;
      }
    };

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (scroller) {
      paddingTop = parseFloat(getComputedStyle(scroller).paddingTop) || 0;
    }
  });

  /**
   * 一覧の中の位置を指定して、その項目を見える位置までスクロールする
   *
   * 呼び出した後の描画（`tick()`の後）で、その項目の要素がある状態になる。
   */
  export function scrollToIndex(index: number): void {
    if (!scroller || index < 0 || index >= items.length) return;
    const current = scroller.scrollTop - paddingTop;
    const target = scrollTopToReveal({
      index,
      columns,
      rowStride,
      gap,
      scrollTop: current,
      viewportHeight
    });
    if (target !== current) {
      // 最初・最後の行では、領域の端までスクロールする（上下の余白も見える位置にする）
      const row = Math.floor(index / columns);
      const lastRow = countRows(items.length, columns) - 1;
      scroller.scrollTop =
        row === 0 ? 0 : row === lastRow ? scroller.scrollHeight : target + paddingTop;
      // scrollイベントを待たずに、描画する範囲を移す
      scrollTop = scroller.scrollTop;
    }
  }

  /** 1行に並んでいる項目の数（リスト表示は1） */
  export function getColumns(): number {
    return columns;
  }
</script>

<div
  class="virtual-scroller {scrollerClass}"
  bind:this={scroller}
  bind:clientHeight={scrollerHeight}
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
>
  {#if header}
    <div class="virtual-header" bind:offsetHeight={headerHeight}>
      {@render header()}
    </div>
  {/if}
  <div class="virtual-sizer" style="height: {totalHeight}px;" bind:clientWidth={listWidth}>
    <div
      {...listAttributes}
      class="virtual-items {listClass}"
      class:is-grid={isGrid}
      style="transform: translateY({range.offsetTop}px); gap: {gap}px; {isGrid
        ? `grid-template-columns: repeat(${columns}, minmax(0, 1fr));`
        : ''} {listStyle}"
      bind:this={list}
    >
      {#each visibleItems as item, offset (getKey(item))}
        {@render row(item, range.start + offset)}
      {/each}
    </div>
  </div>
</div>

<style>
  @reference "../../../app.css";

  .virtual-scroller {
    @apply h-full overflow-y-auto;
  }

  .virtual-header {
    @apply sticky top-0 z-10;
  }

  .virtual-sizer {
    @apply relative;
  }

  /* 見えている範囲の行だけを、スクロール位置に合わせてずらして置く */
  .virtual-items {
    @apply absolute inset-x-0 top-0 flex flex-col;
  }

  .virtual-items.is-grid {
    @apply grid;
  }
</style>
