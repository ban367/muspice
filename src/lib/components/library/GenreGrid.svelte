<script lang="ts">
  import type { GenreGroup } from '$lib/types/models';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';
  import { playTrackFromQueue } from '$lib/stores/player';
  import GroupDetail from './GroupDetail.svelte';

  // クエリ
  const genresQuery = useGenresGroupedQuery();
  const isLoading = $derived(genresQuery.isLoading);
  const isError = $derived(genresQuery.isError);
  const genres = $derived(genresQuery.data ?? []);

  // 選択中のジャンル（モーダル表示用）
  let selectedGenre = $state<GenreGroup | null>(null);

  // ジャンルごとの色を生成
  const genreColors = [
    { bg: 'linear-gradient(135deg, #e91e63, #9c27b0)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #2196f3, #00bcd4)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #4caf50, #8bc34a)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #ff9800, #ff5722)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #9c27b0, #673ab7)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #00bcd4, #009688)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #f44336, #e91e63)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #3f51b5, #2196f3)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #ff5722, #ffc107)', text: '#fff' },
    { bg: 'linear-gradient(135deg, #795548, #607d8b)', text: '#fff' }
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
</script>

<div class="genre-grid-container">
  {#if isLoading}
    <div class="loading-state">
      <div class="spinner"></div>
      <p>ジャンルを読み込み中...</p>
    </div>
  {:else if isError}
    <div class="error-state">
      <p>ジャンルの読み込みに失敗しました</p>
    </div>
  {:else if genres.length > 0}
    <div class="genre-grid">
      {#each genres as genre, index (genre.name)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="genre-card"
          style="background: {getGenreColor(index).bg}"
          onclick={() => handleGenreClick(genre)}
          ondblclick={() => handleGenreDoubleClick(genre)}
        >
          <div class="genre-content">
            <h3 class="genre-name">{genre.name}</h3>
            <p class="genre-meta">{genre.trackCount}曲</p>
          </div>
          <div class="play-overlay">
            <button class="play-button" onclick={(e) => handlePlayClick(e, genre)} title="ジャンルを再生">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty-state">
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

<style>
  .genre-grid-container {
    padding: 1rem;
    min-height: 200px;
  }

  .genre-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 1rem;
  }

  .genre-card {
    position: relative;
    border-radius: 0.5rem;
    padding: 1.5rem;
    cursor: pointer;
    transition: all 0.2s ease;
    min-height: 120px;
    display: flex;
    align-items: flex-end;
    overflow: hidden;
  }

  .genre-card::before {
    content: '';
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.2);
    opacity: 0;
    transition: opacity 0.2s ease;
  }

  .genre-card:hover::before {
    opacity: 1;
  }

  .genre-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }

  .genre-content {
    position: relative;
    z-index: 1;
  }

  .genre-name {
    font-size: 1.25rem;
    font-weight: 700;
    color: #fff;
    margin: 0;
    text-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
    word-break: break-word;
  }

  .genre-meta {
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.8);
    margin: 0.375rem 0 0;
  }

  .play-overlay {
    position: absolute;
    top: 0.75rem;
    right: 0.75rem;
    opacity: 0;
    transition: opacity 0.2s ease;
    z-index: 2;
  }

  .genre-card:hover .play-overlay {
    opacity: 1;
  }

  .play-button {
    width: 40px;
    height: 40px;
    border: none;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.9);
    color: #000;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: transform 0.15s ease;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }

  .play-button:hover {
    transform: scale(1.08);
    background: #fff;
  }

  .play-button svg {
    width: 20px;
    height: 20px;
    margin-left: 2px;
  }

  /* 状態表示 */
  .loading-state,
  .error-state,
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 4rem 2rem;
    color: #666;
    text-align: center;
  }

  .loading-state .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: #3b82f6;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    margin-bottom: 1rem;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .empty-state svg {
    width: 64px;
    height: 64px;
    color: #444;
    margin-bottom: 1rem;
  }

  .empty-state p {
    font-size: 1rem;
    color: #888;
    margin: 0;
  }

  .empty-state span {
    font-size: 0.875rem;
    color: #666;
    margin-top: 0.5rem;
  }
</style>
