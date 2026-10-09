<script lang="ts">
  import { tick } from 'svelte';
  import type { ArtistSummary } from '#lib/types/models.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { listSelectionTarget } from '#lib/utils/listNavigation.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

  // Props
  interface Props {
    artists: ArtistSummary[];
    selectedArtist: ArtistSummary | null;
    onSelect: (artist: ArtistSummary) => void;
  }

  let { artists, selectedArtist, onSelect }: Props = $props();

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_ROW_HEIGHT = 44;

  // 見えている行だけを描画する一覧
  let virtualList = $state<VirtualList<ArtistSummary>>();

  // ↑↓・Home・Endで、選択中のアーティストから前後のアーティストへ選択を移す
  async function handleKeydown(event: KeyboardEvent) {
    const list = event.currentTarget;
    const current = artists.findIndex((artist) => artist.name === selectedArtist?.name);
    const target = listSelectionTarget(event, current, artists.length);
    if (target === null) return;

    onSelect(artists[target]);
    virtualList?.scrollToIndex(target);
    // 移動先の項目へフォーカスも移す（前の項目に残すと、EnterやSpaceが前の項目に効いてしまう）
    await tick();
    if (list instanceof HTMLElement) {
      list.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
    }
  }
</script>

<!-- Tabでは選択中のアーティストだけに止まり、一覧の中は矢印キーで移る -->
<VirtualList
  bind:this={virtualList}
  items={artists}
  getKey={(artist) => artist.name}
  estimatedRowHeight={ESTIMATED_ROW_HEIGHT}
  scrollerClass="py-1"
  class="outline-none"
  role="listbox"
  aria-label={m.library.artists}
  tabindex={-1}
  onkeydown={handleKeydown}
>
  {#snippet row(artist)}
    <button
      class="artist-item"
      class:active={selectedArtist?.name === artist.name}
      onclick={() => onSelect(artist)}
      role="option"
      aria-selected={selectedArtist?.name === artist.name}
      tabindex={selectedArtist?.name === artist.name ? 0 : -1}
    >
      <div class="artist-avatar">
        <AlbumArt
          src={albumArtUrl(artist.representativeTrackId)}
          alt={artist.name}
          rounded="full"
          placeholderType="person"
        />
      </div>
      <MarqueeText text={artist.name} class="artist-name" />
    </button>
  {/snippet}
</VirtualList>

<style>
  @reference "../../../app.css";

  .artist-item {
    @apply flex items-center gap-3 px-3 py-1.5 mx-2 border-none bg-transparent rounded-md cursor-pointer transition-colors duration-150 text-left outline-none;
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
