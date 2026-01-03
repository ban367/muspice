<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import { loadAlbumArt, albumArtCache } from '$lib/stores/albumArtCache';
  import MarqueeText from '../MarqueeText.svelte';

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
        {#if getArt(artist.representativeTrackId)}
          <img src={getArt(artist.representativeTrackId)} alt={artist.name} loading="lazy" />
        {:else}
          <div class="avatar-placeholder">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
            >
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
              <circle cx="12" cy="7" r="4" />
            </svg>
          </div>
        {/if}
      </div>
      <MarqueeText text={artist.name} class="artist-name" />
    </button>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .artist-list {
    @apply flex flex-col h-full overflow-y-auto py-2;
  }

  .artist-item {
    @apply flex items-center gap-3 px-4 py-2 mx-2 border-none bg-transparent rounded-md cursor-pointer transition-colors duration-150 text-left;
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

  .artist-avatar img {
    @apply w-full h-full object-cover;
  }

  .avatar-placeholder {
    @apply w-full h-full flex items-center justify-center;
    background: linear-gradient(135deg, #3a3a4a, #2a2a3a);
  }

  .avatar-placeholder svg {
    @apply w-4 h-4 text-text-muted;
  }

  :global(.artist-name) {
    @apply text-sm text-text-primary truncate flex-1;
  }
</style>
