<!--
  @component LibraryBrowsePage
  ライブラリブラウズページ（アルバム/アーティスト/ジャンル）の共通レイアウト。
  検索デバウンス・表示モード切替・2ペインレイアウト・選択管理を一元化し、
  各ページ固有の表示はSnippetで注入する。

  - listView / detailView を渡すと、リストモードで2ペインレイアウトになる
  - 渡さない場合は常に gridView が表示モードに応じて描画される（ジャンルページ等）
-->
<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import { onDestroy } from 'svelte';
  import LibraryHeader from './LibraryHeader.svelte';
  import { browseSearchQuery } from '$lib/stores/ui';
  import { createSearchDebounce } from '$lib/utils/debounce';
  import { invalidateTrackListQueries } from '$lib/queries/tracks';
  import { useQueryClient } from '@tanstack/svelte-query';

  // Props
  interface Props {
    /** ページタイトル（例: アルバム） */
    title: string;
    /** 件数の単位（例: 枚、人、種類） */
    countUnit: string;
    /** 検索ボックスのプレースホルダー */
    searchPlaceholder: string;
    /** 初期表示モード */
    initialDisplayMode?: 'grid' | 'list';
    /** ブラウズ対象のアイテム一覧（2ペイン表示に使用） */
    items?: T[];
    /** 検索クエリ（小文字化・trim済み）によるフィルタ関数 */
    // eslint-disable-next-line no-unused-vars
    filterFn?: (item: T, query: string) => boolean;
    /** ヘッダーに表示する件数（省略時はitemsの件数） */
    count?: number;
    /** 詳細未選択時のプロンプト文言（例: アルバムを選択してください） */
    emptyPrompt?: string;
    /** グリッドモードでカードサイズスライダーを表示するか */
    enableCardSizeSlider?: boolean;
    /** リストペインの幅 */
    listPaneWidth?: string;
    /** 詳細未選択時に表示するアイコン */
    emptyIcon?: Snippet;
    /** グリッド表示（現在の表示モードを受け取る） */
    gridView: Snippet<['grid' | 'list']>;
    /** リストモードの左ペイン（フィルタ済みアイテム・選択中・選択ハンドラを受け取る） */
    // eslint-disable-next-line no-unused-vars
    listView?: Snippet<[T[], T | null, (item: T) => void]>;
    /** リストモードの右ペイン（選択中アイテムを受け取る） */
    detailView?: Snippet<[T]>;
  }

  let {
    title,
    countUnit,
    searchPlaceholder,
    initialDisplayMode = 'list',
    items = [],
    filterFn,
    count,
    emptyPrompt = '',
    enableCardSizeSlider = true,
    listPaneWidth = '18rem',
    emptyIcon,
    gridView,
    listView,
    detailView
  }: Props = $props();

  // QueryClient for refetching
  const queryClient = useQueryClient();

  // 表示モード（初期値のみpropsから取得する意図的な設計）
  // svelte-ignore state_referenced_locally
  let displayMode = $state<'grid' | 'list'>(initialDisplayMode);

  // 2ペイン表示を持つページかどうか
  const hasTwoPaneList = $derived(listView != null && detailView != null);

  // 件数（明示指定がなければitemsから算出）
  const itemCount = $derived(count ?? items.length);

  // 検索でフィルタリングされたアイテム
  const filteredItems = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query || !filterFn) return items;
    return items.filter((item) => filterFn(item, query));
  });

  // 選択されたアイテム（リストモード用）
  let selectedItem = $state<T | null>(null);

  // リストモードで最初のアイテムを自動選択
  $effect(() => {
    if (hasTwoPaneList && displayMode === 'list' && filteredItems.length > 0 && !selectedItem) {
      selectedItem = filteredItems[0];
    }
  });

  // 検索状態（ページ間でストアが共有されるため、現在値で初期化して表示と一致させる）
  let searchTerm = $state($browseSearchQuery);

  const { handleInput: debouncedSearch, cancel: cancelSearchDebounce } = createSearchDebounce(
    (query) => browseSearchQuery.set(query)
  );

  onDestroy(cancelSearchDebounce);

  function handleSearchInput(value: string) {
    searchTerm = value;
    debouncedSearch(value);
  }

  function clearSearch() {
    searchTerm = '';
    cancelSearchDebounce();
    browseSearchQuery.set('');
  }

  function handleDisplayModeChange(mode: 'grid' | 'list') {
    displayMode = mode;
    // モード切り替え時に選択をリセット
    selectedItem = null;
  }

  function handleItemSelect(item: T) {
    selectedItem = item;
  }

  function handleRefreshComplete() {
    invalidateTrackListQueries(queryClient);
  }
</script>

<div class="browse-page">
  <LibraryHeader
    {title}
    count={itemCount}
    {countUnit}
    {searchPlaceholder}
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={enableCardSizeSlider && displayMode === 'grid'}
    onRefreshComplete={handleRefreshComplete}
  />

  {#if !hasTwoPaneList || displayMode === 'grid'}
    <!-- グリッド表示 -->
    <div class="grid-container">
      {@render gridView(displayMode)}
    </div>
  {:else}
    <!-- リスト表示（2ペインレイアウト） -->
    <div class="list-layout">
      <!-- 左ペイン: アイテムリスト -->
      <div class="list-pane" style="width: {listPaneWidth}">
        {@render listView?.(filteredItems, selectedItem, handleItemSelect)}
      </div>

      <!-- 右ペイン: 詳細表示 -->
      <div class="detail-pane">
        {#if selectedItem}
          {@render detailView?.(selectedItem)}
        {:else}
          <div class="empty-detail-view">
            <div class="empty-detail">
              {@render emptyIcon?.()}
              <p>{emptyPrompt}</p>
              <span>{itemCount}{countUnit}の{title}</span>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../../app.css";

  .browse-page {
    @apply flex flex-col h-full;
  }

  .grid-container {
    @apply flex-1 overflow-auto;
  }

  .list-layout {
    @apply flex-1 flex overflow-hidden;
  }

  .list-pane {
    @apply shrink-0 border-r border-border overflow-hidden;
  }

  .detail-pane {
    @apply flex-1 overflow-hidden;
  }

  .empty-detail-view {
    @apply h-full flex items-center justify-center;
  }

  .empty-detail {
    @apply flex flex-col items-center text-center text-text-dimmed;
  }

  .empty-detail :global(svg) {
    @apply w-16 h-16 text-text-dimmed/50 mb-4;
  }

  .empty-detail p {
    @apply text-base text-text-muted m-0;
  }

  .empty-detail span {
    @apply text-sm mt-2;
  }
</style>
