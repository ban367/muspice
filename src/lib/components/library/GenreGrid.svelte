<script lang="ts">
  import type { GenreGroup } from '$lib/types/models';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import { browseSearchQuery, browseMode, selectedGenreName } from '$lib/stores/ui';
  import { goto } from '$app/navigation';
  import GroupContextMenu from '../GroupContextMenu.svelte';

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

  // ジャンルをクリック（詳細ページに遷移）
  function handleGenreClick(genre: GenreGroup) {
    selectedGenreName.set(genre.name);
    browseMode.set('genre-detail');
    browseSearchQuery.set('');
    goto('/');
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
