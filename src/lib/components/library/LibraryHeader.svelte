<script lang="ts">
  import CardSizeSlider from './CardSizeSlider.svelte';

  // Props
  interface Props {
    title?: string;
    count?: number;
    countUnit?: string;
    searchPlaceholder?: string;
    searchTerm?: string;
    // eslint-disable-next-line no-unused-vars
    onSearchInput?: (value: string) => void;
    onSearchClear?: () => void;
    displayMode?: 'grid' | 'list';
    // eslint-disable-next-line no-unused-vars
    onDisplayModeChange?: (mode: 'grid' | 'list') => void;
    showGridMode?: boolean;
    showListMode?: boolean;
    showCardSizeSlider?: boolean;
  }

  let {
    title = '',
    count,
    countUnit = '曲',
    searchPlaceholder = '検索...',
    searchTerm = '',
    onSearchInput,
    onSearchClear,
    displayMode = 'list',
    onDisplayModeChange,
    showGridMode = true,
    showListMode = true,
    showCardSizeSlider = true
  }: Props = $props();

  // 内部検索状態
  let internalSearchTerm = $state(searchTerm);

  // 外部からの値で同期
  $effect(() => {
    internalSearchTerm = searchTerm;
  });

  function handleInput(event: Event) {
    const target = event.target as HTMLInputElement;
    internalSearchTerm = target.value;
    onSearchInput?.(target.value);
  }

  function handleClear() {
    internalSearchTerm = '';
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
      <span class="header-count">{count}{countUnit}</span>
    {/if}

    <!-- ビュー切り替えボタン -->
    <div class="view-toggle">
      <button
        class="view-btn"
        class:active={displayMode === 'list'}
        onclick={() => setDisplayMode('list')}
        disabled={!showListMode}
        title="リスト表示"
        aria-label="リスト表示"
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
        title="グリッド表示"
        aria-label="グリッド表示"
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

  <!-- 右側: 検索バー -->
  <div class="header-right">
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
      <input
        type="text"
        placeholder={searchPlaceholder}
        value={internalSearchTerm}
        oninput={handleInput}
        class="search-input"
      />
      {#if internalSearchTerm}
        <button onclick={handleClear} class="search-clear" aria-label="検索をクリア">✕</button>
      {/if}
    </div>
  </div>
</div>

<style>
  @reference "../../../app.css";
  .library-header {
    @apply flex items-center justify-between gap-4 py-3 px-4 border-b border-border;
  }

  .header-left {
    @apply flex items-center gap-3 min-w-0;
  }

  .header-title {
    @apply m-0 text-lg font-semibold text-text-primary;
  }

  .header-count {
    @apply text-sm text-text-muted mr-2;
  }

  .header-right {
    @apply flex items-center gap-3 shrink-0;
  }

  .search-box {
    @apply relative flex items-center;
  }

  .search-icon {
    @apply absolute left-3 w-4 h-4 text-text-dimmed;
  }

  .search-input {
    @apply w-56 py-2 pl-9 pr-8 bg-base-400 border border-border rounded-md text-sm text-text-primary transition-all duration-200;
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
