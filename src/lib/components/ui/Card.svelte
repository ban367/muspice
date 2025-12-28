<script lang="ts">
  interface Props {
    title?: string;
    class?: string;
    children?: any;
  }

  let { title, class: className = '', children }: Props = $props();

  const classes = `card bg-base-100 shadow-xl ${className}`;
</script>

<div class={classes}>
  <div class="card-body">
    {#if title}
      <h2 class="card-title">{title}</h2>
    {/if}
    {@render children?.()}
  </div>
</div>
