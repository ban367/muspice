<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

  // Props
  interface Props {
    artists: ArtistGroup[];
    selectedArtist: ArtistGroup | null;
    onSelect: (artist: ArtistGroup) => void;
  }

  let { artists, selectedArtist, onSelect }: Props = $props();

  // リアクティブなキャッシュを購読
  const cache = $derived($albumArtCache);

  // アーティストのアルバムアートを読み込み
  $effect(() => {
    // artistsが変更されたらそのアルバムアートを読み込む
    artists.forEach((artist) => {
      if (artist.representativeTrackId) {
        loadAlbumArt(artist.representativeTrackId);
      }
    });
  });

  // キャッシュからアルバムアートを取得
  function getArt(trackId: string): string | null {
    return cache[trackId] ?? null;
  }
</script>

<div class="artist-list">
  <!-- アーティストリスト -->
  {#each artists as artist (artist.name)}
    <button
      class="artist-item"
      class:active={selectedArtist?.name === artist.name}
      onclick={() => onSelect(artist)}
    >
      <div class="artist-avatar">
        <AlbumArt
          src={getArt(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
      </div>
      <MarqueeText text={artist.name} class="artist-name" />
    </button>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .artist-list {
    @apply flex flex-col h-full overflow-y-auto py-1;
  }

  .artist-item {
    @apply flex items-center gap-3 px-3 py-1.5 mx-2 border-none bg-transparent rounded-md cursor-pointer transition-colors duration-150 text-left;
  }

  .artist-item:hover {
    @apply bg-surface-hover;
  }

  .artist-item.active {
    @apply bg-surface-active;
  }

  .artist-avatar {
    @apply w-8 h-8 rounded-full overflow-hidden shrink-0;
  }

  :global(.artist-name) {
    @apply text-sm text-text-primary truncate flex-1;
  }
</style>
