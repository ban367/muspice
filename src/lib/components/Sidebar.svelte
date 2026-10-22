<script lang="ts">
  import { stopListening } from '#lib/utils/listen.js';
  import { setOf } from '#lib/utils/collections.js';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { useQueryClient } from '@tanstack/svelte-query';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { useRegisterSyncDeviceMutation, useSyncDevicesQuery } from '#lib/queries/devices.js';
  import { queryKeys } from '#lib/queries/keys.js';
  import { showSuccess } from '#lib/stores/error.svelte.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { useGenresQuery } from '#lib/queries/tracks.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import type { SidebarItem } from '#lib/types/models.js';
  import SidebarPlaylists from './SidebarPlaylists.svelte';
  import MarqueeText from './MarqueeText.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // パス変更時にサイドバーを閉じる（モバイル用）
  let previousPath = $state('');
  $effect(() => {
    const path = page.url.pathname;
    if (previousPath && path !== previousPath) {
      // パスが変更されたらサイドバーを閉じる（モバイル幅の場合のみ）
      if (typeof window !== 'undefined' && window.innerWidth <= 1024) {
        ui.isSidebarOpen = false;
      }
    }
    previousPath = path;
  });

  // インポートダイアログを開く
  function handleOpenImportDialog() {
    ui.isImportDialogOpen = true;
  }

  // ジャンルクエリ
  // サイドバーに表示する項目（設定で隠した項目は出さない。設定を読み込むまでは、すべて出す）
  const settingsQuery = useSettingsQuery();
  const hiddenItems = $derived(setOf(settingsQuery.data?.hiddenSidebarItems ?? []));
  const isShown = (item: SidebarItem) => !hiddenItems.has(item);

  const genresQuery = useGenresQuery();
  const genres = $derived(genresQuery.data ?? []);

  // ジャンルが展開されているか
  let isGenreExpanded = $state(false);

  // 現在のパスからアクティブなページを判定
  const currentPath = $derived(page.url.pathname);

  // 転送先デバイス
  const devicesQuery = useSyncDevicesQuery();
  const registerDeviceMutation = useRegisterSyncDeviceMutation();
  const queryClient = useQueryClient();

  // ウィンドウに戻ったら、デバイスの接続の状態を読み直す（デバイスの抜き差しを反映する。
  // TanStack Queryの再取得はページの表示状態だけを見ており、ウィンドウを切り替えても動かないため）
  $effect(() => {
    const unlistenFocus = getCurrentWebviewWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void queryClient.invalidateQueries({ queryKey: queryKeys.syncDevices });
    });
    return () => {
      stopListening(unlistenFocus);
    };
  });

  /**
   * 転送先デバイスを追加する（フォルダを選ぶダイアログはRust側が開き、ドライブ・フォルダの
   * 名前で登録する。名前はデバイスのページで変えられる）
   */
  async function handleAddDevice() {
    try {
      const device = await registerDeviceMutation.mutateAsync();
      // 選ばずに閉じた
      if (!device) return;
      showSuccess(m.devices.added(device.name));
      goto(resolve(`devices/${device.id}`));
    } catch {
      // 失敗はミューテーション内でトースト通知済み
    }
  }

  // ジャンル展開トグル
  function toggleGenreExpand() {
    isGenreExpanded = !isGenreExpanded;
  }
</script>

<aside class="sidebar">
  <!-- サイドバーヘッダー -->
  <div class="sidebar-header">
    <h1 class="text-xl font-bold text-text-primary m-0">Muspice</h1>
    <button
      class="btn-icon w-7 h-7 p-0"
      title={m.sidebar.importFolder}
      onclick={handleOpenImportDialog}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-5 h-5"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="2"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
        />
      </svg>
    </button>
  </div>

  <!-- 項目の一覧（高さが足りない時は、全体をスクロールする） -->
  <div class="sidebar-scroll">
    <!-- ブラウズセクション -->
    <div class="nav-section">
      <h2 class="section-title">{m.sidebar.browse}</h2>
      <ul class="list-none m-0 p-0">
        {#if isShown('songs')}
          <li>
            <a
              href={resolve('library/songs')}
              class="nav-item-base"
              class:active={currentPath === '/library/songs'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M9 18V5l12-2v13" />
                <circle cx="6" cy="18" r="3" />
                <circle cx="18" cy="16" r="3" />
              </svg>
              <span>{m.sidebar.songs}</span>
            </a>
          </li>
        {/if}
        {#if isShown('albums')}
          <li>
            <a
              href={resolve('library/albums')}
              class="nav-item-base"
              class:active={currentPath === '/library/albums'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <circle cx="12" cy="12" r="3" />
              </svg>
              <span>{m.sidebar.albums}</span>
            </a>
          </li>
        {/if}
        {#if isShown('artists')}
          <li>
            <a
              href={resolve('library/artists')}
              class="nav-item-base"
              class:active={currentPath === '/library/artists'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                <circle cx="12" cy="7" r="4" />
              </svg>
              <span>{m.sidebar.artists}</span>
            </a>
          </li>
        {/if}
        {#if isShown('genres')}
          <li>
            <div class="genre-nav-container">
              <a
                href={resolve('library/genres')}
                class="nav-item-base flex-1"
                class:active={currentPath === '/library/genres'}
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="nav-icon"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
                  <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
                </svg>
                <span class="flex-1 text-left">{m.sidebar.genres}</span>
              </a>
              <button
                class="genre-expand-btn"
                class:active={isGenreExpanded}
                onclick={toggleGenreExpand}
                title={m.sidebar.expandGenres}
                aria-label={m.sidebar.expandGenres}
                aria-expanded={isGenreExpanded}
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
                      href={resolve(`library/genres/${encodeURIComponent(genre.name)}`)}
                      class="genre-item"
                      class:active={currentPath ===
                        `/library/genres/${encodeURIComponent(genre.name)}`}
                    >
                      <span class="genre-name">{genre.name}</span>
                      <span class="genre-count">{genre.trackCount}</span>
                    </a>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/if}
        {#if isShown('folders')}
          <li>
            <a
              href={resolve('library/folders')}
              class="nav-item-base"
              class:active={currentPath === '/library/folders'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                />
              </svg>
              <span>{m.sidebar.folders}</span>
            </a>
          </li>
        {/if}
        {#if isShown('years')}
          <li>
            <a
              href={resolve('library/years')}
              class="nav-item-base"
              class:active={currentPath === '/library/years'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                />
              </svg>
              <span>{m.sidebar.years}</span>
            </a>
          </li>
        {/if}
      </ul>
    </div>

    <!-- ライブラリセクション -->
    <div class="nav-section">
      <h2 class="section-title">{m.sidebar.library}</h2>
      <ul class="list-none m-0 p-0">
        {#if isShown('favorites')}
          <li>
            <a
              href={resolve('library/favorites')}
              class="nav-item-base"
              class:active={currentPath === '/library/favorites'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
                />
              </svg>
              <span>{m.sidebar.favorites}</span>
            </a>
          </li>
        {/if}
        {#if isShown('recentlyAdded')}
          <li>
            <a
              href={resolve('library/recent')}
              class="nav-item-base"
              class:active={currentPath === '/library/recent'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <span>{m.sidebar.recentlyAdded}</span>
            </a>
          </li>
        {/if}
        {#if isShown('playHistory')}
          <li>
            <a
              href={resolve('library/history')}
              class="nav-item-base"
              class:active={currentPath === '/library/history'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <span>{m.sidebar.playHistory}</span>
            </a>
          </li>
        {/if}
        {#if isShown('mostPlayed')}
          <li>
            <a
              href={resolve('library/mostplayed')}
              class="nav-item-base"
              class:active={currentPath === '/library/mostplayed'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6"
                />
              </svg>
              <span>{m.sidebar.mostPlayed}</span>
            </a>
          </li>
        {/if}
        {#if isShown('duplicates')}
          <li>
            <a
              href={resolve('library/duplicates')}
              class="nav-item-base"
              class:active={currentPath === '/library/duplicates'}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                />
              </svg>
              <span>{m.sidebar.duplicates}</span>
            </a>
          </li>
        {/if}
      </ul>
    </div>

    <!-- プレイリストセクション -->
    <SidebarPlaylists />
  </div>

  <!-- デバイスセクション（一覧をスクロールしても、いつも下の端に出す） -->
  <div class="devices-section">
    <div class="nav-section-header">
      <h2 class="text-xs font-semibold uppercase tracking-wider text-text-muted m-0">
        {m.sidebar.devices}
      </h2>
      <button class="btn-icon w-6 h-6 p-0" title={m.sidebar.addDevice} onclick={handleAddDevice}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="w-4 h-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 4v16m8-8H4"
          />
        </svg>
      </button>
    </div>

    <ul class="device-list">
      {#if devicesQuery.isLoading}
        <li class="nav-message">{m.common.loading}</li>
      {:else if devicesQuery.isError}
        <li class="nav-message text-error-light">{m.common.errorOccurred}</li>
      {:else if devicesQuery.data}
        {#each devicesQuery.data as device (device.id)}
          <li>
            <a
              href={resolve(`devices/${device.id}`)}
              class="nav-item-base"
              class:active={currentPath === `/devices/${device.id}`}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="nav-icon"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M7 3h7l4 4v13a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1z"
                />
                <path stroke-linecap="round" d="M9.5 6.5v3M12 6.5v3M14.5 8.5v1" />
              </svg>
              <MarqueeText text={device.name} class="flex-1" />
              <span
                class="device-status"
                class:connected={device.connected}
                title={device.connected ? m.devices.connected : m.devices.disconnected}
                role="img"
                aria-label={device.connected ? m.devices.connected : m.devices.disconnected}
              ></span>
            </a>
          </li>
        {/each}
        {#if devicesQuery.data.length === 0}
          <li class="nav-message">{m.sidebar.noDevices}</li>
        {/if}
      {/if}
    </ul>
  </div>
</aside>

<style>
  @reference "../../app.css";

  /* 下の端は、再生バーの高さだけ空ける（再生バーは画面の幅いっぱいに重なる） */
  .sidebar {
    @apply flex flex-col h-full bg-base-200 text-text-secondary;
    padding-bottom: var(--spacing-player-height);
  }

  .sidebar-header {
    @apply flex items-center justify-between shrink-0 px-4 pt-3 pb-2;
  }

  /* プレイリストの一覧が、残りの高さを使う。
     高さが足りない時は、プレイリストの一覧を最小の高さで止め、全体をスクロールする */
  .sidebar-scroll {
    @apply flex-1 min-h-0 flex flex-col overflow-y-auto px-3;
  }

  .nav-section {
    @apply shrink-0 mb-3;
  }

  .nav-section .section-title {
    @apply mb-1;
  }

  .devices-section {
    @apply shrink-0 px-3 pt-2 pb-2 border-t border-border;
  }

  /* デバイスが多い時は、この中でスクロールする */
  .device-list {
    @apply list-none m-0 p-0 max-h-28 overflow-y-auto;
  }

  .genre-nav-container {
    @apply flex items-center;
  }

  .genre-expand-btn {
    @apply flex items-center justify-center w-7 h-7 rounded text-text-muted bg-transparent border-none cursor-pointer transition-colors duration-150;
  }

  .genre-expand-btn:hover {
    @apply bg-surface-hover text-text-primary;
  }

  .genre-expand-btn.active {
    @apply text-primary;
  }

  .genre-sublist {
    @apply list-none m-0 p-0 ml-4 my-0.5 border-l border-border;
  }

  .genre-item {
    @apply flex items-center w-full px-2.5 py-0.5 text-[0.8125rem] text-text-secondary bg-transparent border-none cursor-pointer transition-colors duration-150 text-left;
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

  /* デバイスの接続の状態（接続中は緑） */
  .device-status {
    @apply w-2 h-2 shrink-0 ml-2 rounded-full bg-text-dimmed;
  }

  .device-status.connected {
    @apply bg-success;
  }
</style>
