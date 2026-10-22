<script lang="ts">
  import { notifications, type ErrorNotification } from '#lib/stores/error.svelte.js';
  import { fly } from 'svelte/transition';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // 色の付いた面の上の文字は、どちらのテーマでも同じ色にする（警告の黄色の上は、白より黒が読める）
  function getTypeClass(type: ErrorNotification['type']): string {
    switch (type) {
      case 'error':
        return 'bg-error text-white';
      case 'warning':
        return 'bg-warning text-black';
      case 'info':
        return 'bg-primary text-primary-content';
    }
  }

  function getIcon(type: ErrorNotification['type']): string {
    switch (type) {
      case 'error':
        return '✕';
      case 'warning':
        return '⚠';
      case 'info':
        return 'ℹ';
    }
  }

  function removeNotification(id: string) {
    notifications.remove(id);
  }

  // スクリーンリーダーは、すでにある読み上げの領域に足した内容だけを読むため、領域は常に置いておく。
  // エラーはすぐに読み（alert）、ほかは読み上げ中の内容の後に読む（status）
  const errors = $derived(notifications.items.filter((item) => item.type === 'error'));
  const others = $derived(notifications.items.filter((item) => item.type !== 'error'));
</script>

{#snippet toast(notification: ErrorNotification)}
  <div
    class="flex items-start gap-3 p-4 rounded-lg shadow-lg {getTypeClass(notification.type)}"
    transition:fly={{ x: 300, duration: 300 }}
  >
    <span class="text-xl font-bold flex-shrink-0" aria-hidden="true">
      {getIcon(notification.type)}
    </span>
    <p class="flex-1 text-sm">{notification.message}</p>
    <button
      class="flex-shrink-0 hover:opacity-70 transition-opacity"
      onclick={() => removeNotification(notification.id)}
      aria-label={m.common.close}
    >
      ✕
    </button>
  </div>
{/snippet}

<div class="fixed top-4 right-4 z-50 flex flex-col max-w-md">
  <div class="flex flex-col gap-2" role="alert">
    {#each errors as notification (notification.id)}
      {@render toast(notification)}
    {/each}
  </div>
  <div class="flex flex-col gap-2" class:mt-2={errors.length > 0} role="status">
    {#each others as notification (notification.id)}
      {@render toast(notification)}
    {/each}
  </div>
</div>
