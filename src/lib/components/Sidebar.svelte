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

<aside class="flex flex-col h-full p-4 bg-base-200 text-text-secondary">
  <!-- サイドバーヘッダー -->
  <div class="mb-6">
    <h1 class="text-2xl font-bold text-text-primary m-0">Muspice</h1>
  </div>

  <!-- ライブラリセクション -->
  <div class="mb-6">
    <h2 class="section-title">ライブラリ</h2>
    <ul class="list-none m-0 p-0">
      <li>
        <a
          href="/"
          class="nav-item-base"
          class:active={$page.url.pathname === '/' && !$page.url.searchParams.get('view')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
          </svg>
          <span>すべての曲</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=favorites"
          class="nav-item-base"
          class:active={$page.url.searchParams.get('view') === 'favorites'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
          </svg>
          <span>お気に入り</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=recent"
          class="nav-item-base"
          class:active={$page.url.searchParams.get('view') === 'recent'}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>最近再生した曲</span>
        </a>
      </li>
      <li>
        <a
          href="/?view=mostplayed"
          class="nav-item-base"
          class:active={$page.url.searchParams.get('view') === 'mostplayed'}
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
              href="/playlists?id={playlist.id}"
              class="nav-item-base relative"
              class:active={$page.url.pathname === '/playlists' && $page.url.searchParams.get('id') === playlist.id}
              ondragover={handleDragOver}
              ondragleave={handleDragLeave}
              ondrop={(e) => handleDrop(e, playlist.id)}
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
