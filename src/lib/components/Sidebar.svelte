<script lang="ts">
  import { page } from '$app/stores';
  import { usePlaylistsQuery, useAddTrackToPlaylistMutation, useCreatePlaylistMutation } from '$lib/queries/playlists';
  import { validatePlaylistName, toSafeString } from '$lib/utils/validation';
  import { browseMode, type BrowseMode } from '$lib/stores/ui';

  // ブラウズモードを変更
  function handleBrowseModeChange(mode: BrowseMode) {
    browseMode.set(mode);
  }

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

  <!-- ブラウズセクション -->
  <div class="mb-6">
    <h2 class="section-title">ブラウズ</h2>
    <ul class="list-none m-0 p-0">
      <li>
        <button
          class="nav-item-base w-full"
          class:active={$browseMode === 'songs'}
          onclick={() => handleBrowseModeChange('songs')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M9 18V5l12-2v13" />
            <circle cx="6" cy="18" r="3" />
            <circle cx="18" cy="16" r="3" />
          </svg>
          <span>曲</span>
        </button>
      </li>
      <li>
        <button
          class="nav-item-base w-full"
          class:active={$browseMode === 'albums'}
          onclick={() => handleBrowseModeChange('albums')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span>アルバム</span>
        </button>
      </li>
      <li>
        <button
          class="nav-item-base w-full"
          class:active={$browseMode === 'artists'}
          onclick={() => handleBrowseModeChange('artists')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
          <span>アーティスト</span>
        </button>
      </li>
      <li>
        <button
          class="nav-item-base w-full"
          class:active={$browseMode === 'genres'}
          onclick={() => handleBrowseModeChange('genres')}
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
          <span>ジャンル</span>
        </button>
      </li>
    </ul>
  </div>

  <!-- ライブラリセクション -->
  <div class="mb-6">
    <h2 class="section-title">ライブラリ</h2>
    <ul class="list-none m-0 p-0">
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
