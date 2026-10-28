<!--
  @component ArtistList
  アーティストの一覧（2ペイン表示の左のペイン）。アーティストの画面のアーティストと、アルバムの画面の
  アルバムアーティストで使う。1つだけ選び、↑↓・Home・Endで選択を移す。
-->
<script lang="ts" generics="T extends { name: string | null; representativeTrackId: string }">
  import { tick } from 'svelte';
  import { VirtualList } from '#lib/components/ui/index.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { listSelectionTarget } from '#lib/utils/listNavigation.js';
  import { m } from '#lib/i18n/i18n.svelte.js';
  import MarqueeText from '../MarqueeText.svelte';
  import AlbumArt from '../AlbumArt.svelte';

  // Props
  interface Props {
    artists: T[];
    selectedArtist: T | null;
    onSelect: (artist: T) => void;
    /** 項目を識別するキー（省略時は名前。名前のない項目がある一覧では渡す） */
    getKey?: (artist: T) => string;
    /** 一覧の名前（スクリーンリーダー向け。省略時は「アーティスト」） */
    label?: string;
  }

  let {
    artists,
    selectedArtist,
    onSelect,
    getKey = (artist) => artist.name ?? '',
    label = m.library.artists
  }: Props = $props();

  // 行の高さの見積もり（描画した後は、VirtualListが実測した高さを使う）
  const ESTIMATED_ROW_HEIGHT = 44;

  // 見えている行だけを描画する一覧
  let virtualList = $state<VirtualList<T>>();

  // 選択中のアーティストのキー
  const selectedKey = $derived(selectedArtist ? getKey(selectedArtist) : null);

  // ↑↓・Home・Endで、選択中のアーティストから前後のアーティストへ選択を移す
  async function handleKeydown(event: KeyboardEvent) {
    const list = event.currentTarget;
    const current = artists.findIndex((artist) => getKey(artist) === selectedKey);
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
  {getKey}
  estimatedRowHeight={ESTIMATED_ROW_HEIGHT}
  scrollerClass="py-1"
  class="outline-none"
  role="listbox"
  aria-label={label}
  tabindex={-1}
  onkeydown={handleKeydown}
>
  {#snippet row(artist)}
    <button
      class="artist-item"
      class:active={getKey(artist) === selectedKey}
      onclick={() => onSelect(artist)}
      role="option"
      aria-selected={getKey(artist) === selectedKey}
      tabindex={getKey(artist) === selectedKey ? 0 : -1}
    >
      <div class="artist-avatar">
        <AlbumArt
          src={albumArtUrl(artist.representativeTrackId)}
          alt={artist.name ?? m.common.unknownArtist}
          rounded="full"
          placeholderType="person"
        />
      </div>
      <MarqueeText text={artist.name ?? m.common.unknownArtist} class="artist-name" />
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
