<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { Toast } from '$lib/components/ui';
  import Player from '$lib/components/Player.svelte';
  import '../app.css';

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
  <div class="drawer lg:drawer-open">
    <input id="sidebar-drawer" type="checkbox" class="drawer-toggle" />
    <div class="drawer-content flex flex-col">
      <!-- ページコンテンツ -->
      <div class="navbar bg-base-200 lg:hidden">
        <div class="flex-none">
          <label for="sidebar-drawer" class="btn btn-square btn-ghost">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              class="inline-block h-5 w-5 stroke-current"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M4 6h16M4 12h16M4 18h16"
              ></path>
            </svg>
          </label>
        </div>
        <div class="flex-1">
          <span class="text-xl font-bold">Muspice</span>
        </div>
      </div>
      <main class="flex-1 p-4 pb-32">
        {@render children()}
      </main>
      <!-- プレイヤーコンポーネント -->
      <Player />
    </div>
    <div class="drawer-side">
      <label for="sidebar-drawer" aria-label="close sidebar" class="drawer-overlay"></label>
      <aside class="bg-base-200 min-h-full w-64 p-4">
        <!-- サイドバーコンテンツ -->
        <div class="mb-8">
          <h1 class="text-2xl font-bold">Muspice</h1>
        </div>
        <ul class="menu gap-2">
          <li>
            <a href="/" class="flex items-center gap-3">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-5 w-5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
                />
              </svg>
              <span>ライブラリ</span>
            </a>
          </li>
          <li>
            <a href="/playlists" class="flex items-center gap-3">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-5 w-5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M4 6h16M4 10h16M4 14h16M4 18h16"
                />
              </svg>
              <span>プレイリスト</span>
            </a>
          </li>
        </ul>
      </aside>
    </div>
  </div>
</QueryClientProvider>
