<!--
  曲の情報の編集画面のウィンドウ（`track-editor`。Rustの`open_track_editor`が開く。ADR-094）
-->
<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { Toast } from '#lib/components/ui/index.js';
  import { restoreTheme } from '#lib/utils/theme.js';
  import { restoreLanguage } from '#lib/i18n/i18n.svelte.js';
  import { LOCAL_NETWORK_MODE } from '#lib/queries/shared.js';
  import '../../app.css';

  // メインウィンドウと同じ言語・テーマで表示する
  restoreLanguage();
  restoreTheme();

  let { children } = $props();

  // 編集画面はメインウィンドウとは別のWebViewのため、独自のQueryClientを持つ
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { refetchOnReconnect: false, refetchOnWindowFocus: false, ...LOCAL_NETWORK_MODE },
      mutations: { retry: 0, ...LOCAL_NETWORK_MODE }
    }
  });
</script>

<QueryClientProvider client={queryClient}>
  <Toast />
  {@render children()}
</QueryClientProvider>
