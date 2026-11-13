<!--
  画面の読み込み・移動の間に起きたエラーと、存在しない画面の表示（ADR-089）。
  エラーの内容は、`hooks.client.ts`の`handleError`がログファイルに残す。
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import ErrorScreen from '#lib/components/ErrorScreen.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const isNotFound = $derived(page.status === 404);
</script>

{#if isNotFound}
  <ErrorScreen
    title={m.errors.screen.notFound}
    fullWindow
    onHome={() => void goto(resolve('library/songs'))}
  />
{:else}
  <ErrorScreen
    error={page.error?.message ?? m.errors.unknown}
    fullWindow
    onReload={() => window.location.reload()}
    onHome={() => void goto(resolve('library/songs'))}
  />
{/if}
