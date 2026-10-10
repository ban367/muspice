<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { events } from '#lib/bindings.js';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { TextPromptDialog, Toast } from '#lib/components/ui/index.js';
  import Player from '#lib/components/Player.svelte';
  import Sidebar from '#lib/components/Sidebar.svelte';
  import RightSidebar from '#lib/components/RightSidebar.svelte';
  import ImportDialog from '#lib/components/ImportDialog.svelte';
  import AboutDialog from '#lib/components/AboutDialog.svelte';
  import AlbumArtDialog from '#lib/components/AlbumArtDialog.svelte';
  import TagToolsDialog from '#lib/components/TagToolsDialog.svelte';
  import SmartPlaylistDialog from '#lib/components/SmartPlaylistDialog.svelte';
  import PlaylistInfoDialogHost from '#lib/components/PlaylistInfoDialogHost.svelte';
  import SettingsSync from '#lib/components/SettingsSync.svelte';
  import { ui, saveLastPage } from '#lib/stores/ui.svelte.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { tagToolsDialog } from '#lib/stores/tagTools.svelte.js';
  import { smartPlaylistDialog } from '#lib/stores/smartPlaylist.svelte.js';
  import { invalidateAllTrackQueries } from '#lib/queries/tracks.js';
  import { restoreTheme } from '#lib/utils/theme.js';
  import { restoreLanguage } from '#lib/i18n/i18n.svelte.js';
  import '../../app.css';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 設定を読み込むまでの間も、前回の言語・テーマで表示する（読み込んだ後は`SettingsSync`が反映する）
  restoreLanguage();
  restoreTheme();

  // 起動時に「前回開いていた画面」を開けるよう、開いた画面を記録する
  afterNavigate(({ to }) => {
    if (to) saveLastPage(to.url.pathname);
  });

  // サイドバーの開閉を切り替え
  function toggleSidebar() {
    ui.isSidebarOpen = !ui.isSidebarOpen;
  }

  // サイドバーを閉じる（オーバーレイクリック時）
  function closeSidebar() {
    ui.isSidebarOpen = false;
  }

  // メニューバーからのイベントをリッスン
  onMount(() => {
    // インポートダイアログを開くイベント
    const unlistenImport = events.openImportDialog.listen(() => {
      ui.isImportDialogOpen = true;
    });

    // サイドバー切替イベント
    const unlistenSidebar = events.toggleSidebar.listen(() => {
      ui.isSidebarOpen = !ui.isSidebarOpen;
    });

    // Aboutダイアログイベント
    const unlistenAbout = events.showAboutDialog.listen(() => {
      ui.isAboutDialogOpen = true;
    });

    // 設定ウィンドウでの再スキャン・ライブラリフォルダの削除で、ライブラリが変わった
    const unlistenLibrary = events.libraryChanged.listen(() => {
      invalidateAllTrackQueries(queryClient);
    });

    return () => {
      unlistenImport.then((fn) => fn());
      unlistenSidebar.then((fn) => fn());
      unlistenAbout.then((fn) => fn());
      unlistenLibrary.then((fn) => fn());
    };
  });

  // パフォーマンス最適化されたQueryClient設定
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 10 * 60 * 1000, // 10分間キャッシュを新鮮とみなす
        gcTime: 30 * 60 * 1000, // 30分間メモリに保持
        retry: 1, // 失敗時に1回だけリトライ
        refetchOnWindowFocus: false, // ウィンドウフォーカス時の自動再取得を無効化
        refetchOnMount: false, // マウント時の自動再取得を無効化（キャッシュがあれば使用）
        refetchOnReconnect: true, // ネットワーク再接続時は再取得
        networkMode: 'online' // オンライン時のみクエリを実行
      },
      mutations: {
        retry: 0, // ミューテーションは失敗時にリトライしない
        networkMode: 'online'
      }
    }
  });

  let { children } = $props();
</script>

<QueryClientProvider client={queryClient}>
  <Toast />
  <SettingsSync />
  <div class="app-container">
    <!-- モバイル用オーバーレイ -->
    {#if ui.isSidebarOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="sidebar-overlay" onclick={closeSidebar}></div>
    {/if}

    <!-- サイドバー -->
    <div class="sidebar-container" class:open={ui.isSidebarOpen}>
      <Sidebar />
    </div>

    <!-- メインコンテンツ -->
    <div
      class="main-container"
      class:sidebar-pinned={ui.isRightSidebarPinned && ui.isRightSidebarExpanded}
    >
      <!-- モバイル用ヘッダー -->
      <header class="mobile-header">
        <button class="menu-button" aria-label={m.sidebar.openMenu} onclick={toggleSidebar}>
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="icon">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 6h16M4 12h16M4 18h16"
              stroke="currentColor"
            ></path>
          </svg>
        </button>
        <span class="mobile-title">Muspice</span>
      </header>

      <!-- ページコンテンツ -->
      <main class="main-content">
        {@render children()}
      </main>

      <!-- プレイヤー -->
      <Player />
    </div>

    <!-- 右サイドバー（固定時は埋め込み表示） -->
    <RightSidebar />
  </div>

  <!-- インポートダイアログ -->
  <ImportDialog />

  <!-- Aboutダイアログ -->
  <AboutDialog />

  <!-- アルバムアートの画面（メニューなどから開く。開くたびに作り直す） -->
  {#if albumArtDialog.tracks}
    {#key albumArtDialog.tracks}
      <AlbumArtDialog tracks={albumArtDialog.tracks} onClose={() => albumArtDialog.close()} />
    {/key}
  {/if}

  <!-- タグの一括ツール（曲のメニューから開く。開くたびに作り直す） -->
  {#if tagToolsDialog.tracks}
    {#key tagToolsDialog.tracks}
      <TagToolsDialog tracks={tagToolsDialog.tracks} onClose={() => tagToolsDialog.close()} />
    {/key}
  {/if}

  <!-- 自動プレイリストの編集画面（サイドバー・プレイリストの画面から開く。開くたびに作り直す） -->
  {#if smartPlaylistDialog.target}
    {#key smartPlaylistDialog.target}
      <SmartPlaylistDialog
        playlist={smartPlaylistDialog.target.playlist}
        onClose={() => smartPlaylistDialog.close()}
      />
    {/key}
  {/if}

  <!-- プレイリストの情報（名前と説明）の編集画面 -->
  <PlaylistInfoDialogHost />

  <!-- テキスト入力ダイアログ（promptText()で表示） -->
  <TextPromptDialog />
</QueryClientProvider>

<style>
  @reference "../../app.css";

  .app-container {
    @apply flex h-screen overflow-hidden relative;
  }

  .sidebar-overlay {
    @apply hidden;
  }

  .sidebar-container {
    @apply w-64 shrink-0 h-full overflow-hidden;
  }

  .main-container {
    @apply flex-1 flex flex-col min-w-0 h-full overflow-hidden;
    transition: margin-right 0.2s ease;
  }

  /* 右サイドバー固定時のメインコンテンツ調整 */
  .main-container.sidebar-pinned {
    margin-right: 17rem; /* キューパネルの幅のみ（アイコンバーは常時固定表示で別） */
  }

  .mobile-header {
    @apply hidden items-center gap-3 py-3 px-4 bg-base-300 border-b border-border;
  }

  .menu-button {
    @apply flex items-center justify-center w-10 h-10 p-0 border-none bg-transparent text-text-primary cursor-pointer rounded-md;
  }

  .menu-button:hover {
    @apply bg-surface-active;
  }

  .menu-button .icon {
    @apply w-6 h-6;
  }

  .mobile-title {
    @apply text-xl font-bold text-text-primary;
  }

  .main-content {
    @apply flex-1 overflow-auto p-4 pb-player-height bg-base-100;
    padding-right: calc(1rem + 3rem); /* 右サイドバー分のスペース */
  }

  /* レスポンシブ対応 */
  @media (max-width: 1024px) {
    .sidebar-overlay {
      @apply block fixed inset-0 bg-black/50 z-40;
    }

    .sidebar-container {
      @apply fixed left-0 top-0 bottom-0 z-50 -translate-x-full transition-transform duration-300;
    }

    .sidebar-container.open {
      @apply translate-x-0;
    }

    .mobile-header {
      @apply flex;
    }
  }
</style>
