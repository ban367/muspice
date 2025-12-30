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
  import { playTrackFromQueue, currentTrack, playQueue, currentTrackIndex, isPlaying } from '$lib/stores/player';
  import { browseMode, gridCardSize, MIN_CARD_SIZE, MAX_CARD_SIZE } from '$lib/stores/ui';
  import { get } from 'svelte/store';
  import type { Track, AlbumArt } from '$lib/types/models';
  import BrowseModeSelector from './library/BrowseModeSelector.svelte';
  import AlbumGrid from './library/AlbumGrid.svelte';
  import ArtistGrid from './library/ArtistGrid.svelte';
  import GenreGrid from './library/GenreGrid.svelte';
  import PlayingIndicator from './library/PlayingIndicator.svelte';
  import CardSizeSlider from './library/CardSizeSlider.svelte';

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

  // アルバムアートサイズは共通ストアから取得
  const artSize = $derived($gridCardSize);

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

  // 表示するクエリを選択
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

  function toggleSort(field: SortField) {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = 'asc';
    }
  }

  function getSortIcon(field: SortField): string {
    if (sortField !== field) return '';
    return sortDirection === 'asc' ? '↑' : '↓';
  }

  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchTerm = target.value;

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

  function highlightText(text: string, search: string): string {
    if (!search || !text) return text;
    const regex = new RegExp(`(${search})`, 'gi');
    return text.replace(regex, '<mark class="bg-warning text-black px-0.5 rounded-sm">$1</mark>');
  }

  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  function toggleDisplayMode() {
    displayMode = displayMode === 'grid' ? 'list' : 'grid';
  }

  function toggleFilters() {
    showFilters = !showFilters;
  }

  function clearFilters() {
    selectedArtist = '';
    selectedAlbum = '';
    selectedGenre = '';
  }

  const hasActiveFilters = $derived(!!(selectedArtist || selectedAlbum || selectedGenre));

  const selectedTracks = $derived.by(() => {
    if (!tracks) return [];
    return tracks.filter((track) => selectedTrackIds.has(track.id));
  });

  function toggleTrackSelection(trackId: string, event: MouseEvent | KeyboardEvent) {
    event.stopPropagation();

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
      if (newSelection.has(trackId)) {
        newSelection.delete(trackId);
      } else {
        newSelection.add(trackId);
      }
    } else {
      if (newSelection.has(trackId) && newSelection.size === 1) {
        newSelection.clear();
      } else {
        newSelection.clear();
        newSelection.add(trackId);
      }
    }

    selectedTrackIds = newSelection;
  }

  function clearSelection() {
    selectedTrackIds = new Set();
  }

  function openMetadataEditor() {
    if (selectedTrackIds.size > 0) {
      showMetadataEditor = true;
    }
  }

  function closeMetadataEditor() {
    showMetadataEditor = false;
  }

  function handleMetadataSaved() {
    clearSelection();
  }

  function handleTrackDoubleClick(track: Track) {
    if (!tracks) return;

    const trackIndex = tracks.findIndex((t) => t.id === track.id);
    if (trackIndex !== -1) {
      playTrackFromQueue(tracks, trackIndex);
    }
  }

  function handleContextMenu(event: MouseEvent, track: Track) {
    event.preventDefault();

    if (!selectedTrackIds.has(track.id)) {
      selectedTrackIds = new Set([track.id]);
    }

    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      track
    };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function handlePlayNext() {
    const queue = get(playQueue);
    const currentIndex = get(currentTrackIndex);

    const newQueue = [...queue];
    newQueue.splice(currentIndex + 1, 0, ...selectedTracks);
    playQueue.set(newQueue);
  }

  function handleAddToQueue() {
    const queue = get(playQueue);
    playQueue.set([...queue, ...selectedTracks]);
  }

  async function loadAlbumArt(trackId: string): Promise<void> {
    if (albumArtCache.has(trackId)) {
      return;
    }

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

  function getAlbumArtUrl(trackId: string): string | null {
    return albumArtCache.get(trackId) ?? null;
  }

  const cardWidth = $derived(artSize + 24);

  function handleDragStart(event: DragEvent, track: Track) {
    if (!event.dataTransfer) return;

    event.dataTransfer.effectAllowed = 'copy';

    let trackIds: string[];
    if (selectedTrackIds.size > 0 && selectedTrackIds.has(track.id)) {
      trackIds = Array.from(selectedTrackIds);
    } else {
      trackIds = [track.id];
    }

    event.dataTransfer.setData('application/json', JSON.stringify(trackIds));
    event.dataTransfer.setData('text/plain', trackIds[0]);

    isDragging = true;
    draggedTrackIds = trackIds;

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

    setTimeout(() => {
      document.body.removeChild(dragImage);
    }, 0);
  }

  function handleDragEnd() {
    isDragging = false;
    draggedTrackIds = [];
  }

  $effect(() => {
    if (tracks && displayMode === 'grid') {
      for (const track of tracks.slice(0, 50)) {
        if (!albumArtCache.has(track.id)) {
          loadAlbumArt(track.id);
        }
      }
    }
  });
</script>

<div class="flex flex-col h-full">
  <!-- ヘッダー -->
  <div class="flex justify-between items-center py-3 mb-3 border-b border-border">
    <div class="flex items-center gap-3">
      {#if viewModeParam === 'all'}
        <BrowseModeSelector />
      {/if}
      {#if $browseMode === 'songs' && tracks}
        <span class="text-sm text-text-muted">{tracks.length}曲</span>
      {/if}
    </div>
    <div class="flex gap-3 items-center">
      {#if selectedTrackIds.size > 0}
        <div class="selection-info">
          {selectedTrackIds.size}件選択中
          <button onclick={clearSelection} class="btn-text">選択解除</button>
          <button onclick={openMetadataEditor} class="btn-primary-sm">
            メタデータを編集
          </button>
        </div>
      {/if}
      {#if $browseMode === 'songs' || viewModeParam !== 'all'}
        {#if viewModeParam === 'all'}
          <div class="search-box">
            <svg xmlns="http://www.w3.org/2000/svg" class="absolute left-3 w-4 h-4 text-text-dimmed" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
              <button onclick={clearSearch} class="absolute right-2 p-1 text-text-dimmed hover:text-text-primary" aria-label="検索をクリア">✕</button>
            {/if}
          </div>
          <button onclick={toggleFilters} class="header-btn" class:active={showFilters} title="フィルター">
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
            </svg>
            {#if hasActiveFilters}
              <span class="filter-badge">{[selectedArtist, selectedAlbum, selectedGenre].filter(Boolean).length}</span>
            {/if}
          </button>
        {/if}
        <button onclick={toggleDisplayMode} class="header-btn" title={displayMode === 'grid' ? 'リスト表示' : 'グリッド表示'}>
          {#if displayMode === 'grid'}
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
            </svg>
          {/if}
        </button>
        {#if displayMode === 'grid'}
          <CardSizeSlider />
        {/if}
      {:else if $browseMode === 'albums' || $browseMode === 'artists'}
        <CardSizeSlider />
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
  <div class="flex-1 overflow-y-auto">
    {#if viewModeParam === 'all' && $browseMode === 'albums'}
      <AlbumGrid />
    {:else if viewModeParam === 'all' && $browseMode === 'artists'}
      <ArtistGrid />
    {:else if viewModeParam === 'all' && $browseMode === 'genres'}
      <GenreGrid />
    {:else if isLoading}
      <div class="empty-state">
        <div class="spinner"></div>
        <p>読み込み中...</p>
      </div>
    {:else if isError}
      <div class="empty-state text-error">
        <p>エラーが発生しました</p>
        <p class="text-sm text-text-muted">{error?.message || '不明なエラー'}</p>
      </div>
    {:else if tracks && tracks.length > 0}
      {#if displayMode === 'list'}
        <!-- リスト表示 -->
        <div class="track-table">
          <div class="table-header">
            <div class="col-checkbox"></div>
            <div class="col-favorite"></div>
            <button class="sortable" onclick={() => toggleSort('title')}>
              タイトル {getSortIcon('title')}
            </button>
            <button class="sortable" onclick={() => toggleSort('artist')}>
              アーティスト {getSortIcon('artist')}
            </button>
            <button class="sortable" onclick={() => toggleSort('album')}>
              アルバム {getSortIcon('album')}
            </button>
            <div class="col-rating">評価</div>
            <button class="sortable text-right" onclick={() => toggleSort('duration')}>
              時間 {getSortIcon('duration')}
            </button>
          </div>
          <div class="flex flex-col">
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
                <div class="col-checkbox flex items-center justify-center">
                  {#if currentPlayingTrackId === track.id}
                    <PlayingIndicator size="small" />
                  {/if}
                </div>
                <div class="col-favorite flex items-center justify-center">
                  <button
                    class="favorite-btn"
                    class:active={track.isFavorite}
                    onclick={(e) => handleToggleFavorite(track.id, e)}
                    title={track.isFavorite ? 'お気に入りから削除' : 'お気に入りに追加'}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill={track.isFavorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2" class="w-4 h-4">
                      <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                    </svg>
                  </button>
                </div>
                <div class="text-truncate text-text-primary">
                  {@html highlightText(track.title || track.fileName, debouncedSearchTerm)}
                </div>
                <div class="text-truncate text-text-secondary text-sm">
                  {@html highlightText(track.artist || '不明なアーティスト', debouncedSearchTerm)}
                </div>
                <div class="text-truncate text-text-secondary text-sm">
                  {@html highlightText(track.album || '不明なアルバム', debouncedSearchTerm)}
                </div>
                <div class="col-rating flex items-center justify-center">
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
                <div class="text-right text-text-muted text-sm">
                  {formatDuration(track.duration)}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <!-- グリッド表示 -->
        <div
          class="grid gap-4 justify-items-center"
          style="grid-template-columns: repeat(auto-fill, minmax({cardWidth}px, 1fr));"
        >
          {#each tracks as track (track.id)}
            <div
              class="track-card"
              class:selected={selectedTrackIds.has(track.id)}
              class:playing={currentPlayingTrackId === track.id}
              style="width: {cardWidth}px;"
              draggable="true"
              ondragstart={(e) => handleDragStart(e, track)}
              onclick={(e) => toggleTrackSelection(track.id, e)}
              ondblclick={() => handleTrackDoubleClick(track)}
              oncontextmenu={(e) => handleContextMenu(e, track)}
              onkeydown={(e) => e.key === 'Enter' && handleTrackDoubleClick(track)}
              role="button"
              tabindex="0"
            >
              <div class="relative shrink-0 rounded-md overflow-hidden bg-base-400 mb-2" style="width: {artSize}px; height: {artSize}px;">
                {#if getAlbumArtUrl(track.id)}
                  <img
                    src={getAlbumArtUrl(track.id)}
                    alt="アルバムアート"
                    class="w-full h-full object-cover"
                    loading="lazy"
                  />
                {:else}
                  <div class="w-full h-full flex items-center justify-center text-white/80 bg-gradient-to-br from-accent to-accent-focus">
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-2/5 h-2/5">
                      <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                    </svg>
                  </div>
                {/if}
                {#if currentPlayingTrackId === track.id}
                  <div class="absolute inset-0 bg-black/50 flex items-center justify-center">
                    <PlayingIndicator size="large" />
                  </div>
                {/if}
              </div>
              <div class="w-full text-center min-w-0">
                <div class="font-semibold mb-1 text-truncate text-sm text-text-primary" title={track.title || track.fileName}>
                  {@html highlightText(track.title || track.fileName, debouncedSearchTerm)}
                </div>
                <div class="text-xs text-text-muted text-truncate" title={track.artist || '不明なアーティスト'}>
                  {@html highlightText(track.artist || '不明なアーティスト', debouncedSearchTerm)}
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if isSearching}
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" class="w-16 h-16 text-text-dimmed/50 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <p>結果が見つかりません</p>
        <p class="text-sm text-text-dimmed">別の検索語を試してください</p>
      </div>
    {:else}
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" class="w-16 h-16 text-text-dimmed/50 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
        </svg>
        <p>音楽ライブラリが空です</p>
        <p class="text-sm text-text-dimmed">フォルダをインポートして音楽を追加してください</p>
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
@reference "../../app.css";
  /* 選択情報 */
  .selection-info {
    @apply flex items-center gap-2 px-3 py-1.5 bg-primary/20 rounded-md text-sm text-primary;
  }

  .btn-text {
    @apply px-2 py-1 bg-transparent border-none text-primary cursor-pointer text-sm hover:underline;
  }

  .btn-primary-sm {
    @apply px-3 py-1 bg-primary text-primary-content border-none rounded cursor-pointer text-sm hover:bg-primary-focus;
  }

  /* 検索ボックス */
  .search-box {
    @apply relative flex items-center;
  }

  .search-input {
    @apply py-2 pl-9 pr-8 bg-base-400 border border-border rounded-md text-sm text-text-primary w-52 transition-all duration-200;
  }

  .search-input:focus {
    @apply outline-none border-primary w-72;
  }

  .search-input::placeholder {
    @apply text-text-dimmed;
  }

  /* ヘッダーボタン */
  .header-btn {
    @apply flex items-center justify-center relative w-9 h-9 p-0 bg-base-400 border border-border rounded-md text-text-secondary cursor-pointer transition-all duration-200;
  }

  .header-btn:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .header-btn.active {
    @apply bg-primary border-primary text-primary-content;
  }

  .filter-badge {
    @apply absolute -top-1 -right-1 bg-error text-white text-[0.625rem] font-semibold w-4 h-4 rounded-full flex items-center justify-center;
  }

  /* フィルターパネル */
  .filter-panel {
    @apply flex gap-4 px-4 py-3 bg-base-300 rounded-md mb-3 items-end;
  }

  .filter-group {
    @apply flex flex-col gap-1 flex-1;
  }

  .filter-group label {
    @apply text-xs font-medium text-text-muted;
  }

  .filter-select {
    @apply py-2 px-2 bg-base-400 border border-border rounded text-sm text-text-primary cursor-pointer focus:outline-none focus:border-primary;
  }

  .btn-clear-filters {
    @apply px-4 py-2 bg-error text-white border-none rounded cursor-pointer text-sm whitespace-nowrap hover:bg-error/80;
  }

  /* トラックテーブル */
  .track-table {
    @apply flex flex-col;
  }

  .table-header {
    @apply grid gap-3 px-4 py-2 text-xs font-semibold uppercase text-text-muted border-b border-border sticky top-0 bg-base-100 z-10;
    grid-template-columns: 2.5rem 2rem 2fr 1.5fr 1.5fr 5rem 4rem;
  }

  .sortable {
    @apply bg-transparent border-none text-text-muted cursor-pointer text-left text-xs font-semibold uppercase p-0 transition-colors hover:text-text-primary;
  }

  .track-row {
    @apply grid gap-3 px-4 py-2 items-center cursor-pointer rounded transition-colors;
    grid-template-columns: 2.5rem 2rem 2fr 1.5fr 1.5fr 5rem 4rem;
  }

  .track-row:hover {
    @apply bg-surface;
  }

  .track-row.selected {
    @apply bg-primary/20;
  }

  .track-row.playing {
    @apply bg-secondary/15;
  }

  .track-row.playing > div:nth-child(3) {
    @apply text-secondary;
  }

  .track-row.dragging {
    @apply opacity-50 bg-primary/30;
  }

  /* お気に入りボタン */
  .favorite-btn {
    @apply flex items-center justify-center w-6 h-6 p-0 bg-transparent border-none text-text-dimmed cursor-pointer transition-all;
  }

  .favorite-btn:hover {
    @apply text-error scale-110;
  }

  .favorite-btn.active {
    @apply text-error;
  }

  /* レーティング */
  .col-rating {
    @apply text-xs text-text-muted;
  }

  .rating-stars {
    @apply flex gap-px;
  }

  .star-btn {
    @apply bg-transparent border-none p-0 text-sm text-base-400 cursor-pointer transition-all leading-none;
  }

  .star-btn:hover {
    @apply text-warning scale-125;
  }

  .star-btn.active {
    @apply text-warning;
  }

  /* トラックカード */
  .track-card {
    @apply p-3 bg-base-300 rounded-lg cursor-pointer transition-all border-2 border-transparent flex flex-col items-center;
  }

  .track-card:hover {
    @apply -translate-y-0.5 shadow-lg bg-surface-hover;
  }

  .track-card.selected {
    @apply bg-primary/20 border-primary;
  }

  .track-card.playing {
    @apply border-secondary;
  }

  /* ドラッグプレビュー */
  :global(.drag-preview) {
    @apply flex items-center gap-2 px-3 py-2 bg-primary text-white rounded-md text-sm font-medium shadow-lg;
  }

  :global(.drag-preview-icon) {
    @apply w-5 h-5;
  }

  :global(.drag-preview-icon svg) {
    @apply w-full h-full;
  }

  :global(.drag-preview-count) {
    @apply whitespace-nowrap;
  }
</style>
