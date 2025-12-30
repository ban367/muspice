<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  // ルートページは /library/songs にリダイレクト
  onMount(() => {
    goto('/library/songs', { replaceState: true });
  });
</script>

<div class="redirect-page">
  <p>リダイレクト中...</p>
</div>

<style>
  .redirect-page {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #888;
  }
</style>
