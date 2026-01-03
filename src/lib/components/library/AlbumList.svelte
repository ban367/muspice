<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import MarqueeText from '../MarqueeText.svelte';

  // Props
  interface Props {
    albums: AlbumGroup[];
    selectedAlbum: AlbumGroup | null;
    onSelect: (album: AlbumGroup) => void;
  }

  let { albums, selectedAlbum, onSelect }: Props = $props();

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // アルバムのアートを読み込み
  $effect(() => {
    albums.forEach((album) => {
      if (album.representativeTrackId) {
        loadAlbumArt(album.representativeTrackId);
      }
    });
  });

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }
</script>

<div class="album-list">
  <!-- アルバムリスト -->
  {#each albums as album (album.name)}
    <button
      class="album-item"
      class:active={selectedAlbum?.name === album.name}
      onclick={() => onSelect(album)}
    >
      <div class="album-art">
        {#if getArt(album.representativeTrackId)}
          <img src={getArt(album.representativeTrackId)} alt={album.name} loading="lazy" />
        {:else}
          <div class="art-placeholder">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
            >
              <circle cx="12" cy="12" r="10" />
              <circle cx="12" cy="12" r="3" />
            </svg>
          </div>
        {/if}
      </div>
      <div class="album-info">
        <MarqueeText text={album.name} class="album-name" />
        <span class="album-artist">{album.artist || '不明なアーティスト'}</span>
      </div>
    </button>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .album-list {
    @apply flex flex-col h-full overflow-y-auto py-2;
  }

  .album-item {
    @apply flex items-center gap-3 px-4 py-2 mx-2 border-none bg-transparent rounded-md cursor-pointer transition-colors duration-150 text-left;
  }

  .album-item:hover {
    @apply bg-surface-hover;
  }

  .album-item.active {
    @apply bg-surface-active;
  }

  .album-art {
    @apply w-10 h-10 rounded overflow-hidden shrink-0;
  }

  .album-art img {
    @apply w-full h-full object-cover;
  }

  .art-placeholder {
    @apply w-full h-full flex items-center justify-center;
    background: linear-gradient(135deg, #3a3a4a, #2a2a3a);
  }

  .art-placeholder svg {
    @apply w-5 h-5 text-text-muted;
  }

  .album-info {
    @apply flex flex-col gap-0.5 min-w-0 flex-1;
  }

  :global(.album-name) {
    @apply text-sm text-text-primary truncate;
  }

  .album-artist {
    @apply text-xs text-text-muted truncate;
  }
</style>
