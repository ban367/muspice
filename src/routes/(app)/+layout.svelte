<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { Toast } from '$lib/components/ui';
  import Player from '$lib/components/Player.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import RightSidebar from '$lib/components/RightSidebar.svelte';
  import ImportDialog from '$lib/components/ImportDialog.svelte';
  import AboutDialog from '$lib/components/AboutDialog.svelte';
  import { isSidebarOpen, isImportDialogOpen, isAboutDialogOpen } from '$lib/stores/ui';
  import type { ImportResult } from '$lib/types/models';
  import '../../app.css';

  // サイドバーの開閉を切り替え
  function toggleSidebar() {
    isSidebarOpen.update((v) => !v);
  }

  // サイドバーを閉じる（オーバーレイクリック時）
  function closeSidebar() {
    isSidebarOpen.set(false);
  }

  // メニューバーからのイベントをリッスン
  onMount(() => {
    // インポートダイアログを開くイベント
    const unlistenImport = listen('open-import-dialog', () => {
      isImportDialogOpen.set(true);
    });

    // サイドバー切替イベント
    const unlistenSidebar = listen('toggle-sidebar', () => {
      isSidebarOpen.update((v) => !v);
    });

    // Aboutダイアログイベント
    const unlistenAbout = listen('show-about-dialog', () => {
      isAboutDialogOpen.set(true);
    });

    return () => {
      unlistenImport.then((fn) => fn());
      unlistenSidebar.then((fn) => fn());
      unlistenAbout.then((fn) => fn());
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

  /**
   * インポート完了時の処理
   */
  function handleImportComplete(result: ImportResult) {
    console.log('インポート完了:', result);
    // トラック一覧を再取得
    queryClient.invalidateQueries({ queryKey: ['tracks'] });
    queryClient.invalidateQueries({ queryKey: ['albums'] });
    queryClient.invalidateQueries({ queryKey: ['artists'] });
    queryClient.invalidateQueries({ queryKey: ['genres'] });
  }
</script>

<QueryClientProvider client={queryClient}>
  <Toast />
  <div class="app-container">
    <!-- モバイル用オーバーレイ -->
    {#if $isSidebarOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="sidebar-overlay" onclick={closeSidebar}></div>
    {/if}

    <!-- サイドバー -->
    <div class="sidebar-container" class:open={$isSidebarOpen}>
      <Sidebar />
    </div>

    <!-- メインコンテンツ -->
    <div class="main-container">
      <!-- モバイル用ヘッダー -->
      <header class="mobile-header">
        <button class="menu-button" aria-label="メニューを開く" onclick={toggleSidebar}>
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

    <!-- 右サイドバー -->
    <RightSidebar />
  </div>

  <!-- インポートダイアログ -->
  <ImportDialog onImportComplete={handleImportComplete} />

  <!-- Aboutダイアログ -->
  <AboutDialog />
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
  }

  .mobile-header {
    @apply hidden items-center gap-3 py-3 px-4 bg-base-300 border-b border-border;
  }

  .menu-button {
    @apply flex items-center justify-center w-10 h-10 p-0 border-none bg-transparent text-white cursor-pointer rounded-md;
  }

  .menu-button:hover {
    @apply bg-surface-active;
  }

  .menu-button .icon {
    @apply w-6 h-6;
  }

  .mobile-title {
    @apply text-xl font-bold text-white;
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
