<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { Toast } from '#lib/components/ui/index.js';
  import { restoreTheme } from '#lib/utils/theme.js';
  import { restoreLanguage } from '#lib/i18n/i18n.svelte.js';
  import '../../app.css';

  // 設定を読み込むまでの間も、前回の言語・テーマで表示する
  restoreLanguage();
  restoreTheme();

  let { children } = $props();

  // 設定ウィンドウはメインウィンドウとは別のWebViewのため、独自のQueryClientを持つ
  const queryClient = new QueryClient();
</script>

<QueryClientProvider client={queryClient}>
  <Toast />
  <div class="settings-layout">
    {@render children()}
  </div>
</QueryClientProvider>

<style>
  @reference "../../app.css";

  .settings-layout {
    @apply h-screen bg-base-100 text-text-primary overflow-hidden;
  }
</style>
