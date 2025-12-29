<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import {
    useTracksQuery,
    useSearchQuery,
    useFilterQuery,
    useUniqueArtistsQuery,
    useUniqueAlbumsQuery,
    useUniqueGenresQuery,
    useFavoriteTracksQuery,
    useMostPlayedTracksQuery,
    useRecentlyPlayedTracksQuery,
    toggleFavorite,
    setRating,
    type FilterOptions
  } from '$lib/queries/tracks';
  import { useQueryClient } from '@tanstack/svelte-query';
  import MetadataEditor from './MetadataEditor.svelte';
  import ContextMenu from './ContextMenu.svelte';
  import { sanitizeSearchQuery } from '$lib/utils/validation';
  import { playTrackFromQueue, currentTrack, playQueue, currentTrackIndex } from '$lib/stores/player';
  import { browseMode } from '$lib/stores/ui';
  import { get } from 'svelte/store';
  import type { Track, AlbumArt } from '$lib/types/models';
  import BrowseModeSelector from './library/BrowseModeSelector.svelte';
  import AlbumGrid from './library/AlbumGrid.svelte';
  import ArtistGrid from './library/ArtistGrid.svelte';
  import GenreGrid from './library/GenreGrid.svelte';

  // Props
  interface Props {
    viewMode?: 'all' | 'favorites' | 'recent' | 'mostplayed';
  }

  let { viewMode: viewModeParam = 'all' }: Props = $props();

  type DisplayMode = 'grid' | 'list';
  type SortField = 'title' | 'artist' | 'album' | 'duration' | 'createdAt';
  type SortDirection = 'asc' | 'desc';

  let displayMode = $state<DisplayMode>('list');
  let searchTerm = $state('');
  let debouncedSearchTerm = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // ソート状態
  let sortField = $state<SortField>('createdAt');
  let sortDirection = $state<SortDirection>('desc');

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

  // コンテキストメニュー状態
  let contextMenu = $state<{ x: number; y: number; track: Track } | null>(null);

  // ドラッグ状態
  let isDragging = $state(false);
  let draggedTrackIds = $state<string[]>([]);

  const queryClient = useQueryClient();
  const tracksQuery = useTracksQuery();
  const favoritesQuery = useFavoriteTracksQuery();
  const recentQuery = useRecentlyPlayedTracksQuery(50);
  const mostPlayedQuery = useMostPlayedTracksQuery(50);
  const artistsQuery = useUniqueArtistsQuery();
  const albumsQuery = useUniqueAlbumsQuery();
  const genresQuery = useUniqueGenresQuery();

  // ビューモードに応じたベースクエリを選択
  const baseQuery = $derived.by(() => {
    switch (viewModeParam) {
      case 'favorites':
        return favoritesQuery;
      case 'recent':
        return recentQuery;
      case 'mostplayed':
        return mostPlayedQuery;
      default:
        return tracksQuery;
    }
  });

  /**
   * お気に入りをトグル
   */
  async function handleToggleFavorite(trackId: string, event: MouseEvent) {
    event.stopPropagation();
    try {
      await toggleFavorite(trackId);
      // キャッシュを更新
      queryClient.invalidateQueries({ queryKey: ['tracks'] });
    } catch (error) {
      console.error('お気に入りの切り替えに失敗しました:', error);
    }
  }

  /**
   * レーティングを設定
   */
  async function handleSetRating(trackId: string, rating: number, event: MouseEvent) {
    event.stopPropagation();
    try {
      await setRating(trackId, rating);
      // キャッシュを更新
      queryClient.invalidateQueries({ queryKey: ['tracks'] });
    } catch (error) {
      console.error('レーティングの設定に失敗しました:', error);
    }
  }

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

  // 表示するクエリを選択（検索 > フィルター > ベースクエリ）
  // 注: お気に入りや最近再生などの特殊ビューでは検索/フィルターを無効化
  const activeQuery = $derived.by(() => {
    if (viewModeParam !== 'all') {
      return baseQuery;
    }
    return searchQuery || filterQuery || baseQuery;
  });

  // クエリ結果を取得
  const isLoading = $derived(activeQuery.isLoading);
  const isError = $derived(activeQuery.isError);
  const error = $derived(activeQuery.error);
  
  // ソートされたトラック
  const tracks = $derived.by(() => {
    const data = activeQuery.data;
    if (!data) return null;
    
    return [...data].sort((a, b) => {
      let aVal: string | number | null;
      let bVal: string | number | null;
      
      switch (sortField) {
        case 'title':
          aVal = a.title || a.fileName;
          bVal = b.title || b.fileName;
          break;
        case 'artist':
          aVal = a.artist || '';
          bVal = b.artist || '';
          break;
        case 'album':
          aVal = a.album || '';
          bVal = b.album || '';
          break;
        case 'duration':
          aVal = a.duration || 0;
          bVal = b.duration || 0;
          break;
        case 'createdAt':
          aVal = a.createdAt;
          bVal = b.createdAt;
          break;
        default:
          return 0;
      }
      
      if (aVal === null) aVal = '';
      if (bVal === null) bVal = '';
      
      let comparison = 0;
      if (typeof aVal === 'string' && typeof bVal === 'string') {
        comparison = aVal.localeCompare(bVal, 'ja');
      } else {
        comparison = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      }
      
      return sortDirection === 'asc' ? comparison : -comparison;
    });
  });

  // 現在再生中のトラックID
  const currentPlayingTrackId = $derived($currentTrack?.id);

  /**
   * ソートを切り替え
   */
  function toggleSort(field: SortField) {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = 'asc';
    }
  }

  /**
   * ソートアイコンを取得
   */
  function getSortIcon(field: SortField): string {
    if (sortField !== field) return '';
    return sortDirection === 'asc' ? '↑' : '↓';
  }

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
  function toggleDisplayMode() {
    displayMode = displayMode === 'grid' ? 'list' : 'grid';
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
   * 右クリックでコンテキストメニューを表示
   */
  function handleContextMenu(event: MouseEvent, track: Track) {
    event.preventDefault();
    
    // トラックが選択されていない場合は、右クリックしたトラックを選択
    if (!selectedTrackIds.has(track.id)) {
      selectedTrackIds = new Set([track.id]);
    }
    
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      track
    };
  }

  /**
   * コンテキストメニューを閉じる
   */
  function closeContextMenu() {
    contextMenu = null;
  }

  /**
   * 次に再生
   */
  function handlePlayNext() {
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);
    
    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...selectedTracks);
    playQueue.set(newQueue);
  }

  /**
   * キューに追加
   */
  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...selectedTracks]);
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

  /**
   * ドラッグ開始時のハンドラー
   */
  function handleDragStart(event: DragEvent, track: Track) {
    if (!event.dataTransfer) return;
    
    event.dataTransfer.effectAllowed = 'copy';
    
    // 選択されているトラックがある場合は複数のトラックIDを設定
    let trackIds: string[];
    if (selectedTrackIds.size > 0 && selectedTrackIds.has(track.id)) {
      trackIds = Array.from(selectedTrackIds);
    } else {
      trackIds = [track.id];
    }
    
    event.dataTransfer.setData('application/json', JSON.stringify(trackIds));
    event.dataTransfer.setData('text/plain', trackIds[0]);
    
    // ドラッグ状態を設定
    isDragging = true;
    draggedTrackIds = trackIds;
    
    // カスタムドラッグ画像を作成
    const dragImage = document.createElement('div');
    dragImage.className = 'drag-preview';
    dragImage.innerHTML = `
      <div class="drag-preview-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
        </svg>
      </div>
      <span class="drag-preview-count">${trackIds.length}曲</span>
    `;
    dragImage.style.position = 'absolute';
    dragImage.style.left = '-9999px';
    dragImage.style.top = '-9999px';
    document.body.appendChild(dragImage);
    
    event.dataTransfer.setDragImage(dragImage, 40, 25);
    
    // クリーンアップ
    setTimeout(() => {
      document.body.removeChild(dragImage);
    }, 0);
  }

  /**
   * ドラッグ終了時のハンドラー
   */
  function handleDragEnd() {
    isDragging = false;
    draggedTrackIds = [];
  }

  // トラックが変更されたときにアルバムアートを読み込む
  $effect(() => {
    if (tracks && displayMode === 'grid') {
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
    <div class="header-left">
      {#if viewModeParam === 'all'}
        <BrowseModeSelector />
      {/if}
      {#if $browseMode === 'songs' && tracks}
        <span class="track-count">{tracks.length}曲</span>
      {/if}
    </div>
    <div class="header-controls">
      {#if selectedTrackIds.size > 0}
        <div class="selection-info">
          {selectedTrackIds.size}件選択中
          <button onclick={clearSelection} class="btn-text">選択解除</button>
          <button onclick={openMetadataEditor} class="btn-primary-sm">
            メタデータを編集
          </button>
        </div>
      {/if}
      {#if (viewModeParam === 'all' && $browseMode === 'songs') || viewModeParam !== 'all'}
        {#if viewModeParam === 'all'}
          <div class="search-box">
            <svg xmlns="http://www.w3.org/2000/svg" class="search-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            <input
              type="text"
              placeholder="検索..."
              value={searchTerm}
              oninput={handleSearchInput}
              class="search-input"
            />
            {#if searchTerm}
              <button onclick={clearSearch} class="clear-button" aria-label="検索をクリア">✕</button>
            {/if}
          </div>
          <button onclick={toggleFilters} class="btn-icon" class:active={showFilters} title="フィルター">
            <svg xmlns="http://www.w3.org/2000/svg" class="icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
            </svg>
            {#if hasActiveFilters}
              <span class="badge">{[selectedArtist, selectedAlbum, selectedGenre].filter(Boolean).length}</span>
            {/if}
          </button>
        {/if}
        <button onclick={toggleDisplayMode} class="btn-icon" title={displayMode === 'grid' ? 'リスト表示' : 'グリッド表示'}>
          {#if displayMode === 'grid'}
            <svg xmlns="http://www.w3.org/2000/svg" class="icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" class="icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
            </svg>
          {/if}
        </button>
        {#if displayMode === 'grid'}
          <div class="size-slider">
            <input
              type="range"
              min={MIN_ART_SIZE}
              max={MAX_ART_SIZE}
              bind:value={artSize}
              class="slider"
              aria-label="カードサイズ"
            />
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <!-- フィルターパネル -->
  {#if showFilters && viewModeParam === 'all' && $browseMode === 'songs'}
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
        <button onclick={clearFilters} class="btn-clear-filters">クリア</button>
      {/if}
    </div>
  {/if}

  <!-- コンテンツ -->
  <div class="library-content">
    {#if viewModeParam === 'all' && $browseMode === 'albums'}
      <AlbumGrid />
    {:else if viewModeParam === 'all' && $browseMode === 'artists'}
      <ArtistGrid />
    {:else if viewModeParam === 'all' && $browseMode === 'genres'}
      <GenreGrid />
    {:else if isLoading}
      <div class="loading">
        <div class="loading-spinner"></div>
        <p>読み込み中...</p>
      </div>
    {:else if isError}
      <div class="error">
        <p>エラーが発生しました</p>
        <p class="error-detail">{error?.message || '不明なエラー'}</p>
      </div>
    {:else if tracks && tracks.length > 0}
      <!-- トラック表示 -->
      {#if displayMode === 'list'}
        <!-- リスト表示 -->
        <div class="track-table">
          <div class="table-header">
            <div class="col-checkbox"></div>
            <div class="col-favorite"></div>
            <button class="col-title sortable" onclick={() => toggleSort('title')}>
              タイトル {getSortIcon('title')}
            </button>
            <button class="col-artist sortable" onclick={() => toggleSort('artist')}>
              アーティスト {getSortIcon('artist')}
            </button>
            <button class="col-album sortable" onclick={() => toggleSort('album')}>
              アルバム {getSortIcon('album')}
            </button>
            <div class="col-rating">評価</div>
            <button class="col-duration sortable" onclick={() => toggleSort('duration')}>
              時間 {getSortIcon('duration')}
            </button>
          </div>
          <div class="table-body">
            {#each tracks as track (track.id)}
              <div
                class="track-row"
                class:selected={selectedTrackIds.has(track.id)}
                class:playing={currentPlayingTrackId === track.id}
                class:dragging={isDragging && draggedTrackIds.includes(track.id)}
                draggable="true"
                ondragstart={(e) => handleDragStart(e, track)}
                ondragend={handleDragEnd}
                onclick={(e) => toggleTrackSelection(track.id, e)}
                ondblclick={() => handleTrackDoubleClick(track)}
                oncontextmenu={(e) => handleContextMenu(e, track)}
                onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
                role="button"
                tabindex="0"
              >
                <div class="col-checkbox">
                  {#if currentPlayingTrackId === track.id}
                    <div class="playing-indicator">
                      <span class="bar"></span>
                      <span class="bar"></span>
                      <span class="bar"></span>
                    </div>
                  {/if}
                </div>
                <div class="col-favorite">
                  <button
                    class="favorite-btn"
                    class:active={track.isFavorite}
                    onclick={(e) => handleToggleFavorite(track.id, e)}
                    title={track.isFavorite ? 'お気に入りから削除' : 'お気に入りに追加'}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill={track.isFavorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2">
                      <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                    </svg>
                  </button>
                </div>
                <div class="col-title">
                  {@html highlightText(track.title || track.fileName, debouncedSearchTerm)}
                </div>
                <div class="col-artist">
                  {@html highlightText(track.artist || '不明なアーティスト', debouncedSearchTerm)}
                </div>
                <div class="col-album">
                  {@html highlightText(track.album || '不明なアルバム', debouncedSearchTerm)}
                </div>
                <div class="col-rating">
                  <div class="rating-stars">
                    {#each [1, 2, 3, 4, 5] as star}
                      <button
                        class="star-btn"
                        class:active={track.rating >= star}
                        onclick={(e) => handleSetRating(track.id, track.rating === star ? 0 : star, e)}
                        title={`${star}つ星`}
                      >
                        ★
                      </button>
                    {/each}
                  </div>
                </div>
                <div class="col-duration">
                  {formatDuration(track.duration)}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <!-- グリッド表示 -->
        <div
          class="tracks-grid"
          style="--card-width: {cardWidth}px; --art-size: {artSize}px;"
        >
          {#each tracks as track (track.id)}
            <div
              class="track-card"
              class:selected={selectedTrackIds.has(track.id)}
              class:playing={currentPlayingTrackId === track.id}
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track)}
              onclick={(e) => toggleTrackSelection(track.id, e)}
              ondblclick={() => handleTrackDoubleClick(track)}
              oncontextmenu={(e) => handleContextMenu(e, track)}
              onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
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
                {#if currentPlayingTrackId === track.id}
                  <div class="playing-overlay">
                    <div class="playing-indicator large">
                      <span class="bar"></span>
                      <span class="bar"></span>
                      <span class="bar"></span>
                    </div>
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
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if isSearching}
      <div class="empty">
        <svg xmlns="http://www.w3.org/2000/svg" class="empty-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <p>結果が見つかりません</p>
        <p class="hint">別の検索語を試してください</p>
      </div>
    {:else}
      <div class="empty">
        <svg xmlns="http://www.w3.org/2000/svg" class="empty-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
        </svg>
        <p>音楽ライブラリが空です</p>
        <p class="hint">フォルダをインポートして音楽を追加してください</p>
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

<!-- コンテキストメニュー -->
{#if contextMenu}
  <ContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    track={contextMenu.track}
    tracks={tracks || []}
    selectedTrackIds={selectedTrackIds}
    onClose={closeContextMenu}
    onEditMetadata={openMetadataEditor}
    onPlayNext={handlePlayNext}
    onAddToQueue={handleAddToQueue}
  />
{/if}

<style>
  .library-container {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .library-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 0;
    margin-bottom: 0.75rem;
    border-bottom: 1px solid #333;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .library-header h2 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 600;
    color: #fff;
  }

  .track-count {
    font-size: 0.875rem;
    color: #888;
  }

  .header-controls {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }

  .selection-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.375rem 0.75rem;
    background-color: rgba(59, 130, 246, 0.2);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    color: #60a5fa;
  }

  .btn-text {
    padding: 0.25rem 0.5rem;
    background: transparent;
    border: none;
    color: #60a5fa;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-text:hover {
    text-decoration: underline;
  }

  .btn-primary-sm {
    padding: 0.25rem 0.75rem;
    background-color: #3b82f6;
    color: #fff;
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-primary-sm:hover {
    background-color: #2563eb;
  }

  .search-box {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 0.75rem;
    width: 1rem;
    height: 1rem;
    color: #666;
  }

  .search-input {
    padding: 0.5rem 2rem 0.5rem 2.25rem;
    background-color: #2a2a3a;
    border: 1px solid #444;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    color: #fff;
    width: 200px;
    transition: border-color 0.2s, width 0.2s;
  }

  .search-input:focus {
    outline: none;
    border-color: #3b82f6;
    width: 280px;
  }

  .search-input::placeholder {
    color: #666;
  }

  .clear-button {
    position: absolute;
    right: 0.5rem;
    background: none;
    border: none;
    color: #666;
    cursor: pointer;
    font-size: 0.875rem;
    padding: 0.25rem;
  }

  .clear-button:hover {
    color: #fff;
  }

  .btn-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    width: 2.25rem;
    height: 2.25rem;
    padding: 0;
    background-color: #2a2a3a;
    border: 1px solid #444;
    border-radius: 0.375rem;
    color: #ccc;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-icon:hover {
    background-color: #3a3a4a;
    color: #fff;
  }

  .btn-icon.active {
    background-color: #3b82f6;
    border-color: #3b82f6;
    color: #fff;
  }

  .btn-icon .icon {
    width: 1.25rem;
    height: 1.25rem;
  }

  .btn-icon .badge {
    position: absolute;
    top: -0.25rem;
    right: -0.25rem;
    background-color: #ef4444;
    color: #fff;
    font-size: 0.625rem;
    font-weight: 600;
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .size-slider {
    display: flex;
    align-items: center;
  }

  .slider {
    width: 80px;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: #444;
    border-radius: 2px;
    outline: none;
  }

  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    background: #3b82f6;
    border-radius: 50%;
    cursor: pointer;
  }

  .filter-panel {
    display: flex;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background-color: #1e1e2e;
    border-radius: 0.375rem;
    margin-bottom: 0.75rem;
    align-items: flex-end;
  }

  .filter-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
  }

  .filter-group label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #888;
  }

  .filter-select {
    padding: 0.5rem;
    background-color: #2a2a3a;
    border: 1px solid #444;
    border-radius: 0.25rem;
    font-size: 0.875rem;
    color: #fff;
    cursor: pointer;
  }

  .filter-select:focus {
    outline: none;
    border-color: #3b82f6;
  }

  .btn-clear-filters {
    padding: 0.5rem 1rem;
    background-color: #ef4444;
    color: white;
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.875rem;
    white-space: nowrap;
  }

  .btn-clear-filters:hover {
    background-color: #dc2626;
  }

  :global(mark) {
    background-color: #fbbf24;
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
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  .loading-spinner {
    width: 2rem;
    height: 2rem;
    border: 2px solid #333;
    border-top-color: #3b82f6;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    margin-bottom: 1rem;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error {
    color: #ef4444;
  }

  .error-detail {
    font-size: 0.875rem;
    color: #888;
  }

  .empty-icon {
    width: 4rem;
    height: 4rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }

  .empty p {
    margin: 0.25rem 0;
  }

  .hint {
    font-size: 0.875rem;
    color: #666;
  }

  /* テーブル表示 */
  .track-table {
    display: flex;
    flex-direction: column;
  }

  .table-header {
    display: grid;
    grid-template-columns: 2.5rem 2rem 2fr 1.5fr 1.5fr 5rem 4rem;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
    border-bottom: 1px solid #333;
    position: sticky;
    top: 0;
    background-color: #0f0f1a;
    z-index: 1;
  }

  .sortable {
    background: none;
    border: none;
    color: #888;
    cursor: pointer;
    text-align: left;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    padding: 0;
    transition: color 0.2s;
  }

  .sortable:hover {
    color: #fff;
  }

  .table-body {
    display: flex;
    flex-direction: column;
  }

  .track-row {
    display: grid;
    grid-template-columns: 2.5rem 2rem 2fr 1.5fr 1.5fr 5rem 4rem;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    align-items: center;
    cursor: pointer;
    border-radius: 0.25rem;
    transition: background-color 0.15s;
  }

  .track-row:hover {
    background-color: rgba(255, 255, 255, 0.05);
  }

  .track-row.selected {
    background-color: rgba(59, 130, 246, 0.2);
  }

  .track-row.playing {
    background-color: rgba(29, 185, 84, 0.15);
  }

  .track-row.playing .col-title {
    color: #1db954;
  }

  .col-checkbox {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .col-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #fff;
  }

  .col-artist,
  .col-album {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #aaa;
    font-size: 0.875rem;
  }

  .col-duration {
    text-align: right;
    color: #888;
    font-size: 0.875rem;
  }

  /* 再生中インジケーター */
  .playing-indicator {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 1rem;
  }

  .playing-indicator .bar {
    width: 3px;
    background-color: #1db954;
    border-radius: 1px;
    animation: equalizer 0.5s ease-in-out infinite alternate;
  }

  .playing-indicator .bar:nth-child(1) {
    height: 40%;
    animation-delay: 0s;
  }

  .playing-indicator .bar:nth-child(2) {
    height: 80%;
    animation-delay: 0.2s;
  }

  .playing-indicator .bar:nth-child(3) {
    height: 60%;
    animation-delay: 0.4s;
  }

  .playing-indicator.large {
    height: 2rem;
  }

  .playing-indicator.large .bar {
    width: 4px;
  }

  @keyframes equalizer {
    0% { height: 20%; }
    100% { height: 100%; }
  }

  /* グリッド表示 */
  .tracks-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--card-width, 150px), 1fr));
    gap: 1rem;
    justify-items: center;
  }

  .track-card {
    width: var(--card-width, 150px);
    padding: 0.75rem;
    background-color: #1e1e2e;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: transform 0.2s, box-shadow 0.2s, background-color 0.2s;
    border: 2px solid transparent;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .track-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    background-color: #252538;
  }

  .track-card.selected {
    background-color: rgba(59, 130, 246, 0.2);
    border-color: #3b82f6;
  }

  .track-card.playing {
    border-color: #1db954;
  }

  .album-art {
    position: relative;
    flex-shrink: 0;
    border-radius: 0.375rem;
    overflow: hidden;
    background-color: #2a2a3a;
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

  .playing-overlay {
    position: absolute;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
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
    font-size: 0.875rem;
    color: #fff;
  }

  .track-artist {
    font-size: 0.75rem;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* お気に入りとレーティング */
  .col-favorite {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .col-rating {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    color: #888;
  }

  .favorite-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    background: none;
    border: none;
    color: #666;
    cursor: pointer;
    transition: color 0.15s, transform 0.15s;
  }

  .favorite-btn:hover {
    color: #f43f5e;
    transform: scale(1.1);
  }

  .favorite-btn.active {
    color: #f43f5e;
  }

  .favorite-btn svg {
    width: 1rem;
    height: 1rem;
  }

  .rating-stars {
    display: flex;
    gap: 1px;
  }

  .star-btn {
    background: none;
    border: none;
    padding: 0;
    font-size: 0.875rem;
    color: #444;
    cursor: pointer;
    transition: color 0.1s, transform 0.1s;
    line-height: 1;
  }

  .star-btn:hover {
    color: #fbbf24;
    transform: scale(1.2);
  }

  .star-btn.active {
    color: #fbbf24;
  }

  /* ドラッグ状態 */
  .track-row.dragging,
  .track-card.dragging {
    opacity: 0.5;
    background-color: rgba(59, 130, 246, 0.3);
  }

  /* グローバルドラッグプレビュー */
  :global(.drag-preview) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background-color: #3b82f6;
    color: white;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  :global(.drag-preview-icon) {
    width: 1.25rem;
    height: 1.25rem;
  }

  :global(.drag-preview-icon svg) {
    width: 100%;
    height: 100%;
  }

  :global(.drag-preview-count) {
    white-space: nowrap;
  }
</style>
