<!--
  すべてのウィンドウ（メイン・設定）の土台。
  描画・`$effect`の中の例外で、ウィンドウが壊れたまま残らないよう、エラー境界で受け止めて、
  読み込み直せる画面を出す（ADR-089）。ウィンドウごとの中身は、`(app)`・`settings`のレイアウトが作る。
-->
<script lang="ts">
  import ErrorScreen from '#lib/components/ErrorScreen.svelte';
  import { restoreTheme } from '#lib/utils/theme.js';
  import { restoreLanguage } from '#lib/i18n/i18n.svelte.js';
  import { reportError } from '#lib/utils/errorLog.js';
  import '../app.css';

  // エラーの画面も、前回の言語・テーマで表示する
  restoreLanguage();
  restoreTheme();

  let { children } = $props();
</script>

<svelte:boundary onerror={(error) => reportError(error, 'Rendering error (window)')}>
  {@render children()}

  {#snippet failed(error)}
    <ErrorScreen {error} fullWindow onReload={() => window.location.reload()} />
  {/snippet}
</svelte:boundary>
