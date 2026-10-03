<script lang="ts">
  import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
  import { Toast } from '#lib/components/ui/index.js';
  import '../../app.css';

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
