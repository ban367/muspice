<script lang="ts">
  import '../../app.css';
  let { children } = $props();
</script>

<div class="settings-layout">
  {@render children()}
</div>

<style>
  @reference "../../app.css";

  .settings-layout {
    @apply h-screen bg-base-100 text-text-primary overflow-hidden;
  }
</style>
