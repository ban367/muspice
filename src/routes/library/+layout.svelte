<script lang="ts">
  let { children } = $props();
</script>

<div class="library-layout">
  {@render children()}
</div>

<style>
@reference "../../app.css";
  .library-layout {
    @apply flex flex-col h-full overflow-hidden;
  }
</style>
