<script lang="ts">
  import { page } from '$app/stores';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation, useCreatePlaylistMutation } from '$lib/queries/playlists';
  import { validatePlaylistName, toSafeString } from '$lib/utils/validation';
  import { isImportDialogOpen, isSidebarOpen } from '$lib/stores/ui';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';
  import type { Playlist } from '$lib/types/models';
  import PlaylistContextMenu from './PlaylistContextMenu.svelte';

  // パス変更時にサイドバーを閉じる（モバイル用）
  let previousPath = $state('');
  $effect(() => {
    const path = $page.url.pathname;
    if (previousPath && path !== previousPath) {
      // パスが変更されたらサイドバーを閉じる（モバイル幅の場合のみ）
      if (typeof window !== 'undefined' && window.innerWidth <= 1024) {
        isSidebarOpen.set(false);
      }
    }
    previousPath = path;
  });

  // インポートダイアログを開く
  function handleOpenImportDialog() {
    isImportDialogOpen.set(true);
  }

  // ジャンルクエリ
  const genresQuery = useGenresGroupedQuery();
  const genres = $derived(genresQuery.data ?? []);

  // ジャンルが展開されているか
  let isGenreExpanded = $state(false);

  // 現在のパスからアクティブなページを判定
  const currentPath = $derived($page.url.pathname);

  // クエリとミューテーション
  const playlistsQuery = usePlaylistsQuery();
  const addTrackMutation = useAddTrackToPlaylistMutation();
  const createPlaylistMutation = useCreatePlaylistMutation();

  // コンテキストメニュー
  let contextMenu = $state<{ x: number; y: number; playlist: Playlist } | null>(null);

  /**
   * 右クリックメニューを表示
   */
  function handleContextMenu(event: MouseEvent, playlist: Playlist) {
    event.preventDefault();
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      playlist
    };
  }

  /**
   * 右クリックメニューを閉じる
   */
  function closeContextMenu() {
    contextMenu = null;
  }

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

  // ジャンル展開トグル
  function toggleGenreExpand() {
    isGenreExpanded = !isGenreExpanded;
  }
</script>

<aside class="flex flex-col h-full p-4 bg-base-200 text-text-secondary">
  <!-- サイドバーヘッダー -->
  <div class="mb-6 flex items-center justify-between">
    <h1 class="text-2xl font-bold text-text-primary m-0">Muspice</h1>
    <button
      class="btn-icon w-8 h-8 p-0"
      title="フォルダをインポート"
      onclick={handleOpenImportDialog}
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
      </svg>
    </button>
  </div>

  <!-- ブラウズセクション -->
  <div class="mb-6">
    <h2 class="section-title">ブラウズ</h2>
    <ul class="list-none m-0 p-0">
      <li>
        <a
          href="/library/songs"
          class="nav-item-base"
          class:active={currentPath === '/library/songs'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M9 18V5l12-2v13" />
            <circle cx="6" cy="18" r="3" />
            <circle cx="18" cy="16" r="3" />
          </svg>
          <span>曲</span>
        </a>
      </li>
      <li>
        <a
          href="/library/albums"
          class="nav-item-base"
          class:active={currentPath === '/library/albums'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span>アルバム</span>
        </a>
      </li>
      <li>
        <a
          href="/library/artists"
          class="nav-item-base"
          class:active={currentPath === '/library/artists'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
          <span>アーティスト</span>
        </a>
      </li>
      <li>
        <div class="genre-nav-container">
          <a
            href="/library/genres"
            class="nav-item-base flex-1"
            class:active={currentPath === '/library/genres'}
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
              <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
            </svg>
            <span class="flex-1 text-left">ジャンル</span>
          </a>
          <button
            class="genre-expand-btn"
            class:active={isGenreExpanded}
            onclick={toggleGenreExpand}
            title="ジャンルを展開"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="w-4 h-4 transition-transform duration-200"
              class:rotate-90={isGenreExpanded}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>
        <!-- ジャンルサブリスト -->
        {#if isGenreExpanded && genres.length > 0}
          <ul class="genre-sublist">
            {#each genres as genre (genre.name)}
              <li>
                <a
                  href="/library/genres/{encodeURIComponent(genre.name)}"
                  class="genre-item"
                  class:active={currentPath === `/library/genres/${encodeURIComponent(genre.name)}`}
                >
                  <span class="genre-name">{genre.name}</span>
                  <span class="genre-count">{genre.trackCount}</span>
                </a>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    </ul>
  </div>

  <!-- ライブラリセクション -->
  <div class="mb-6">
    <h2 class="section-title">ライブラリ</h2>
    <ul class="list-none m-0 p-0">
      <li>
        <a
          href="/library/recent"
          class="nav-item-base"
          class:active={currentPath === '/library/recent'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>最近再生した曲</span>
        </a>
      </li>
      <li>
        <a
          href="/library/mostplayed"
          class="nav-item-base"
          class:active={currentPath === '/library/mostplayed'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" />
          </svg>
          <span>よく再生する曲</span>
        </a>
      </li>
    </ul>
  </div>

  <!-- プレイリストセクション -->
  <div class="flex-1 flex flex-col min-h-0 mb-6">
    <div class="flex items-center justify-between mb-2 px-2">
      <h2 class="text-xs font-semibold uppercase tracking-wider text-text-muted m-0">プレイリスト</h2>
      <button
        class="btn-icon w-6 h-6 p-0"
        title="新規プレイリスト (Ctrl+N)"
        onclick={handleCreatePlaylist}
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
      </button>
    </div>

    <ul class="list-none m-0 p-0 flex-1 overflow-y-auto">
      {#if playlistsQuery.isLoading}
        <li class="px-3 py-2 text-sm text-text-dimmed">読み込み中...</li>
      {:else if playlistsQuery.isError}
        <li class="px-3 py-2 text-sm text-error-light">エラーが発生しました</li>
      {:else if playlistsQuery.data}
        {#each playlistsQuery.data as playlist (playlist.id)}
          <li>
            <a
              href="/playlists/{playlist.id}"
              class="nav-item-base relative"
              class:active={currentPath === `/playlists/${playlist.id}`}
              ondragover={handleDragOver}
              ondragleave={handleDragLeave}
              ondrop={(e) => handleDrop(e, playlist.id)}
              oncontextmenu={(e) => handleContextMenu(e, playlist)}
            >
              <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
              </svg>
              <span class="flex-1 text-truncate">{playlist.name}</span>
              <span class="text-xs text-text-dimmed shrink-0">{playlist.tracks.length}</span>
            </a>
          </li>
        {/each}
        {#if playlistsQuery.data.length === 0}
          <li class="px-3 py-2 text-sm text-text-dimmed">プレイリストがありません</li>
        {/if}
      {/if}
    </ul>
  </div>
</aside>

<!-- コンテキストメニュー -->
{#if contextMenu}
  <PlaylistContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    playlist={contextMenu.playlist}
    onClose={closeContextMenu}
  />
{/if}

<style>
@reference "../../app.css";
  .genre-nav-container {
    @apply flex items-center;
  }

  .genre-expand-btn {
    @apply flex items-center justify-center w-8 h-8 rounded text-text-muted bg-transparent border-none cursor-pointer transition-colors duration-150;
  }

  .genre-expand-btn:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .genre-expand-btn.active {
    @apply text-primary;
  }

  .genre-sublist {
    @apply list-none m-0 p-0 ml-4 mt-1 border-l border-border;
  }

  .genre-item {
    @apply flex items-center w-full px-3 py-1.5 text-sm text-text-secondary bg-transparent border-none cursor-pointer transition-colors duration-150 text-left;
  }

  .genre-item:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .genre-item.active {
    @apply bg-primary/20 text-primary;
  }

  .genre-name {
    @apply flex-1 truncate;
  }

  .genre-count {
    @apply text-xs text-text-dimmed shrink-0 ml-2;
  }
</style>
