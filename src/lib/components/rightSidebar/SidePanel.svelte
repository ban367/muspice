<!--
  @component SidePanel
  右サイドバーのパネルの枠。どのパネルも、この枠に中身を渡して作る（見出しの高さ・余白・スクロールをそろえるため）。
  - 見出し: パネルの名前と、右端のボタン（`actions`。`SidePanelButton`を並べる）。高さは、ボタンの有無で変えない
  - 上部（`top`）: スクロールせず、常に見える領域（曲のカード`SidePanelTrack`を置く）
  - 本文（`children`）: 残りの高さを使い、はみ出したらスクロールする。中身が自分でスクロールする時（仮想スクロールの
    一覧）は`scroll={false}`にする
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** パネルの名前（右端のアイコンのツールチップと同じ） */
    title: string;
    /** 見出しの右端に並べるボタン */
    actions?: Snippet;
    /** スクロールせずに、見出しの下に出す領域 */
    top?: Snippet;
    /** 本文をこの枠でスクロールするか（falseの時は、中身が残りの高さの中で自分でスクロールする） */
    scroll?: boolean;
    /** 本文のスクロールする要素（パネルが、先頭へ戻す・行を見える位置へ出すのに使う） */
    body?: HTMLElement;
    children: Snippet;
  }

  let { title, actions, top, scroll = true, body = $bindable(), children }: Props = $props();

  const titleId = $props.id();
</script>

<section class="panel" aria-labelledby={titleId}>
  <header class="panel-header">
    <h2 class="panel-title" id={titleId}>{title}</h2>
    {#if actions}
      <div class="panel-actions">{@render actions()}</div>
    {/if}
  </header>

  {#if top}
    <div class="panel-top">{@render top()}</div>
  {/if}

  <div class="panel-body" class:scroll bind:this={body}>
    {@render children()}
  </div>
</section>

<style>
  @reference "../../../app.css";

  .panel {
    @apply flex-1 min-h-0 flex flex-col;
  }

  .panel-header {
    @apply flex items-center justify-between gap-2 h-12 px-4 border-b border-border shrink-0;
  }

  .panel-title {
    @apply m-0 min-w-0 text-sm font-semibold text-text-primary truncate;
  }

  .panel-actions {
    @apply flex items-center gap-1 shrink-0;
  }

  .panel-top {
    @apply border-b border-border shrink-0;
  }

  .panel-body {
    @apply flex-1 min-h-0 flex flex-col overflow-hidden;
  }

  .panel-body.scroll {
    @apply block overflow-y-auto;
  }
</style>
