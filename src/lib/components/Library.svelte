<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {
    useTracksQuery,
    useSearchQuery,
    useFilterQuery,
    useUniqueArtistsQuery,
    useUniqueAlbumsQuery,
    useUniqueGenresQuery,
    type FilterOptions
  } from '$lib/queries/tracks';
  import MetadataEditor from './MetadataEditor.svelte';
  import { sanitizeSearchQuery } from '$lib/utils/validation';
  import { playTrackFromQueue } from '$lib/stores/player';
  import type { Track, AlbumArt } from '$lib/types/models';

  type ViewMode = 'grid' | 'list';

  let viewMode = $state<ViewMode>('grid');
  let searchTerm = $state('');
  let debouncedSearchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // アルバムアートサイズ設定 (50 - 200px)
  let artSize = $state(120);
  const MIN_ART_SIZE = 50;
  const MAX_ART_SIZE = 200;

  // アルバムアートキャッシュ
  let albumArtCache = $state<Map<string, string | null>>(new Map());

  // フィルター状態
  let selectedArtist = $state<string>('');
  let selectedAlbum = $state<string>('');
  let selectedGenre = $state<string>('');
  let showFilters = $state(false);

  // トラック選択状態
  let selectedTrackIds = $state<Set<string>>(new Set());
  let showMetadataEditor = $state(false);

  const tracksQuery = useTracksQuery();
  const artistsQuery = useUniqueArtistsQuery();
  const albumsQuery = useUniqueAlbumsQuery();
  const genresQuery = useUniqueGenresQuery();

  // 検索中かどうかを判定
  const isSearching = $derived(debouncedSearchTerm.length > 0);

  // フィルタリング中かどうかを判定
  const isFiltering = $derived(!!(selectedArtist || selectedAlbum || selectedGenre));

  // フィルターオプション
  const filterOptions = $derived<FilterOptions>({
    artist: selectedArtist || undefined,
    album: selectedAlbum || undefined,
    genre: selectedGenre || undefined
  });

  // 検索クエリを動的に作成
  const searchQuery = $derived.by(() => {
    if (isSearching) {
      return useSearchQuery(debouncedSearchTerm);
    }
    return null;
  });

  // フィルタークエリを動的に作成
  const filterQuery = $derived.by(() => {
    if (isFiltering && !isSearching) {
      return useFilterQuery(filterOptions);
    }
    return null;
  });

  // 表示するクエリを選択（検索 > フィルター > 全トラック）
  const activeQuery = $derived(searchQuery || filterQuery || tracksQuery);

  // クエリ結果を取得
  const isLoading = $derived(activeQuery.isLoading);
  const isError = $derived(activeQuery.isError);
  const error = $derived(activeQuery.error);
  const tracks = $derived(activeQuery.data);

  /**
   * 検索入力のデバウンス処理
   */
  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchTerm = target.value;

    // 既存のタイマーをクリア
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    // 300ms後に検索を実行（サニタイズ済み）
    debounceTimer = setTimeout(() => {
      debouncedSearchTerm = sanitizeSearchQuery(searchTerm);
    }, 300);
  }

  /**
   * 検索をクリア
   */
  function clearSearch() {
    searchTerm = '';
    debouncedSearchTerm = '';
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  }

  /**
   * テキスト内の検索語をハイライト
   */
  function highlightText(text: string, search: string): string {
    if (!search || !text) return text;

    const regex = new RegExp(`(${search})`, 'gi');
    return text.replace(regex, '<mark>$1</mark>');
  }

  /**
   * 再生時間をフォーマット (秒 -> mm:ss)
   */
  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  /**
   * 表示モードを切り替え
   */
  function toggleViewMode() {
    viewMode = viewMode === 'grid' ? 'list' : 'grid';
  }

  /**
   * フィルター表示を切り替え
   */
  function toggleFilters() {
    showFilters = !showFilters;
  }

  /**
   * すべてのフィルターをクリア
   */
  function clearFilters() {
    selectedArtist = '';
    selectedAlbum = '';
    selectedGenre = '';
  }

  /**
   * フィルターが適用されているかチェック
   */
  const hasActiveFilters = $derived(!!(selectedArtist || selectedAlbum || selectedGenre));

  /**
   * 選択されたトラックを取得
   */
  const selectedTracks = $derived.by(() => {
    if (!tracks) return [];
    return tracks.filter((track) => selectedTrackIds.has(track.id));
  });

  /**
   * トラックの選択を切り替え
   */
  function toggleTrackSelection(trackId: string, event: MouseEvent | KeyboardEvent) {
    event.stopPropagation();

    // キーボードイベントの場合は通常の選択のみ
    if (event instanceof KeyboardEvent) {
      const newSelection = new Set(selectedTrackIds);
      if (newSelection.has(trackId)) {
        newSelection.clear();
      } else {
        newSelection.clear();
        newSelection.add(trackId);
      }
      selectedTrackIds = newSelection;
      return;
    }

    const newSelection = new Set(selectedTrackIds);

    if (event.shiftKey && selectedTrackIds.size > 0 && tracks) {
      // Shift+クリックで範囲選択
      const lastSelectedId = Array.from(selectedTrackIds).pop();
      const lastIndex = tracks.findIndex((t) => t.id === lastSelectedId);
      const currentIndex = tracks.findIndex((t) => t.id === trackId);

      if (lastIndex !== -1 && currentIndex !== -1) {
        const start = Math.min(lastIndex, currentIndex);
        const end = Math.max(lastIndex, currentIndex);

        for (let i = start; i <= end; i++) {
          newSelection.add(tracks[i].id);
        }
      }
    } else if (event.ctrlKey || event.metaKey) {
      // Ctrl/Cmd+クリックで複数選択
      if (newSelection.has(trackId)) {
        newSelection.delete(trackId);
      } else {
        newSelection.add(trackId);
      }
    } else {
      // 通常のクリックで単一選択
      if (newSelection.has(trackId) && newSelection.size === 1) {
        newSelection.clear();
      } else {
        newSelection.clear();
        newSelection.add(trackId);
      }
    }

    selectedTrackIds = newSelection;
  }

  /**
   * すべての選択を解除
   */
  function clearSelection() {
    selectedTrackIds = new Set();
  }

  /**
   * メタデータエディタを開く
   */
  function openMetadataEditor() {
    if (selectedTrackIds.size > 0) {
      showMetadataEditor = true;
    }
  }

  /**
   * メタデータエディタを閉じる
   */
  function closeMetadataEditor() {
    showMetadataEditor = false;
  }

  /**
   * メタデータ保存後の処理
   */
  function handleMetadataSaved() {
    clearSelection();
  }

  /**
   * トラックをダブルクリックで再生
   */
  function handleTrackDoubleClick(track: Track) {
    if (!tracks) return;

    // 現在のトラックリストから再生キューを作成
    const trackIndex = tracks.findIndex((t) => t.id === track.id);
    if (trackIndex !== -1) {
      playTrackFromQueue(tracks, trackIndex);
    }
  }

  /**
   * アルバムアートを取得
   */
  async function loadAlbumArt(trackId: string): Promise<void> {
    // 既にキャッシュにある場合はスキップ
    if (albumArtCache.has(trackId)) {
      return;
    }

    // ローディング中のプレースホルダーを設定
    albumArtCache.set(trackId, null);

    try {
      const art = await invoke<AlbumArt | null>('get_album_art', { trackId });
      if (art) {
        const dataUrl = `data:${art.mimeType};base64,${art.data}`;
        albumArtCache = new Map(albumArtCache.set(trackId, dataUrl));
      } else {
        albumArtCache = new Map(albumArtCache.set(trackId, null));
      }
    } catch {
      albumArtCache = new Map(albumArtCache.set(trackId, null));
    }
  }

  /**
   * アルバムアートのData URLを取得
   */
  function getAlbumArtUrl(trackId: string): string | null {
    return albumArtCache.get(trackId) ?? null;
  }

  /**
   * グリッドカードサイズを計算（アートサイズ + パディング）
   */
  const cardWidth = $derived(artSize + 24); // 12px padding on each side

  // トラックが変更されたときにアルバムアートを読み込む
  $effect(() => {
    if (tracks && viewMode === 'grid') {
      // 表示されているトラックのアルバムアートを順次読み込む
      for (const track of tracks.slice(0, 50)) {
        // 最初の50件のみ
        if (!albumArtCache.has(track.id)) {
          loadAlbumArt(track.id);
        }
      }
    }
  });
</script>

<div class="library-container">
  <!-- ヘッダー -->
  <div class="library-header">
    <h2>音楽ライブラリ</h2>
    <div class="header-controls">
      {#if selectedTrackIds.size > 0}
        <div class="selection-info">
          {selectedTrackIds.size}件選択中
          <button onclick={clearSelection} class="clear-selection-button">選択解除</button>
          <button onclick={openMetadataEditor} class="edit-metadata-button">
            メタデータを編集
          </button>
        </div>
      {/if}
      <div class="search-box">
        <input
          type="text"
          placeholder="タイトル、アーティスト、アルバム、ジャンルで検索..."
          value={searchTerm}
          oninput={handleSearchInput}
          class="search-input"
        />
        {#if searchTerm}
          <button onclick={clearSearch} class="clear-button" aria-label="検索をクリア"> ✕ </button>
        {/if}
      </div>
      <button onclick={toggleFilters} class="filter-toggle" class:active={showFilters}>
        🔍 フィルター
        {#if hasActiveFilters}
          ({[selectedArtist, selectedAlbum, selectedGenre].filter(Boolean).length})
        {/if}
      </button>
      <button onclick={toggleViewMode} class="view-toggle">
        {viewMode === 'grid' ? 'リスト表示' : 'グリッド表示'}
      </button>
      {#if viewMode === 'grid'}
        <div class="size-slider">
          <span class="size-label">サイズ:</span>
          <input
            type="range"
            min={MIN_ART_SIZE}
            max={MAX_ART_SIZE}
            bind:value={artSize}
            class="slider"
            aria-label="カードサイズ"
          />
          <span class="size-value">{artSize}px</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- フィルターパネル -->
  {#if showFilters}
    <div class="filter-panel">
      <div class="filter-group">
        <label for="artist-filter">アーティスト</label>
        <select id="artist-filter" bind:value={selectedArtist} class="filter-select">
          <option value="">すべて</option>
          {#if artistsQuery.data}
            {#each artistsQuery.data as artist}
              <option value={artist}>{artist}</option>
            {/each}
          {/if}
        </select>
      </div>

      <div class="filter-group">
        <label for="album-filter">アルバム</label>
        <select id="album-filter" bind:value={selectedAlbum} class="filter-select">
          <option value="">すべて</option>
          {#if albumsQuery.data}
            {#each albumsQuery.data as album}
              <option value={album}>{album}</option>
            {/each}
          {/if}
        </select>
      </div>

      <div class="filter-group">
        <label for="genre-filter">ジャンル</label>
        <select id="genre-filter" bind:value={selectedGenre} class="filter-select">
          <option value="">すべて</option>
          {#if genresQuery.data}
            {#each genresQuery.data as genre}
              <option value={genre}>{genre}</option>
            {/each}
          {/if}
        </select>
      </div>

      {#if hasActiveFilters}
        <button onclick={clearFilters} class="clear-filters-button">すべてクリア</button>
      {/if}
    </div>
  {/if}

  <!-- コンテンツ -->
  <div class="library-content">
    {#if isLoading}
      <div class="loading">読み込み中...</div>
    {:else if isError}
      <div class="error">
        エラーが発生しました: {error?.message || '不明なエラー'}
      </div>
    {:else if tracks && tracks.length > 0}
      <!-- トラック表示 -->
      <div
        class="tracks-{viewMode}"
        style={viewMode === 'grid' ? `--card-width: ${cardWidth}px; --art-size: ${artSize}px;` : ''}
      >
        {#each tracks as track (track.id)}
          <div class="track-item">
            {#if viewMode === 'grid'}
              <!-- グリッド表示 -->
              <div
                class="track-card"
                class:selected={selectedTrackIds.has(track.id)}
                draggable="true"
                ondragstart={(e) => {
                  if (e.dataTransfer) {
                    e.dataTransfer.effectAllowed = 'copy';
                    e.dataTransfer.setData('text/plain', track.id);
                  }
                }}
                onclick={(e) => toggleTrackSelection(track.id, e)}
                ondblclick={() => handleTrackDoubleClick(track)}
                onkeydown={(e) => e.key === 'Enter' && toggleTrackSelection(track.id, e)}
                role="button"
                tabindex="0"
              >
                <div class="album-art" style="width: {artSize}px; height: {artSize}px;">
                  {#if getAlbumArtUrl(track.id)}
                    <img
                      src={getAlbumArtUrl(track.id)}
                      alt="アルバムアート"
                      class="album-art-image"
                      loading="lazy"
                    />
                  {:else}
                    <div class="album-art-placeholder">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="music-icon">
                        <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                      </svg>
                    </div>
                  {/if}
                </div>
                <div class="track-info">
                  <div class="track-title" title={track.title || track.fileName}>
                    {@html highlightText(track.title || track.fileName, debouncedSearchTerm)}
                  </div>
                  <div class="track-artist" title={track.artist || '不明なアーティスト'}>
                    {@html highlightText(track.artist || '不明なアーティスト', debouncedSearchTerm)}
                  </div>
                  {#if artSize >= 100}
                    <div class="track-album" title={track.album || '不明なアルバム'}>
                      {@html highlightText(track.album || '不明なアルバム', debouncedSearchTerm)}
                    </div>
                  {/if}
                </div>
              </div>
            {:else}
              <!-- リスト表示 -->
              <div
                class="track-row"
                class:selected={selectedTrackIds.has(track.id)}
                draggable="true"
                ondragstart={(e) => {
                  if (e.dataTransfer) {
                    e.dataTransfer.effectAllowed = 'copy';
                    e.dataTransfer.setData('text/plain', track.id);
                  }
                }}
                onclick={(e) => toggleTrackSelection(track.id, e)}
                ondblclick={() => handleTrackDoubleClick(track)}
                onkeydown={(e) => e.key === 'Enter' && toggleTrackSelection(track.id, e)}
                role="button"
                tabindex="0"
              >
                <div class="track-col track-col-title">
                  {@html highlightText(track.title || track.fileName, debouncedSearchTerm)}
                </div>
                <div class="track-col track-col-artist">
                  {@html highlightText(track.artist || '不明なアーティスト', debouncedSearchTerm)}
                </div>
                <div class="track-col track-col-album">
                  {@html highlightText(track.album || '不明なアルバム', debouncedSearchTerm)}
                </div>
                <div class="track-col track-col-duration">
                  {formatDuration(track.duration)}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {:else if isSearching}
      <div class="empty">
        <p>結果が見つかりません</p>
        <p>別の検索語を試してください</p>
      </div>
    {:else}
      <div class="empty">
        <p>音楽ライブラリが空です</p>
        <p>フォルダをインポートして音楽を追加してください</p>
      </div>
    {/if}
  </div>
</div>

<!-- メタデータエディタ -->
{#if showMetadataEditor}
  <MetadataEditor
    tracks={selectedTracks}
    onClose={closeMetadataEditor}
    onSave={handleMetadataSaved}
  />
{/if}

<style>
  .library-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1rem;
  }

  .library-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid #e0e0e0;
  }

  .library-header h2 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 600;
  }

  .header-controls {
    display: flex;
    gap: 1rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .selection-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background-color: #e3f2fd;
    border-radius: 4px;
    font-size: 0.9rem;
    color: #1976d2;
  }

  .clear-selection-button {
    padding: 0.25rem 0.75rem;
    background-color: #fff;
    color: #1976d2;
    border: 1px solid #1976d2;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.85rem;
    transition: background-color 0.2s;
  }

  .clear-selection-button:hover {
    background-color: #f5f5f5;
  }

  .edit-metadata-button {
    padding: 0.25rem 0.75rem;
    background-color: #1976d2;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.85rem;
    transition: background-color 0.2s;
  }

  .edit-metadata-button:hover {
    background-color: #1565c0;
  }

  .search-box {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-input {
    padding: 0.5rem 2.5rem 0.5rem 1rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 0.9rem;
    width: 300px;
    transition: border-color 0.2s;
  }

  .search-input:focus {
    outline: none;
    border-color: #007bff;
  }

  .clear-button {
    position: absolute;
    right: 0.5rem;
    background: none;
    border: none;
    color: #666;
    cursor: pointer;
    font-size: 1.2rem;
    padding: 0.25rem;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    transition: background-color 0.2s;
  }

  .clear-button:hover {
    background-color: #e0e0e0;
  }

  .filter-toggle {
    padding: 0.5rem 1rem;
    background-color: #6c757d;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9rem;
    white-space: nowrap;
    transition: background-color 0.2s;
  }

  .filter-toggle:hover {
    background-color: #5a6268;
  }

  .filter-toggle.active {
    background-color: #28a745;
  }

  .filter-toggle.active:hover {
    background-color: #218838;
  }

  .view-toggle {
    padding: 0.5rem 1rem;
    background-color: #007bff;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9rem;
    white-space: nowrap;
  }

  .view-toggle:hover {
    background-color: #0056b3;
  }

  .size-slider {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .size-label {
    font-size: 0.85rem;
    color: #666;
  }

  .slider {
    width: 100px;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: #ddd;
    border-radius: 2px;
    outline: none;
  }

  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    background: #007bff;
    border-radius: 50%;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .slider::-webkit-slider-thumb:hover {
    background: #0056b3;
  }

  .slider::-moz-range-thumb {
    width: 14px;
    height: 14px;
    background: #007bff;
    border-radius: 50%;
    cursor: pointer;
    border: none;
    transition: background-color 0.2s;
  }

  .slider::-moz-range-thumb:hover {
    background: #0056b3;
  }

  .size-value {
    font-size: 0.85rem;
    color: #666;
    min-width: 45px;
  }

  .filter-panel {
    display: flex;
    gap: 1rem;
    padding: 1rem;
    background-color: #f8f9fa;
    border-radius: 4px;
    margin-bottom: 1rem;
    align-items: flex-end;
  }

  .filter-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
  }

  .filter-group label {
    font-size: 0.85rem;
    font-weight: 500;
    color: #495057;
  }

  .filter-select {
    padding: 0.5rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 0.9rem;
    background-color: white;
    cursor: pointer;
  }

  .filter-select:focus {
    outline: none;
    border-color: #007bff;
  }

  .clear-filters-button {
    padding: 0.5rem 1rem;
    background-color: #dc3545;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9rem;
    white-space: nowrap;
  }

  .clear-filters-button:hover {
    background-color: #c82333;
  }

  :global(mark) {
    background-color: #ffeb3b;
    color: #000;
    padding: 0 2px;
    border-radius: 2px;
  }

  .library-content {
    flex: 1;
    overflow-y: auto;
  }

  .loading,
  .error,
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
  }

  .error {
    color: #dc3545;
  }

  .empty p {
    margin: 0.5rem 0;
  }

  /* グリッド表示 */
  .tracks-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width, 200px), 1fr));
    gap: 1rem;
    justify-items: center;
  }

  .track-card {
    width: var(--card-width, 200px);
    padding: 0.75rem;
    background-color: #f8f9fa;
    border-radius: 8px;
    cursor: pointer;
    transition:
      transform 0.2s,
      box-shadow 0.2s,
      background-color 0.2s;
    border: 2px solid transparent;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .track-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
  }

  .track-card.selected {
    background-color: #e3f2fd;
    border-color: #1976d2;
  }

  .album-art {
    flex-shrink: 0;
    border-radius: 4px;
    overflow: hidden;
    background-color: #e9ecef;
    margin-bottom: 0.5rem;
  }

  .album-art-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .album-art-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: rgba(255, 255, 255, 0.8);
  }

  .music-icon {
    width: 40%;
    height: 40%;
  }

  .track-info {
    width: 100%;
    text-align: center;
    min-width: 0;
  }

  .track-title {
    font-weight: 600;
    margin-bottom: 0.25rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.9rem;
  }

  .track-artist,
  .track-album {
    font-size: 0.8rem;
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* リスト表示 */
  .tracks-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .track-row {
    display: grid;
    grid-template-columns: 2fr 1.5fr 1.5fr 100px;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background-color: #f8f9fa;
    border-radius: 4px;
    cursor: pointer;
    transition: background-color 0.2s;
    border: 2px solid transparent;
  }

  .track-row:hover {
    background-color: #e9ecef;
  }

  .track-row.selected {
    background-color: #e3f2fd;
    border-color: #1976d2;
  }

  .track-col {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-col-title {
    font-weight: 600;
  }

  .track-col-artist,
  .track-col-album {
    color: #666;
  }

  .track-col-duration {
    text-align: right;
    color: #666;
  }

  @media (prefers-color-scheme: dark) {
    .library-header {
      border-bottom-color: #444;
    }

    .selection-info {
      background-color: #1a237e;
      color: #90caf9;
    }

    .clear-selection-button {
      background-color: #2a2a2a;
      color: #90caf9;
      border-color: #90caf9;
    }

    .clear-selection-button:hover {
      background-color: #333;
    }

    .edit-metadata-button {
      background-color: #1565c0;
    }

    .edit-metadata-button:hover {
      background-color: #0d47a1;
    }

    .search-input {
      background-color: #2a2a2a;
      border-color: #555;
      color: #fff;
    }

    .search-input:focus {
      border-color: #007bff;
    }

    .clear-button {
      color: #aaa;
    }

    .clear-button:hover {
      background-color: #444;
    }

    .filter-panel {
      background-color: #2a2a2a;
    }

    .filter-group label {
      color: #ccc;
    }

    .filter-select {
      background-color: #333;
      border-color: #555;
      color: #fff;
    }

    .filter-select:focus {
      border-color: #007bff;
    }

    .track-card,
    .track-row {
      background-color: #2a2a2a;
    }

    .track-card:hover {
      box-shadow: 0 4px 8px rgba(255, 255, 255, 0.1);
    }

    .track-row:hover {
      background-color: #333;
    }

    .track-card.selected,
    .track-row.selected {
      background-color: #1a237e;
      border-color: #90caf9;
    }

    .track-artist,
    .track-album,
    .track-col-artist,
    .track-col-album,
    .track-col-duration {
      color: #aaa;
    }

    .size-label,
    .size-value {
      color: #aaa;
    }

    .slider {
      background: #444;
    }

    .album-art {
      background-color: #333;
    }

    .album-art-placeholder {
      background: linear-gradient(135deg, #4a5568 0%, #2d3748 100%);
    }

    :global(mark) {
      background-color: #ffc107;
      color: #000;
    }
  }
</style>
