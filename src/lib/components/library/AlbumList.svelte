<script lang="ts">
  import type { AlbumGroup } from '$lib/types/models';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

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
        <AlbumArt src={getArt(album.representativeTrackId)} alt={album.name} rounded="sm" />
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
