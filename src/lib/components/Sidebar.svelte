<script lang="ts">
  import { page } from '$app/stores';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation, useCreatePlaylistMutation } from '$lib/queries/playlists';
  import { validatePlaylistName, toSafeString } from '$lib/utils/validation';

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const addTrackMutation = useAddTrackToPlaylistMutation();
  const createPlaylistMutation = useCreatePlaylistMutation();

  /**
   * 新規プレイリストを作成
   */
  function handleCreatePlaylist() {
    const name = prompt('プレイリスト名を入力してください');
    if (!name || !name.trim()) return;

    const trimmedName = name.trim();
    const validation = validatePlaylistName(trimmedName);
    if (!validation.valid) {
      alert(validation.error);
      return;
    }

    const safeName = toSafeString(trimmedName, 100);
    createPlaylistMutation.mutate(safeName);
  }

  /**
   * プレイリストへのドラッグオーバー
   */
  function handleDragOver(event: DragEvent) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'copy';
    }
    (event.currentTarget as HTMLElement).classList.add('playlist-drag-over');
  }

  /**
   * プレイリストからのドラッグ離脱
   */
  function handleDragLeave(event: DragEvent) {
    (event.currentTarget as HTMLElement).classList.remove('playlist-drag-over');
  }

  /**
   * プレイリストへのドロップ
   */
  function handleDrop(event: DragEvent, playlistId: string) {
    event.preventDefault();
    (event.currentTarget as HTMLElement).classList.remove('playlist-drag-over');

    const trackId = event.dataTransfer?.getData('text/plain');
    const trackIdsJson = event.dataTransfer?.getData('application/json');

    if (trackIdsJson) {
      // 複数トラックの追加
      try {
        const trackIds = JSON.parse(trackIdsJson) as string[];
        for (const id of trackIds) {
          addTrackMutation.mutate({ playlistId, trackId: id });
        }
      } catch {
        // JSON解析失敗時は単一トラックとして処理
        if (trackId) {
          addTrackMutation.mutate({ playlistId, trackId });
        }
      }
    } else if (trackId) {
      addTrackMutation.mutate({ playlistId, trackId });
    }
  }
</script>

<aside class="sidebar">
  <!-- サイドバーヘッダー -->
  <div class="sidebar-header">
    <h1 class="app-title">Muspice</h1>
  </div>

  <!-- ライブラリセクション -->
  <div class="sidebar-section">
    <h2 class="section-title">ライブラリ</h2>
    <ul class="nav-list">
      <li>
        <a
          href="/"
          class="nav-item"
          class:active={$page.url.pathname === '/' && !$page.url.searchParams.get('view')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
          </svg>
          <span>すべての曲</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=favorites"
          class="nav-item"
          class:active={$page.url.searchParams.get('view') === 'favorites'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
          </svg>
          <span>お気に入り</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=recent"
          class="nav-item"
          class:active={$page.url.searchParams.get('view') === 'recent'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>最近再生した曲</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=mostplayed"
          class="nav-item"
          class:active={$page.url.searchParams.get('view') === 'mostplayed'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" />
          </svg>
          <span>よく再生する曲</span>
        </a>
      </li>
    </ul>
  </div>

  <!-- プレイリストセクション -->
  <div class="sidebar-section playlists-section">
    <div class="section-header">
      <h2 class="section-title">プレイリスト</h2>
      <button
        class="btn-add-playlist"
        title="新規プレイリスト (Ctrl+N)"
        onclick={handleCreatePlaylist}
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="icon-add" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
      </button>
    </div>

    <ul class="nav-list playlist-list">
      {#if playlistsQuery.isLoading}
        <li class="nav-message">読み込み中...</li>
      {:else if playlistsQuery.isError}
        <li class="nav-message error">エラーが発生しました</li>
      {:else if playlistsQuery.data}
        {#each playlistsQuery.data as playlist (playlist.id)}
          <li>
            <a
              href="/playlists?id={playlist.id}"
              class="nav-item playlist-item"
              class:active={$page.url.pathname === '/playlists' && $page.url.searchParams.get('id') === playlist.id}
              ondragover={handleDragOver}
              ondragleave={handleDragLeave}
              ondrop={(e) => handleDrop(e, playlist.id)}
            >
              <svg xmlns="http://www.w3.org/2000/svg" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
              </svg>
              <span class="playlist-name">{playlist.name}</span>
              <span class="playlist-count">{playlist.tracks.length}</span>
            </a>
          </li>
        {/each}
        {#if playlistsQuery.data.length === 0}
          <li class="nav-message">プレイリストがありません</li>
        {/if}
      {/if}
    </ul>
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1rem;
    background-color: #1a1a2e;
    color: #e0e0e0;
  }

  .sidebar-header {
    margin-bottom: 1.5rem;
  }

  .app-title {
    font-size: 1.5rem;
    font-weight: 700;
    margin: 0;
    color: #fff;
  }

  .sidebar-section {
    margin-bottom: 1.5rem;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.5rem;
    padding: 0 0.5rem;
  }

  .section-title {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
    margin: 0;
    padding: 0 0.5rem;
    margin-bottom: 0.5rem;
  }

  .section-header .section-title {
    margin-bottom: 0;
    padding: 0;
  }

  .btn-add-playlist {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: none;
    background: transparent;
    color: #888;
    cursor: pointer;
    border-radius: 0.25rem;
    transition: all 0.2s;
  }

  .btn-add-playlist:hover {
    background-color: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .icon-add {
    width: 1rem;
    height: 1rem;
  }

  .nav-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    color: #ccc;
    text-decoration: none;
    transition: all 0.2s;
  }

  .nav-item:hover {
    background-color: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .nav-item.active {
    background-color: rgba(59, 130, 246, 0.2);
    color: #60a5fa;
  }

  .nav-icon {
    width: 1.25rem;
    height: 1.25rem;
    flex-shrink: 0;
  }

  .playlists-section {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .playlist-list {
    flex: 1;
    overflow-y: auto;
  }

  .playlist-item {
    position: relative;
  }

  .playlist-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .playlist-count {
    font-size: 0.75rem;
    color: #666;
    flex-shrink: 0;
  }

  /* ドラッグオーバー時のスタイル */
  :global(.playlist-drag-over) {
    background-color: rgba(59, 130, 246, 0.3) !important;
    outline: 2px dashed #60a5fa;
    outline-offset: -2px;
  }

  .nav-message {
    padding: 0.5rem 0.75rem;
    font-size: 0.875rem;
    color: #666;
  }

  .nav-message.error {
    color: #f87171;
  }

  /* スクロールバーのスタイル */
  .playlist-list::-webkit-scrollbar {
    width: 6px;
  }

  .playlist-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .playlist-list::-webkit-scrollbar-thumb {
    background-color: rgba(255, 255, 255, 0.2);
    border-radius: 3px;
  }

  .playlist-list::-webkit-scrollbar-thumb:hover {
    background-color: rgba(255, 255, 255, 0.3);
  }
</style>
