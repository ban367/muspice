<script lang="ts">
  import type { Snippet } from 'svelte';
  import CardSizeSlider from './CardSizeSlider.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    title?: string;
    count?: number;
    /** 件数の表示（省略時は曲数。例: 12曲） */
    formatCount?: (count: number) => string;
    /** 検索ボックスのプレースホルダー（省略時は「検索...」） */
    searchPlaceholder?: string;
    searchTerm?: string;
    onSearchInput?: (value: string) => void;
    onSearchClear?: () => void;
    displayMode?: 'grid' | 'list';
    onDisplayModeChange?: (mode: 'grid' | 'list') => void;
    showGridMode?: boolean;
    showListMode?: boolean;
    showCardSizeSlider?: boolean;
    /** 検索欄の左に出す、画面ごとのボタン（絞り込みなど） */
    actions?: Snippet;
  }

  let {
    title = '',
    count,
    formatCount = (count: number) => m.common.trackCount(count),
    searchPlaceholder,
    searchTerm = '',
    onSearchInput,
    onSearchClear,
    displayMode = 'list',
    onDisplayModeChange,
    showGridMode = true,
    showListMode = true,
    showCardSizeSlider = true,
    actions
  }: Props = $props();

  // 検索語は、呼び出し側が持つ（入力のたびに`onSearchInput`で更新し、`searchTerm`で返してもらう）
  function handleInput(event: Event) {
    onSearchInput?.((event.target as HTMLInputElement).value);
  }

  function handleClear() {
    onSearchClear?.();
  }

  function setDisplayMode(mode: 'grid' | 'list') {
    onDisplayModeChange?.(mode);
  }
</script>

<div class="library-header">
  <!-- 左側: タイトル、カウント、表示切り替え、スライダー -->
  <div class="header-left">
    {#if title}
      <h2 class="header-title">{title}</h2>
    {/if}
    {#if count !== undefined}
      <span class="header-count">{formatCount(count)}</span>
    {/if}

    <!-- ビュー切り替えボタン -->
    <div class="view-toggle">
      <button
        class="view-btn"
        class:active={displayMode === 'list'}
        onclick={() => setDisplayMode('list')}
        disabled={!showListMode}
        title={m.library.listView}
        aria-label={m.library.listView}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-5 h-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 6h16M4 10h16M4 14h16M4 18h16"
          />
        </svg>
      </button>
      <button
        class="view-btn"
        class:active={displayMode === 'grid'}
        onclick={() => setDisplayMode('grid')}
        disabled={!showGridMode}
        title={m.library.gridView}
        aria-label={m.library.gridView}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-5 h-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"
          />
        </svg>
      </button>
    </div>

    <!-- カードサイズスライダー -->
    {#if showCardSizeSlider && displayMode === 'grid'}
      <CardSizeSlider />
    {/if}
  </div>

  <!-- 右側: 画面ごとのボタン、検索バー（ライブラリの再読み込みは、メニューバーの「ファイル」。ADR-128） -->
  <div class="header-right">
    {@render actions?.()}
    <div class="search-box">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="search-icon"
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
      <!-- メニューバーの「検索」（⌘F）は、この欄へ移る（`stores/menuCommands.ts`） -->
      <input
        type="text"
        data-library-search
        placeholder={searchPlaceholder ?? m.library.search}
        aria-label={searchPlaceholder ?? m.library.search}
        value={searchTerm}
        oninput={handleInput}
        class="search-input"
      />
      {#if searchTerm}
        <button onclick={handleClear} class="search-clear" aria-label={m.library.clearSearch}>
          ✕
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  @reference "../../../app.css";

  /* 幅が狭い時（右サイドバーのパネルを並べている時など）は、検索欄を縮め、それでも入らなければ
     項目を次の行へ送る（見出しの文字は折り返さない） */
  .library-header {
    @apply flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-3 px-4 border-b border-border;
  }

  .header-left {
    @apply flex flex-wrap items-center gap-3 min-w-0;
  }

  .header-title {
    @apply m-0 text-lg font-semibold text-text-primary whitespace-nowrap;
  }

  .header-count {
    @apply text-sm text-text-muted mr-2 whitespace-nowrap;
  }

  /* 次の行へ送るかは、検索欄を縮めた幅（12rem）で決め、余った幅は検索欄が元の幅まで使う */
  .header-right {
    @apply flex flex-[1_1_12rem] items-center justify-end gap-3 min-w-0;
  }

  .search-box {
    @apply relative flex items-center min-w-0;
  }

  .search-icon {
    @apply absolute left-3 w-4 h-4 text-text-dimmed;
  }

  .search-input {
    @apply w-56 min-w-0 py-2 pl-9 pr-8 bg-base-400 border border-border rounded-md text-sm text-text-primary transition-all duration-200;
  }

  .search-input:focus {
    @apply outline-none border-primary w-72;
  }

  .search-input::placeholder {
    @apply text-text-dimmed;
  }

  .search-clear {
    @apply absolute right-2 p-1 text-text-dimmed hover:text-text-primary bg-transparent border-none cursor-pointer;
  }

  .view-toggle {
    @apply flex items-center bg-base-400 rounded-md p-0.5;
  }

  .view-btn {
    @apply flex items-center justify-center w-8 h-8 bg-transparent border-none rounded text-text-muted cursor-pointer transition-all duration-150;
  }

  .view-btn:hover:not(:disabled) {
    @apply text-text-primary bg-surface-hover;
  }

  .view-btn.active {
    @apply bg-primary text-primary-content;
  }

  .view-btn:disabled {
    @apply opacity-40 cursor-not-allowed;
  }
</style>
