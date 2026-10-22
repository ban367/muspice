<!--
  @component TreeBrowser
  木の形の一覧（フォルダ別・年代別の画面の、左側の一覧）。
  項目のクリックで選び、三角（または項目のダブルクリック）で、その下の項目を開閉する。
  キーボードでは、Tabで選んでいる項目に止まり、↑↓で選ぶ項目を移し、→←で開閉する（`treeKeyAction`）。
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { flattenTree, type TreeNode, type TreeRow } from '#lib/utils/libraryViews.js';
  import { treeKeyAction } from '#lib/utils/listNavigation.js';

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

  let virtualList = $state<VirtualList<TreeRow>>();

  // Tabで止まる行（選んでいる行。選んでいる行が閉じた項目の中に隠れた時・選んでいない時は先頭の行）
  const focusKey = $derived(
    rows.some((row) => row.node.key === selectedKey) ? selectedKey : (rows[0]?.node.key ?? null)
  );

  function toggle(row: TreeRow) {
    if (!row.hasChildren) return;
    if (expanded.has(row.node.key)) expanded.delete(row.node.key);
    else expanded.add(row.node.key);
  }

  async function handleKeydown(event: KeyboardEvent) {
    const tree = event.currentTarget;
    const current = rows.findIndex((row) => row.node.key === selectedKey);
    const action = treeKeyAction(event, rows, current);
    if (action === null) return;

    const target = rows[action.index];
    if (action.type === 'expand') expanded.add(target.node.key);
    else if (action.type === 'collapse') expanded.delete(target.node.key);
    else {
      onSelect(target.node.key);
      virtualList?.scrollToIndex(action.index);
    }
    // 選んだ行へフォーカスも移す（前の行に残すと、次のキーが前の行に効いてしまう）
    await tick();
    if (tree instanceof HTMLElement) {
      tree.querySelector<HTMLElement>('[role="treeitem"][tabindex="0"]')?.focus();
    }
  }
</script>

<VirtualList
  bind:this={virtualList}
  items={rows}
  getKey={(row) => row.node.key}
  estimatedRowHeight={ROW_HEIGHT}
  class="outline-none"
  role="tree"
  aria-label={label}
  tabindex={-1}
  onkeydown={handleKeydown}
>
  {#snippet row(item)}
    <!-- キー操作は一覧（tree）で受ける。行のEnter・Spaceも`treeKeyAction`が扱う -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="tree-row"
      class:selected={item.node.key === selectedKey}
      style="height: {ROW_HEIGHT}px; padding-left: {8 + item.depth * INDENT}px;"
      role="treeitem"
      tabindex={item.node.key === focusKey ? 0 : -1}
      title={item.node.key}
      aria-level={item.depth + 1}
      aria-selected={item.node.key === selectedKey}
      aria-expanded={item.hasChildren ? item.isExpanded : undefined}
      onclick={() => onSelect(item.node.key)}
      ondblclick={() => toggle(item)}
    >
      <!-- 開閉はキーボードでは→←で行うため、三角はポインタだけで押す（フォーカスを受けない） -->
      <span
        class="toggle"
        class:invisible={!item.hasChildren}
        onclick={(event) => {
          // 行のクリック（選ぶ）にしない
          event.stopPropagation();
          toggle(item);
        }}
        ondblclick={(event) => event.stopPropagation()}
        aria-hidden="true"
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
      </span>
      <span class="tree-label">
        <span class="truncate">{item.node.label}</span>
        <span class="count">{item.node.count}</span>
      </span>
    </div>
  {/snippet}
</VirtualList>

<style>
  @reference "../../../app.css";

  .tree-row {
    @apply flex items-center gap-1 pr-2 text-sm text-text-secondary select-none cursor-pointer outline-none;
  }

  /* 選んでいる行は背景の色で示す。選ぶ前（Tabで入った先頭の行）は、フォーカスの枠で示す */
  .tree-row:focus-visible:not(.selected) {
    box-shadow: inset 0 0 0 2px var(--color-primary);
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
    @apply flex items-center gap-2 flex-1 min-w-0 h-full;
  }

  .count {
    @apply ml-auto shrink-0 text-xs text-text-dimmed tabular-nums;
  }
</style>
