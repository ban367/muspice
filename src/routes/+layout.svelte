<script lang="ts">
  import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/svelte-query';
  import { Toast } from '$lib/components/ui';
  import Player from '$lib/components/Player.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import ImportDialog from '$lib/components/ImportDialog.svelte';
  import { isSidebarOpen } from '$lib/stores/ui';
  import type { ImportResult } from '$lib/types/models';
  import '../app.css';

  // サイドバーの開閉を切り替え
  function toggleSidebar() {
    isSidebarOpen.update(v => !v);
  }

  // サイドバーを閉じる（オーバーレイクリック時）
  function closeSidebar() {
    isSidebarOpen.set(false);
  }

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
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" stroke="currentColor"></path>
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
  </div>

  <!-- インポートダイアログ -->
  <ImportDialog onImportComplete={handleImportComplete} />
</QueryClientProvider>

<style>
  .app-container {
    display: flex;
    height: 100vh;
    overflow: hidden;
    position: relative;
  }

  .sidebar-overlay {
    display: none;
  }

  .sidebar-container {
    width: 256px;
    flex-shrink: 0;
    height: 100%;
    overflow: hidden;
  }

  .main-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }

  .mobile-header {
    display: none;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background-color: #1e1e2e;
    border-bottom: 1px solid #333;
  }

  .menu-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    padding: 0;
    border: none;
    background: transparent;
    color: #fff;
    cursor: pointer;
    border-radius: 0.375rem;
  }

  .menu-button:hover {
    background-color: rgba(255, 255, 255, 0.1);
  }

  .menu-button .icon {
    width: 1.5rem;
    height: 1.5rem;
  }

  .mobile-title {
    font-size: 1.25rem;
    font-weight: 700;
    color: #fff;
  }

  .main-content {
    flex: 1;
    overflow: auto;
    padding: 1rem;
    padding-bottom: 8rem; /* プレイヤーの高さ分のパディング */
  }

  /* レスポンシブ対応 */
  @media (max-width: 1024px) {
    .sidebar-overlay {
      display: block;
      position: fixed;
      inset: 0;
      background-color: rgba(0, 0, 0, 0.5);
      z-index: 40;
    }

    .sidebar-container {
      position: fixed;
      left: 0;
      top: 0;
      bottom: 0;
      z-index: 50;
      transform: translateX(-100%);
      transition: transform 0.3s ease;
    }

    .sidebar-container.open {
      transform: translateX(0);
    }

    .mobile-header {
      display: flex;
    }
  }

  /* ダークモード */
  @media (prefers-color-scheme: dark) {
    .main-content {
      background-color: #0f0f1a;
    }
  }
</style>
