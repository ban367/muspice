<script lang="ts">
  import { tick } from 'svelte';
  import type { AlbumSummary } from '#lib/types/models.js';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { listSelectionTarget } from '#lib/utils/listNavigation.js';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    albums: AlbumSummary[];
    selectedAlbum: AlbumSummary | null;
    onSelect: (album: AlbumSummary) => void;
  }

  let { albums, selectedAlbum, onSelect }: Props = $props();

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_ROW_HEIGHT = 52;

  // 見えている行だけを描画する一覧
  let virtualList = $state<VirtualList<AlbumSummary>>();

  // ↑↓・Home・Endで、選択中のアルバムから前後のアルバムへ選択を移す
  async function handleKeydown(event: KeyboardEvent) {
    const list = event.currentTarget;
    const current = albums.findIndex((album) => album.name === selectedAlbum?.name);
    const target = listSelectionTarget(event, current, albums.length);
    if (target === null) return;

    onSelect(albums[target]);
    virtualList?.scrollToIndex(target);
    // 移動先の項目へフォーカスも移す（前の項目に残すと、EnterやSpaceが前の項目に効いてしまう）
    await tick();
    if (list instanceof HTMLElement) {
      list.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
    }
  }
</script>

<!-- Tabでは選択中のアルバムだけに止まり、一覧の中は矢印キーで移る -->
<VirtualList
  bind:this={virtualList}
  items={albums}
  getKey={(album) => album.name}
  estimatedRowHeight={ESTIMATED_ROW_HEIGHT}
  scrollerClass="py-1"
  class="outline-none"
  role="listbox"
  aria-label={m.library.albums}
  tabindex={-1}
  onkeydown={handleKeydown}
>
  {#snippet row(album)}
    <button
      class="album-item"
      class:active={selectedAlbum?.name === album.name}
      onclick={() => onSelect(album)}
      role="option"
      aria-selected={selectedAlbum?.name === album.name}
      tabindex={selectedAlbum?.name === album.name ? 0 : -1}
    >
      <div class="album-art">
        <AlbumArt src={albumArtUrl(album.representativeTrackId)} alt={album.name} rounded="sm" />
      </div>
      <div class="album-info">
        <MarqueeText text={album.name} class="album-name" />
        <span class="album-artist">{album.artist || m.common.unknownArtist}</span>
      </div>
    </button>
  {/snippet}
</VirtualList>

<style>
  @reference "../../../app.css";

  /* 選択中の項目は背景の色で示すため、フォーカスの枠は出さない */
  .album-item {
    @apply flex items-center gap-3 px-3 py-1.5 mx-2 border-none bg-transparent rounded-md cursor-pointer transition-colors duration-150 text-left outline-none;
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
