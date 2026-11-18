<!--
  @component GridDetailSplit
  グリッド表示の下に、選んだ項目（アルバム・アーティスト）の中身を出す領域を置く（ADR-097）。
  中身（`detail`）を渡した時だけ、境目と領域を出す（渡さなければ、グリッドが全体を使う）。

  - 領域の高さは、境目をドラッグして変える（キーボードでは、境目にフォーカスして上下の矢印キー。Shiftで大きく）。
    高さは`ui.gridDetailHeight`に保存し、画面に入らない時は、グリッドに最低限の高さを残して縮めて出す
  - 境目のダブルクリックで、既定の高さに戻す
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { DEFAULT_GRID_DETAIL_HEIGHT, MIN_GRID_DETAIL_HEIGHT, ui } from '#lib/stores/ui.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** グリッド（残りの高さを使う） */
    grid: Snippet;
    /** 下に出す中身（ない時は、領域を出さない） */
    detail?: Snippet;
  }

  let { grid, detail }: Props = $props();

  /** グリッドに残す最低限の高さ（px） */
  const MIN_GRID_HEIGHT = 120;
  /** 矢印キー1回で変える高さ（px。Shiftを押していれば4倍） */
  const KEY_STEP = 16;

  /** 全体の高さ（領域の高さの上限を決める） */
  let containerHeight = $state(0);
  /** ドラッグ中の高さ（ドラッグの間は保存せず、離した時に保存する） */
  let dragHeight = $state<number | null>(null);
  /** ドラッグを始めた位置と、その時の高さ */
  let dragStart: { y: number; height: number } | null = null;

  /** 領域の高さの上限（全体が測れるまでは、制限しない） */
  const maxHeight = $derived(
    containerHeight > 0
      ? Math.max(MIN_GRID_DETAIL_HEIGHT, containerHeight - MIN_GRID_HEIGHT)
      : Number.POSITIVE_INFINITY
  );

  function clampHeight(height: number): number {
    return Math.round(Math.min(Math.max(height, MIN_GRID_DETAIL_HEIGHT), maxHeight));
  }

  /** 出す高さ（保存した高さを、今の画面に収める） */
  const height = $derived(clampHeight(dragHeight ?? ui.gridDetailHeight));

  function handlePointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragStart = { y: event.clientY, height };
    dragHeight = height;
  }

  function handlePointerMove(event: PointerEvent) {
    if (dragStart === null) return;
    // 境目を上へ動かすと、領域が高くなる
    dragHeight = clampHeight(dragStart.height + dragStart.y - event.clientY);
  }

  function handlePointerUp() {
    if (dragStart === null) return;
    if (dragHeight !== null) ui.gridDetailHeight = dragHeight;
    dragStart = null;
    dragHeight = null;
  }

  function handleKeydown(event: KeyboardEvent) {
    const step = event.shiftKey ? KEY_STEP * 4 : KEY_STEP;
    let next: number;
    switch (event.key) {
      case 'ArrowUp':
        next = height + step;
        break;
      case 'ArrowDown':
        next = height - step;
        break;
      case 'Home':
        next = maxHeight;
        break;
      case 'End':
        next = MIN_GRID_DETAIL_HEIGHT;
        break;
      default:
        return;
    }
    event.preventDefault();
    ui.gridDetailHeight = clampHeight(next);
  }

  function resetHeight() {
    ui.gridDetailHeight = DEFAULT_GRID_DETAIL_HEIGHT;
  }
</script>

<div class="split" bind:clientHeight={containerHeight}>
  <div class="split-grid">
    {@render grid()}
  </div>
  {#if detail}
    <!-- 境目（ドラッグ・キーボードで、下の領域の高さを変える） -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="split-handle"
      class:dragging={dragHeight !== null}
      role="separator"
      aria-orientation="horizontal"
      aria-label={m.library.detailResize}
      aria-valuenow={height}
      aria-valuemin={MIN_GRID_DETAIL_HEIGHT}
      aria-valuemax={Number.isFinite(maxHeight) ? maxHeight : undefined}
      tabindex="0"
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={handlePointerUp}
      onpointercancel={handlePointerUp}
      onkeydown={handleKeydown}
      ondblclick={resetHeight}
    ></div>
    <div class="split-detail" style="height: {height}px;">
      {@render detail()}
    </div>
  {/if}
</div>

<style>
  @reference "../../../app.css";

  .split {
    @apply flex flex-col h-full min-h-0;
  }

  .split-grid {
    @apply flex-1 min-h-0;
  }

  /* 見た目は1pxの線で、つかめる範囲は上下に広げる */
  .split-handle {
    @apply relative h-px shrink-0 bg-border outline-none;
    cursor: row-resize;
    touch-action: none;
  }

  .split-handle::before {
    content: '';
    @apply absolute inset-x-0 -top-1 -bottom-1 z-10;
  }

  .split-handle:hover,
  .split-handle.dragging,
  .split-handle:focus-visible {
    @apply bg-primary;
    box-shadow: 0 0 0 1px var(--color-primary);
  }

  .split-detail {
    @apply shrink-0 min-h-0 overflow-hidden bg-base-100;
  }
</style>
