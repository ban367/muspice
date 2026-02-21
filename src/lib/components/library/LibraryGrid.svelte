<!--
  @component LibraryGrid
  ライブラリビュー（アルバム、アーティスト、ジャンル）の共通グリッド/リストコンポーネント。
  ローディング/エラー/空/検索結果なし状態の表示、browseSearchQueryフィルタリング、
  displayMode切替、コンテキストメニューを共通化する。

  カードの見た目はSnippetでカスタマイズ可能。
-->
<script lang="ts" generics="T extends AlbumGroup | ArtistGroup | GenreGroup">
  import type { Snippet } from 'svelte';
  import type { AlbumGroup, ArtistGroup, GenreGroup } from '$lib/types/models';
  import { browseSearchQuery } from '$lib/stores/ui';
  import GroupContextMenu from '../GroupContextMenu.svelte';

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
    /** グリッドのCSSスタイル（CSS変数等） */
    gridStyle?: string;
    /** グリッドのCSSクラス */
    gridClass?: string;
    /** GroupContextMenuのtype */
    groupType: 'album' | 'artist' | 'genre';
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
    gridStyle = '',
    gridClass = '',
    groupType,
    footer
  }: Props = $props();

  // 検索でフィルタリングされたアイテム
  const filteredItems = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return items;
    return items.filter((item) => filterFn(item, query));
  });

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

<div class="p-2 min-h-[200px]">
  {#if isLoading}
    <div class="state-container">
      <div class="spinner"></div>
      <p>{itemLabel}を読み込み中...</p>
    </div>
  {:else if isError}
    <div class="state-container">
      <p class="text-error-light">{itemLabel}の読み込みに失敗しました</p>
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
      <p>「{$browseSearchQuery}」に一致する{itemLabel}が見つかりません</p>
    </div>
  {:else if filteredItems.length > 0}
    {#if displayMode === 'grid'}
      <div class={gridClass} style={gridStyle}>
        {#each filteredItems as item (item.name)}
          {@render gridCard(item)}
        {/each}
      </div>
    {:else}
      <div class="item-list">
        {#each filteredItems as item (item.name)}
          {@render listRow(item)}
        {/each}
      </div>
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

  .item-list {
    @apply flex flex-col;
  }
</style>
