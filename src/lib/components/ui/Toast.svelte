<script lang="ts">
  import { notifications, type ErrorNotification } from '#lib/stores/error.svelte.js';
  import { fly } from 'svelte/transition';
  import { m } from '#lib/i18n/i18n.svelte.js';

  function getTypeClass(type: ErrorNotification['type']): string {
    switch (type) {
      case 'error':
        return 'bg-red-500 text-white';
      case 'warning':
        return 'bg-yellow-500 text-white';
      case 'info':
        return 'bg-blue-500 text-white';
      default:
        return 'bg-gray-500 text-white';
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
      default:
        return '•';
    }
  }

  function removeNotification(id: string) {
    notifications.remove(id);
  }
</script>

<div class="fixed top-4 right-4 z-50 flex flex-col gap-2 max-w-md">
  {#each notifications.items as notification (notification.id)}
    <div
      class="flex items-start gap-3 p-4 rounded-lg shadow-lg {getTypeClass(notification.type)}"
      transition:fly={{ x: 300, duration: 300 }}
    >
      <span class="text-xl font-bold flex-shrink-0">{getIcon(notification.type)}</span>
      <p class="flex-1 text-sm">{notification.message}</p>
      <button
        class="flex-shrink-0 hover:opacity-70 transition-opacity"
        onclick={() => removeNotification(notification.id)}
        aria-label={m.common.close}
      >
        ✕
      </button>
    </div>
  {/each}
</div>
