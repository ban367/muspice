<!--
  @component LibraryGrid
  ライブラリビュー（アルバム、アーティスト、ジャンル）の共通グリッド/リストコンポーネント。
  ローディング/エラー/空/検索結果なし状態の表示、browseSearchQueryフィルタリング、
  displayMode切替、コンテキストメニュー、キーボード操作（矢印キーでの移動）を共通化する。
  一覧は、見えている項目だけを描画する（VirtualList）。

  カードの見た目はSnippetでカスタマイズ可能。
-->
<script lang="ts" generics="T extends AlbumSummary | ArtistSummary | GenreSummary">
  import type { Snippet } from 'svelte';
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { navigationTarget } from '#lib/utils/listNavigation.js';
  import GroupContextMenu from '../GroupContextMenu.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    /** 表示するアイテム一覧（クエリ結果） */
    items: T[];
    /** ローディング状態 */
    isLoading: boolean;
    /** エラー状態 */
    isError: boolean;
    /** 表示モード */
    displayMode?: 'grid' | 'list';
    /** アイテム名（ローディングメッセージ等に使用） */
    itemLabel: string;
    /** 空状態のアイコンSVGパス */
    emptyIcon: Snippet;
    /** 空状態のメッセージ */
    emptyMessage: string;
    /** 空状態の補足メッセージ */
    emptyHint: string;
    /** 検索フィルター関数 */
    filterFn: (_item: T, _query: string) => boolean;
    /** グリッドカードSnippet */
    gridCard: Snippet<[T]>;
    /** リスト行Snippet */
    listRow: Snippet<[T]>;
    /** グリッド表示のカードの最小の幅（px）。幅に入るだけ列を並べる */
    minCardWidth: number;
    /** グリッド表示のカードの高さの見積もり（px）。描画した後は実測した高さを使う */
    estimatedCardHeight: number;
    /** リスト表示の行の高さの見積もり（px）。描画した後は実測した高さを使う */
    estimatedRowHeight: number;
    /**
     * 見えている項目だけを描画するか
     *
     * 項目の高さがそろわない一覧（名前の長さで高さが変わるジャンルのカード）では、
     * 位置を正しく求められないため、falseにしてすべて描画する。
     */
    virtualized?: boolean;
    /** GroupContextMenuのtype */
    groupType: 'album' | 'artist' | 'genre';
    /** キーボードで選んだ項目をEnterで開く時の操作（カード・行のクリックと同じ操作を渡す） */
    onOpen?: (_item: T) => void;
    /** グリッド/リストの下に追加するコンテンツ（モーダル等） */
    footer?: Snippet;
  }

  let {
    items,
    isLoading,
    isError,
    displayMode = 'grid',
    itemLabel,
    emptyIcon,
    emptyMessage,
    emptyHint,
    filterFn,
    gridCard,
    listRow,
    minCardWidth,
    estimatedCardHeight,
    estimatedRowHeight,
    virtualized = true,
    groupType,
    onOpen,
    footer
  }: Props = $props();

  // 検索でフィルタリングされたアイテム
  const filteredItems = $derived.by(() => {
    const query = ui.browseSearchQuery.toLowerCase().trim();
    if (!query) return items;
    return items.filter((item) => filterFn(item, query));
  });

  // グリッド表示のカードの間隔（px）
  const GRID_GAP = 12;
  // グリッド表示で、見えている範囲の前後に余分に描画する行の数（1行に何枚も並ぶため、少なくする）
  const GRID_OVERSCAN_ROWS = 2;

  // 見えている項目だけを描画する一覧（グリッド表示・リスト表示のどちらか）
  let virtualList = $state<VirtualList<T>>();

  // キーボードで移動する時の現在位置（項目の名前。クリックした項目もここに入る）
  let activeName = $state<string | null>(null);

  /**
   * 一覧のキーボード操作
   *
   * 矢印キー（グリッドでは上下左右、リストでは上下）・Home・Endで現在位置を移し、Enterで開く。
   */
  function handleKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return;

    if (event.key === 'Enter') {
      // カードの中のボタン（再生）では、そのボタンの操作を優先する
      if (event.target instanceof Element && event.target.closest('button, a')) return;
      const item = filteredItems.find((candidate) => candidate.name === activeName);
      if (item && onOpen) {
        onOpen(item);
        event.preventDefault();
      }
      return;
    }

    const current = filteredItems.findIndex((item) => item.name === activeName);
    const columns = displayMode === 'grid' ? (virtualList?.getColumns() ?? 1) : null;
    const target = navigationTarget(event.key, current, filteredItems.length, columns);
    if (target === null) return;

    // 既定の動作（一覧のスクロール）の代わりに、移動先の項目を見える位置へ出す
    event.preventDefault();
    activeName = filteredItems[target].name;
    virtualList?.scrollToIndex(target);
  }

  // コンテキストメニュー
  let contextMenu = $state<{ x: number; y: number; item: T } | null>(null);

  // 右クリックメニューを表示
  export function handleContextMenu(event: MouseEvent, item: T) {
    event.preventDefault();
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      item
    };
  }

  // 右クリックメニューを閉じる
  function closeContextMenu() {
    contextMenu = null;
  }
</script>

<div class="h-full min-h-[200px]">
  {#if isLoading}
    <div class="state-container">
      <div class="spinner"></div>
      <p>{m.library.loadingItems(itemLabel)}</p>
    </div>
  {:else if isError}
    <div class="state-container">
      <p class="text-error-light">{m.library.loadItemsFailed(itemLabel)}</p>
    </div>
  {:else if items.length > 0 && filteredItems.length === 0}
    <div class="state-container">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-12 h-12 text-text-dimmed/50 mb-4"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
        />
      </svg>
      <p>{m.library.noMatch(ui.browseSearchQuery, itemLabel)}</p>
    </div>
  {:else if filteredItems.length > 0}
    <!-- 一覧がフォーカスを受けてキー操作を扱い、現在位置の項目を枠・背景で示す -->
    {#if displayMode === 'grid'}
      <VirtualList
        bind:this={virtualList}
        items={filteredItems}
        getKey={(item) => item.name}
        estimatedRowHeight={estimatedCardHeight}
        minColumnWidth={minCardWidth}
        gap={GRID_GAP}
        overscan={virtualized ? GRID_OVERSCAN_ROWS : Infinity}
        scrollerClass="p-2"
        class="outline-none"
        role="listbox"
        aria-label={itemLabel}
        tabindex={0}
        onkeydown={handleKeydown}
      >
        {#snippet row(item)}
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div
            class="grid-item"
            class:active={item.name === activeName}
            role="option"
            aria-selected={item.name === activeName}
            onpointerdown={() => (activeName = item.name)}
          >
            {@render gridCard(item)}
          </div>
        {/snippet}
      </VirtualList>
    {:else}
      <VirtualList
        bind:this={virtualList}
        items={filteredItems}
        getKey={(item) => item.name}
        {estimatedRowHeight}
        overscan={virtualized ? undefined : Infinity}
        scrollerClass="p-2"
        class="outline-none"
        role="listbox"
        aria-label={itemLabel}
        tabindex={0}
        onkeydown={handleKeydown}
      >
        {#snippet row(item)}
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div
            class="list-item"
            class:active={item.name === activeName}
            role="option"
            aria-selected={item.name === activeName}
            onpointerdown={() => (activeName = item.name)}
          >
            {@render listRow(item)}
          </div>
        {/snippet}
      </VirtualList>
    {/if}
  {:else}
    <div class="state-container">
      {@render emptyIcon()}
      <p>{emptyMessage}</p>
      <span>{emptyHint}</span>
    </div>
  {/if}
</div>

<!-- フッター（モーダル等） -->
{#if footer}
  {@render footer()}
{/if}

<!-- コンテキストメニュー -->
{#if contextMenu}
  <GroupContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    group={contextMenu.item}
    type={groupType}
    onClose={closeContextMenu}
  />
{/if}

<style>
  @reference "../../../app.css";

  /* キーボードで移動する時の現在位置 */
  .grid-item {
    @apply rounded-lg;
  }

  .grid-item.active {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .list-item {
    @apply rounded-md;
  }

  .list-item.active {
    @apply bg-surface-active;
  }
</style>
