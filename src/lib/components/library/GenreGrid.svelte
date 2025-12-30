<script lang="ts">
  import type { GenreGroup } from '$lib/types/models';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { browseSearchQuery } from '$lib/stores/ui';
  import GroupDetail from './GroupDetail.svelte';
  import GroupContextMenu from '../GroupContextMenu.svelte';

  // 検索状態
  let searchInput = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // 検索入力のデバウンス処理
  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchInput = target.value;

    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
      browseSearchQuery.set(searchInput);
    }, 300);
  }

  // 検索クリア
  function clearSearch() {
    searchInput = '';
    browseSearchQuery.set('');
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  }

  // クエリ
  const genresQuery = useGenresGroupedQuery();
  const isLoading = $derived(genresQuery.isLoading);
  const isError = $derived(genresQuery.isError);
  const allGenres = $derived(genresQuery.data ?? []);

  // 検索でフィルタリングされたジャンル
  const genres = $derived.by(() => {
    const query = $browseSearchQuery.toLowerCase().trim();
    if (!query) return allGenres;
    return allGenres.filter(genre =>
      genre.name.toLowerCase().includes(query)
    );
  });

  // 選択中のジャンル（モーダル表示用）
  let selectedGenre = $state<GenreGroup | null>(null);

  // コンテキストメニュー
  let contextMenu = $state<{ x: number; y: number; genre: GenreGroup } | null>(null);

  // ジャンルごとの色を生成
  const genreColors = [
    { bg: 'linear-gradient(135deg, #e91e63, #9c27b0)' },
    { bg: 'linear-gradient(135deg, #2196f3, #00bcd4)' },
    { bg: 'linear-gradient(135deg, #4caf50, #8bc34a)' },
    { bg: 'linear-gradient(135deg, #ff9800, #ff5722)' },
    { bg: 'linear-gradient(135deg, #9c27b0, #673ab7)' },
    { bg: 'linear-gradient(135deg, #00bcd4, #009688)' },
    { bg: 'linear-gradient(135deg, #f44336, #e91e63)' },
    { bg: 'linear-gradient(135deg, #3f51b5, #2196f3)' },
    { bg: 'linear-gradient(135deg, #ff5722, #ffc107)' },
    { bg: 'linear-gradient(135deg, #795548, #607d8b)' }
  ];

  function getGenreColor(index: number) {
    return genreColors[index % genreColors.length];
  }

  // ジャンルをクリック
  function handleGenreClick(genre: GenreGroup) {
    selectedGenre = genre;
  }

  // ジャンルをダブルクリック（すべて再生）
  function handleGenreDoubleClick(genre: GenreGroup) {
    if (genre.tracks.length > 0) {
      playTrackFromQueue(genre.tracks, 0);
    }
  }

  // 再生ボタンクリック
  function handlePlayClick(event: MouseEvent, genre: GenreGroup) {
    event.stopPropagation();
    handleGenreDoubleClick(genre);
  }

  // モーダルを閉じる
  function handleCloseDetail() {
    selectedGenre = null;
  }

  // 右クリックメニューを表示
  function handleContextMenu(event: MouseEvent, genre: GenreGroup) {
    event.preventDefault();
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      genre
    };
  }

  // 右クリックメニューを閉じる
  function closeContextMenu() {
    contextMenu = null;
  }
</script>

<div class="p-4 min-h-[200px]">
  <!-- 検索バー -->
  <div class="browse-search-bar">
    <svg xmlns="http://www.w3.org/2000/svg" class="search-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
    </svg>
    <input
      type="text"
      placeholder="ジャンルを検索..."
      value={searchInput}
      oninput={handleSearchInput}
      class="browse-search-input"
    />
    {#if searchInput}
      <button onclick={clearSearch} class="search-clear-btn" aria-label="検索をクリア">✕</button>
    {/if}
  </div>

  {#if isLoading}
    <div class="state-container">
      <div class="spinner"></div>
      <p>ジャンルを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="state-container">
      <p class="text-error-light">ジャンルの読み込みに失敗しました</p>
    </div>
  {:else if allGenres.length > 0 && genres.length === 0}
    <div class="state-container">
      <svg xmlns="http://www.w3.org/2000/svg" class="w-12 h-12 text-text-dimmed/50 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <p>「{$browseSearchQuery}」に一致するジャンルが見つかりません</p>
    </div>
  {:else if genres.length > 0}
    <div class="genre-grid">
      {#each genres as genre, index (genre.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="genre-card"
          style="background: {getGenreColor(index).bg}"
          onclick={() => handleGenreClick(genre)}
          ondblclick={() => handleGenreDoubleClick(genre)}
          oncontextmenu={(e) => handleContextMenu(e, genre)}
        >
          <div class="genre-content">
            <h3 class="genre-name">{genre.name}</h3>
            <p class="genre-meta">{genre.trackCount}曲</p>
          </div>
          <div class="genre-play-overlay">
            <button class="genre-play-button" onclick={(e) => handlePlayClick(e, genre)} title="ジャンルを再生">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="state-container">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
        <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
      </svg>
      <p>ジャンルがありません</p>
      <span>音楽をインポートしてジャンルを追加してください</span>
    </div>
  {/if}
</div>

<!-- 詳細モーダル -->
<GroupDetail
  group={selectedGenre}
  type="genre"
  onClose={handleCloseDetail}
/>

<!-- コンテキストメニュー -->
{#if contextMenu}
  <GroupContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    group={contextMenu.genre}
    type="genre"
    onClose={closeContextMenu}
  />
{/if}

<style>
@reference "../../../app.css";
  .browse-search-bar {
    @apply relative flex items-center mb-4 max-w-md;
  }

  .search-icon {
    @apply absolute left-3 w-4 h-4 text-text-dimmed;
  }

  .browse-search-input {
    @apply w-full py-2 pl-9 pr-8 bg-base-400 border border-border rounded-md text-sm text-text-primary transition-all duration-200;
  }

  .browse-search-input:focus {
    @apply outline-none border-primary;
  }

  .browse-search-input::placeholder {
    @apply text-text-dimmed;
  }

  .search-clear-btn {
    @apply absolute right-2 p-1 text-text-dimmed hover:text-text-primary bg-transparent border-none cursor-pointer;
  }

  .genre-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 1rem;
  }

  .genre-card {
    @apply relative rounded-lg p-6 cursor-pointer transition-all duration-200 min-h-[120px] flex items-end overflow-hidden;
  }

  .genre-card::before {
    content: '';
    @apply absolute inset-0 bg-black/20 opacity-0 transition-opacity duration-200;
  }

  .genre-card:hover::before {
    @apply opacity-100;
  }

  .genre-card:hover {
    @apply -translate-y-0.5;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }

  .genre-content {
    @apply relative z-10;
  }

  .genre-name {
    @apply text-xl font-bold text-white m-0;
    text-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
    word-break: break-word;
  }

  .genre-meta {
    @apply text-sm text-white/80 mt-1.5 m-0;
  }

  .genre-play-overlay {
    @apply absolute top-3 right-3 opacity-0 transition-opacity duration-200 z-20;
  }

  .genre-card:hover .genre-play-overlay {
    @apply opacity-100;
  }

  .genre-play-button {
    @apply w-10 h-10 border-none rounded-full flex items-center justify-center cursor-pointer transition-transform duration-150;
    background: rgba(255, 255, 255, 0.9);
    color: #000;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }

  .genre-play-button:hover {
    @apply scale-110 bg-white;
  }

  .genre-play-button svg {
    @apply w-5 h-5 ml-0.5;
  }
</style>
