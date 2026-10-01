<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    class?: string;
    children?: Snippet;
  }

  let { title, class: className = '', children }: Props = $props();

  const classes = $derived(`card bg-base-100 shadow-xl ${className}`);
</script>

<div class={classes}>
  <div class="card-body">
    {#if title}
      <h2 class="card-title">{title}</h2>
    {/if}
    {@render children?.()}
  </div>
</div>
