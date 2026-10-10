<!--
  曲の画面（ライブラリの全曲の一覧）。

  検索・フィルタ（評価・年・お気に入り）・カラムブラウザ（ジャンル → アーティスト → アルバム）の
  すべてに合う曲を、一覧に出す。絞り込みは、取得済みの曲の一覧に対して行う
  （`#lib/utils/trackFilter`）。
-->
<script lang="ts">
  import { useTracksQuery, useSearchQuery } from '#lib/queries/tracks.js';
  import { sanitizeSearchQuery } from '#lib/utils/validation.js';
  import TrackList from '#lib/components/library/TrackList.svelte';
  import LibraryHeader from '#lib/components/library/LibraryHeader.svelte';
  import ColumnBrowser from '#lib/components/library/ColumnBrowser.svelte';
  import TrackFilterMenu from '#lib/components/library/TrackFilterMenu.svelte';
  import { libraryBrowser } from '#lib/stores/libraryBrowser.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { applyTrackFilters, browseTracks, type BrowserColumn } from '#lib/utils/trackFilter.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 表示モード
  let displayMode = $state<'grid' | 'list'>('list');

  // 検索状態
  let searchTerm = $state('');
  let debouncedSearchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // クエリ
  const tracksQuery = useTracksQuery();

  // 検索クエリ
  const searchQuery = $derived.by(() => {
    if (debouncedSearchTerm.length > 0) {
      return useSearchQuery(debouncedSearchTerm);
    }
    return null;
  });

  // アクティブなクエリ
  const activeQuery = $derived(searchQuery || tracksQuery);
  const isLoading = $derived(activeQuery.isLoading);
  const isError = $derived(activeQuery.isError);
  const error = $derived(activeQuery.error);

  // 絞り込み: 検索に合う曲 → フィルタ → カラムブラウザの選択
  const searchedTracks = $derived(activeQuery.data ?? null);
  const filteredTracks = $derived(
    searchedTracks ? applyTrackFilters(searchedTracks, libraryBrowser.filters) : null
  );
  // カラムブラウザを隠している間は、選択がない（隠す時に解除する）ため、集計を省く
  const browsed = $derived(
    filteredTracks && libraryBrowser.isVisible
      ? browseTracks(filteredTracks, libraryBrowser.selection)
      : null
  );
  const tracks = $derived(browsed ? browsed.tracks : filteredTracks);

  const trackCount = $derived(tracks?.length ?? 0);
  // ライブラリの曲数（絞り込んでいる時に、「12 / 340曲」のように出す）
  const totalCount = $derived(tracksQuery.data?.length ?? 0);
  const isNarrowed = $derived(debouncedSearchTerm !== '' || libraryBrowser.isActive);

  function handleSearchInput(value: string) {
    searchTerm = value;

    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
      debouncedSearchTerm = sanitizeSearchQuery(searchTerm);
    }, 300);
  }

  function clearSearch() {
    searchTerm = '';
    debouncedSearchTerm = '';
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  }

  /** 検索・フィルタ・カラムブラウザの選択を、すべて解除する */
  function clearNarrowing() {
    clearSearch();
    libraryBrowser.clear();
  }

  function handleDisplayModeChange(mode: 'grid' | 'list') {
    displayMode = mode;
  }

  /** カラムブラウザの選択を変える（上の列を変えたら、下の列の選択は解除する） */
  function handleBrowserSelect(column: BrowserColumn, keys: string[]) {
    const { selection } = libraryBrowser;
    libraryBrowser.selection = {
      genres: column === 'genres' ? keys : selection.genres,
      artists: column === 'artists' ? keys : column === 'genres' ? [] : selection.artists,
      albums: column === 'albums' ? keys : []
    };
  }

  // 再生中の曲へのジャンプ: 絞り込んでいると、その曲が一覧にないことがあるため、絞り込みを解除する
  $effect(() => {
    if (ui.revealTrackId !== null && (searchTerm !== '' || libraryBrowser.isActive)) {
      clearNarrowing();
    }
  });
</script>

<div class="songs-page">
  <LibraryHeader
    title={m.library.songs}
    count={trackCount}
    formatCount={(count) =>
      isNarrowed ? m.library.filteredCount(count, totalCount) : m.library.songCount(count)}
    searchPlaceholder={m.library.searchSongs}
    {searchTerm}
    onSearchInput={handleSearchInput}
    onSearchClear={clearSearch}
    {displayMode}
    onDisplayModeChange={handleDisplayModeChange}
    showGridMode={true}
    showListMode={true}
    showCardSizeSlider={true}
  >
    {#snippet actions()}
      {#if libraryBrowser.isActive}
        <button type="button" class="clear-button" onclick={() => libraryBrowser.clear()}>
          {m.library.clearFilters}
        </button>
      {/if}
      <button
        type="button"
        class="browser-toggle"
        class:active={libraryBrowser.isVisible}
        onclick={() => (libraryBrowser.isVisible = !libraryBrowser.isVisible)}
        aria-pressed={libraryBrowser.isVisible}
        title={m.library.columnBrowser}
        aria-label={m.library.columnBrowser}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-4 h-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M9 4v16m6-16v16M5 4h14a1 1 0 011 1v14a1 1 0 01-1 1H5a1 1 0 01-1-1V5a1 1 0 011-1z"
          />
        </svg>
      </button>
      <TrackFilterMenu
        filters={libraryBrowser.filters}
        onChange={(filters) => (libraryBrowser.filters = filters)}
      />
    {/snippet}
  </LibraryHeader>

  <!-- カラムブラウザ（ジャンル / アーティスト / アルバム） -->
  {#if browsed}
    <ColumnBrowser result={browsed} onSelect={handleBrowserSelect} />
  {/if}

  <!-- トラックリスト -->
  <div class="track-list-container">
    <TrackList
      {tracks}
      {isLoading}
      {isError}
      {error}
      emptyMessage={isNarrowed ? m.library.noMatches : undefined}
      emptyHint={isNarrowed ? m.library.noMatchesHint : undefined}
      {displayMode}
      viewId="songs"
      isRevealFallback={!isNarrowed}
    />
  </div>
</div>

<style>
  @reference "../../../../app.css";
  .songs-page {
    @apply flex flex-col h-full;
  }

  .track-list-container {
    @apply flex-1 min-h-0 overflow-hidden px-4;
  }

  /* カラムブラウザを出す・隠すボタン */
  .browser-toggle {
    @apply flex items-center justify-center w-9 h-9 bg-base-400 border border-border rounded-md text-text-muted cursor-pointer transition-colors;
  }

  .browser-toggle:hover {
    @apply text-text-primary;
  }

  .browser-toggle.active {
    @apply border-primary text-primary;
  }

  .clear-button {
    @apply h-9 px-3 bg-transparent border-none text-sm text-primary cursor-pointer whitespace-nowrap hover:underline;
  }
</style>
