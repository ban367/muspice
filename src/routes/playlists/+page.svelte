<script lang="ts">
  import Playlist from '$lib/components/Playlist.svelte';
</script>

<div class="playlists-page">
  <Playlist />
</div>

<style>
  .playlists-page {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
</style>
