<!--
  @component TreeBrowser
  木の形の一覧（フォルダ別・年代別の画面の、左側の一覧）。
  項目のクリックで選び、三角のボタン（または項目のダブルクリック）で、その下の項目を開閉する。
-->
<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { flattenTree, type TreeNode, type TreeRow } from '#lib/utils/libraryViews.js';

  interface Props {
    /** 木の根の項目 */
    nodes: TreeNode[];
    /** 選んでいる項目のキー */
    selectedKey: string | null;
    onSelect: (key: string) => void;
    /** 一覧の名前（スクリーンリーダー向け） */
    label: string;
    /** 最初から開いておく項目のキー */
    initiallyExpanded?: readonly string[];
  }

  let { nodes, selectedKey, onSelect, label, initiallyExpanded = [] }: Props = $props();

  const ROW_HEIGHT = 30;
  /** 1段ごとの字下げ（px） */
  const INDENT = 14;

  // 開いている項目（最初に受け取ったものから始める）
  // svelte-ignore state_referenced_locally
  const expanded = new SvelteSet<string>(initiallyExpanded);
  const rows = $derived(flattenTree(nodes, expanded));

  function toggle(row: TreeRow) {
    if (!row.hasChildren) return;
    if (expanded.has(row.node.key)) expanded.delete(row.node.key);
    else expanded.add(row.node.key);
  }
</script>

<VirtualList
  items={rows}
  getKey={(row) => row.node.key}
  estimatedRowHeight={ROW_HEIGHT}
  role="tree"
  aria-label={label}
>
  {#snippet row(item)}
    <div
      class="tree-row"
      class:selected={item.node.key === selectedKey}
      style="height: {ROW_HEIGHT}px; padding-left: {8 + item.depth * INDENT}px;"
      role="treeitem"
      aria-level={item.depth + 1}
      aria-selected={item.node.key === selectedKey}
      aria-expanded={item.hasChildren ? item.isExpanded : undefined}
    >
      <button
        type="button"
        class="toggle"
        class:invisible={!item.hasChildren}
        onclick={() => toggle(item)}
        tabindex={item.hasChildren ? 0 : -1}
        aria-hidden={!item.hasChildren}
        aria-label={item.node.label}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-3 h-3 transition-transform"
          class:rotate-90={item.isExpanded}
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>
      <button
        type="button"
        class="tree-label"
        title={item.node.key}
        onclick={() => onSelect(item.node.key)}
        ondblclick={() => toggle(item)}
      >
        <span class="truncate">{item.node.label}</span>
        <span class="count">{item.node.count}</span>
      </button>
    </div>
  {/snippet}
</VirtualList>

<style>
  @reference "../../../app.css";

  .tree-row {
    @apply flex items-center gap-1 pr-2 text-sm text-text-secondary select-none;
  }

  .tree-row:hover {
    @apply bg-surface;
  }

  .tree-row.selected {
    @apply bg-primary/20 text-text-primary;
  }

  .toggle {
    @apply flex items-center justify-center w-5 h-5 shrink-0 p-0 bg-transparent border-none text-text-dimmed cursor-pointer rounded;
  }

  .toggle:hover {
    @apply text-text-primary;
  }

  .tree-label {
    @apply flex items-center gap-2 flex-1 min-w-0 h-full p-0 bg-transparent border-none text-left text-inherit cursor-pointer;
    font: inherit;
  }

  .count {
    @apply ml-auto shrink-0 text-xs text-text-dimmed tabular-nums;
  }
</style>
