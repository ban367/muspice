<!--
  @component LibraryGrid
  ライブラリビュー（アルバム、アーティスト、ジャンル）の共通グリッド/リストコンポーネント。
  ローディング/エラー/空/検索結果なし状態の表示、browseSearchQueryフィルタリング、
  displayMode切替、コンテキストメニュー、キーボード操作（矢印キーでの移動）を共通化する。
  項目を押した時の操作（ジャンルの画面を開く・再生するなど）は、使う側が渡す。
  一覧は、見えている項目だけを描画する（VirtualList）。
  `detail`を渡すと、グリッド表示で選んだ項目（現在位置の項目）の中身を、グリッドの下の領域に出す
  （`GridDetailSplit`。閉じると現在位置を外す。ADR-097）。
  項目は、サイドバーのプレイリストへドラッグすると、その曲すべてを追加できる（落とした時に曲を取得する）。

  カードの見た目はSnippetでカスタマイズ可能。
-->
<script lang="ts" generics="T extends AlbumSummary | ArtistSummary | GenreSummary">
  import { tick, untrack, type Snippet } from 'svelte';
  import type { AlbumSummary, ArtistSummary, GenreSummary } from '#lib/types/models.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { GRID_CARD_GAP, ui } from '#lib/stores/ui.svelte.js';
  import { openableListAction } from '#lib/utils/listNavigation.js';
  import { matchesSearchTerms, searchTerms } from '#lib/utils/searchText.js';
  import { matchesReveal } from '#lib/utils/libraryReveal.js';
  import { startTrackGroupDrag, type TrackDragGroup } from '#lib/utils/trackDrag.js';
  import GroupContextMenu from '../GroupContextMenu.svelte';
  import GridDetailSplit from './GridDetailSplit.svelte';
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
    /** 絞り込みの対象にする項目（検索語を、このどれかに含む項目だけを表示する） */
    searchFields: (_item: T) => (string | null)[];
    /** 項目を識別するキー（省略時は名前。同じ名前の項目がある一覧では渡す） */
    getKey?: (_item: T) => string;
    /** グリッドカードSnippet */
    gridCard: Snippet<[T]>;
    /** リスト行Snippet */
    listRow: Snippet<[T]>;
    /** グリッド表示のカードの最小の幅（px）。幅に入るだけ列を並べる */
    minCardWidth: number;
    /**
     * グリッド表示のカードを、列の幅まで広げずに列の中央へ置くか
     *
     * カードが自分の幅を持つ一覧（ジャケット画像のカード）で使う。
     */
    centerCards?: boolean;
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
    /**
     * キーボードで選んだ項目をEnterで開く時の操作（ジャンルはクリックと同じく画面を開き、
     * アルバム・アーティストはダブルクリックと同じく再生する）
     */
    onOpen?: (_item: T) => void;
    /**
     * グリッド表示で選んだ項目の中身（グリッドの下の領域に出す。項目と、領域を閉じる関数を受け取る）
     */
    detail?: Snippet<[T, () => void]>;
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
    searchFields,
    getKey = (item) => item.name,
    gridCard,
    listRow,
    minCardWidth,
    centerCards = false,
    estimatedCardHeight,
    estimatedRowHeight,
    virtualized = true,
    groupType,
    onOpen,
    detail
  }: Props = $props();

  // 検索でフィルタリングされたアイテム（曲の検索と同じく、大文字と小文字・全角と半角・
  // ひらがなとカタカナの違いを同じとみなす）
  const filteredItems = $derived.by(() => {
    const terms = searchTerms(ui.browseSearchQuery);
    if (terms.length === 0) return items;
    return items.filter((item) => matchesSearchTerms(searchFields(item), terms));
  });

  // グリッド表示で、見えている範囲の前後に余分に描画する行の数（1行に何枚も並ぶため、少なくする）
  const GRID_OVERSCAN_ROWS = 2;

  // 見えている項目だけを描画する一覧（グリッド表示・リスト表示のどちらか）
  let virtualList = $state<VirtualList<T>>();

  // キーボードで移動する時の現在位置（項目のキー。クリックした項目もここに入る）
  let activeKey = $state<string | null>(null);

  /**
   * 一覧のキーボード操作
   *
   * 矢印キー（グリッドでは上下左右、リストでは上下）・Home・Endで現在位置を移し、Enterで開く。
   */
  function handleKeydown(event: KeyboardEvent) {
    const current = filteredItems.findIndex((item) => getKey(item) === activeKey);
    const columns = displayMode === 'grid' ? (virtualList?.getColumns() ?? 1) : null;
    const action = openableListAction(event, current, filteredItems.length, columns);
    if (action === null) return;

    const item = filteredItems[action.index];
    if (action.type === 'open') {
      onOpen?.(item);
      return;
    }
    // 既定の動作（一覧のスクロール）は止めてあるため、移動先の項目を見える位置へ出す
    activeKey = getKey(item);
    virtualList?.scrollToIndex(action.index);
  }

  /**
   * グリッドの下に中身を出す項目（グリッド表示で選んだ項目。検索で絞り込んで一覧から外れても出し続け、
   * ライブラリからなくなったら閉じる）
   */
  const detailItem = $derived(
    displayMode === 'grid' && detail && activeKey !== null
      ? (items.find((item) => getKey(item) === activeKey) ?? null)
      : null
  );

  /** 中身の領域を閉じる（現在位置を外す） */
  function closeDetail() {
    activeKey = null;
  }

  // 曲から移ってきた時（`ui.revealGroup`）は、グリッド表示でその項目を選び（中身の領域を開く）、
  // 見える位置へ出す（リスト表示では、`LibraryBrowsePage`が左の一覧で選ぶ。ADR-110）
  $effect(() => {
    const request = ui.revealGroup;
    if (request === null || request.kind !== groupType || displayMode !== 'grid' || isLoading) {
      return;
    }
    const item = items.find((candidate) => matchesReveal(request, candidate)) ?? null;
    untrack(() => {
      ui.revealGroup = null;
      if (item === null) return;
      const key = getKey(item);
      activeKey = key;
      void tick().then(() => {
        const index = filteredItems.findIndex((candidate) => getKey(candidate) === key);
        if (index >= 0) virtualList?.scrollToIndex(index);
      });
    });
  });

  // 中身の領域を開いた時は、グリッドが低くなった後に、選んだ項目を見える位置へ出す
  // （下の方の項目を選ぶと、領域の下に隠れるため）
  let isDetailOpen = false;
  $effect(() => {
    const isOpen = detailItem !== null;
    if (isOpen === isDetailOpen) return;
    isDetailOpen = isOpen;
    if (!isOpen) return;
    const key = activeKey;
    void tick().then(() => {
      // 高さが変わったのを一覧が測ってから（ResizeObserver）スクロールする
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          const index = filteredItems.findIndex((item) => getKey(item) === key);
          if (index >= 0) virtualList?.scrollToIndex(index);
        })
      );
    });
  });

  /** 項目を、ドラッグで運ぶまとまりにする（アルバムは、まとめたアーティストでも見分ける） */
  function dragGroup(item: T): TrackDragGroup {
    if (groupType === 'album') {
      return { type: 'album', name: item.name, albumArtist: 'artist' in item ? item.artist : null };
    }
    return { type: groupType, name: item.name };
  }

  /** 項目のドラッグを始める（サイドバーのプレイリストへ落とすと、その曲すべてを追加する） */
  function handleDragStart(event: DragEvent, item: T) {
    startTrackGroupDrag(event, dragGroup(item), {
      label: item.name,
      detail: m.common.trackCount(item.trackCount)
    });
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
      <GridDetailSplit detail={detailItem ? detailPane : undefined}>
        {#snippet grid()}
          <VirtualList
            bind:this={virtualList}
            items={filteredItems}
            {getKey}
            estimatedRowHeight={estimatedCardHeight}
            minColumnWidth={minCardWidth}
            gap={GRID_CARD_GAP}
            overscan={virtualized ? GRID_OVERSCAN_ROWS : Infinity}
            scrollerClass="p-2"
            class="outline-none {centerCards ? 'justify-items-center' : ''}"
            role="listbox"
            aria-label={itemLabel}
            tabindex={0}
            onkeydown={handleKeydown}
          >
            {#snippet row(item)}
              <!-- svelte-ignore a11y_interactive_supports_focus -->
              <div
                class="grid-item"
                class:active={getKey(item) === activeKey}
                role="option"
                aria-selected={getKey(item) === activeKey}
                draggable="true"
                onpointerdown={() => (activeKey = getKey(item))}
                ondragstart={(event) => handleDragStart(event, item)}
              >
                {@render gridCard(item)}
              </div>
            {/snippet}
          </VirtualList>
        {/snippet}
      </GridDetailSplit>
    {:else}
      <VirtualList
        bind:this={virtualList}
        items={filteredItems}
        {getKey}
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
            class:active={getKey(item) === activeKey}
            role="option"
            aria-selected={getKey(item) === activeKey}
            draggable="true"
            onpointerdown={() => (activeKey = getKey(item))}
            ondragstart={(event) => handleDragStart(event, item)}
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

<!-- グリッドの下に出す、選んだ項目の中身 -->
{#snippet detailPane()}
  {#if detailItem && detail}
    {@render detail(detailItem, closeDetail)}
  {/if}
{/snippet}

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

  /* キーボードで移動する時の現在位置（グリッド表示の`.grid-item`は`app.css`） */
  .list-item {
    @apply rounded-md;
  }

  .list-item.active {
    @apply bg-surface-active;
  }
</style>
