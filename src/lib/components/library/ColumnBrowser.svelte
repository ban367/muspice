<!--
  @component ColumnBrowser
  カラムブラウザ。ジャンル・アーティスト・アルバムの列を並べ、選んだ項目で曲の一覧を絞り込む。

  - 各列の先頭の「すべて」を選ぶと、その列の絞り込みを解除する
  - Cmd / Ctrl・Shift を押しながらクリックすると、複数の項目を選べる
  - 列の項目と件数は、`#lib/utils/trackFilter`の`browseTracks`が計算したものを受け取る
-->
<script lang="ts">
  import { VirtualList } from '#lib/components/ui/index.js';
  import {
    BROWSER_COLUMNS,
    clickBrowserItem,
    type BrowserColumn,
    type BrowserItem,
    type BrowserResult
  } from '#lib/utils/trackFilter.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** 各列の項目と、今の選択 */
    result: BrowserResult;
    /** 列の選択を変えた時に呼ばれる（その列の、新しい選択） */
    onSelect: (column: BrowserColumn, keys: string[]) => void;
  }

  let { result, onSelect }: Props = $props();

  const ROW_HEIGHT = 26;

  /** 「すべて」の行（一覧の先頭に出す） */
  const ALL = Symbol('all');
  type Row = BrowserItem | typeof ALL;

  const titles: Record<BrowserColumn, string> = {
    genres: m.fields.genre,
    artists: m.fields.artist,
    albums: m.fields.album
  };

  const unknownNames: Record<BrowserColumn, string> = {
    genres: m.common.unknownGenre,
    artists: m.common.unknownArtist,
    albums: m.common.unknownAlbum
  };

  const rowsOf = (column: BrowserColumn): Row[] => [ALL, ...result[column]];

  // 同じ名前のアルバムが複数ある場合は、見分けるためにアーティストも出す
  const duplicateAlbumNames = $derived.by(() => {
    const seen = new Set<string | null>();
    const duplicates = new Set<string | null>();
    for (const album of result.albums) {
      if (seen.has(album.name)) duplicates.add(album.name);
      seen.add(album.name);
    }
    return duplicates;
  });

  function handleClick(column: BrowserColumn, row: Row, event: MouseEvent) {
    if (row === ALL) {
      onSelect(column, []);
      return;
    }
    onSelect(
      column,
      clickBrowserItem(result[column], result.selection[column], row.key, {
        toggleKey: event.ctrlKey || event.metaKey,
        shiftKey: event.shiftKey
      })
    );
  }
</script>

<div class="column-browser">
  {#each BROWSER_COLUMNS as column (column)}
    {@const selected = result.selection[column]}
    <section class="browser-column" aria-label={titles[column]}>
      <h3 class="column-title">{titles[column]}</h3>
      <div class="column-list">
        <VirtualList
          items={rowsOf(column)}
          getKey={(row) => (row === ALL ? '\u0000all' : row.key)}
          estimatedRowHeight={ROW_HEIGHT}
          role="listbox"
          aria-multiselectable="true"
          aria-label={titles[column]}
        >
          {#snippet row(item)}
            {@const isSelected = item === ALL ? selected.length === 0 : selected.includes(item.key)}
            <button
              type="button"
              class="browser-row"
              class:selected={isSelected}
              class:all={item === ALL}
              style="height: {ROW_HEIGHT}px;"
              role="option"
              aria-selected={isSelected}
              onclick={(event) => handleClick(column, item, event)}
            >
              {#if item === ALL}
                <span class="row-name">{m.library.browserAll(result[column].length)}</span>
              {:else}
                <span class="row-name" class:unknown={item.name === null}>
                  {item.name ?? unknownNames[column]}
                  {#if column === 'albums' && duplicateAlbumNames.has(item.name)}
                    <span class="row-artist">{item.artist ?? m.common.unknownArtist}</span>
                  {/if}
                </span>
                <span class="row-count">{item.count}</span>
              {/if}
            </button>
          {/snippet}
        </VirtualList>
      </div>
    </section>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .column-browser {
    @apply grid gap-px bg-border border-b border-border shrink-0;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    height: 12rem;
  }

  .browser-column {
    @apply flex flex-col min-h-0 bg-base-100;
  }

  .column-title {
    @apply m-0 px-3 py-1 text-xs font-semibold uppercase text-text-muted border-b border-border;
  }

  .column-list {
    @apply flex-1 min-h-0;
  }

  .browser-row {
    @apply flex items-center gap-2 w-full px-3 bg-transparent border-none text-left text-sm text-text-secondary cursor-pointer select-none;
  }

  .browser-row:hover {
    @apply bg-surface;
  }

  .browser-row.selected {
    @apply bg-primary/20 text-text-primary;
  }

  .browser-row.all {
    @apply text-text-muted;
  }

  .row-name {
    @apply flex-1 min-w-0 truncate;
  }

  .row-name.unknown {
    @apply text-text-dimmed;
  }

  .row-artist {
    @apply ml-1 text-xs text-text-dimmed;
  }

  .row-count {
    @apply shrink-0 text-xs text-text-dimmed tabular-nums;
  }
</style>
