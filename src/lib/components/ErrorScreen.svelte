<!--
  @component ErrorScreen
  画面（か、その一部）を表示できなかった時に、代わりに出す（ADR-089）。
  エラー境界（`<svelte:boundary>`）と、SvelteKitのエラーの画面（`+error.svelte`）で使う。
  エラーの内容は、呼び出し側がログファイルに残す（ここでは、短い文だけを見せる）。
-->
<script lang="ts">
  import { m } from '#lib/i18n/i18n.svelte.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';

  interface Props {
    /** 起きたエラー（短い文にして見せる。ログの案内も出す）。なければ、見出しだけを出す */
    error?: unknown;
    /** 見出し（省略時は「画面を表示できませんでした」） */
    title?: string;
    /** ウィンドウ全体に出すか（falseは、ページの領域の中に収める） */
    fullWindow?: boolean;
    /** 「もう一度試す」（境界を作り直す）。なければ出さない */
    onRetry?: () => void;
    /** 「読み込み直す」（ウィンドウを読み込み直す）。なければ出さない */
    onReload?: () => void;
    /** 「曲の一覧へ戻る」。なければ出さない */
    onHome?: () => void;
  }

  let { error, title, fullWindow = false, onRetry, onReload, onHome }: Props = $props();

  const hasError = $derived(error !== undefined);
</script>

<div class="error-screen" class:full-window={fullWindow} role="alert">
  <h1 class="error-title">{title ?? m.errors.screen.title}</h1>
  {#if hasError}
    <p class="error-detail">{toErrorMessage(error)}</p>
    <p class="error-hint">{m.errors.screen.hint}</p>
  {/if}
  <div class="error-actions">
    {#if onRetry}
      <button type="button" class="btn-primary" onclick={onRetry}>{m.errors.screen.retry}</button>
    {/if}
    {#if onReload}
      <button type="button" class={onRetry ? 'btn-secondary' : 'btn-primary'} onclick={onReload}>
        {m.errors.screen.reload}
      </button>
    {/if}
    {#if onHome}
      <button type="button" class="btn-secondary" onclick={onHome}>{m.errors.screen.home}</button>
    {/if}
  </div>
</div>

<style>
  @reference "../../app.css";

  .error-screen {
    @apply h-full flex flex-col items-center justify-center gap-3 p-8 text-center;
    @apply bg-base-100 text-text-primary;
  }

  .error-screen.full-window {
    @apply h-screen;
  }

  .error-title {
    @apply text-lg font-semibold m-0;
  }

  .error-detail {
    @apply text-sm text-text-secondary m-0 max-w-xl break-words;
  }

  .error-hint {
    @apply text-xs text-text-muted m-0 max-w-xl;
  }

  .error-actions {
    @apply flex flex-wrap justify-center gap-2 mt-2;
  }
</style>
