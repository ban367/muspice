<script lang="ts">
  import type { AlbumSummary } from '#lib/types/models.js';
  import AlbumGrid from '#lib/components/library/AlbumGrid.svelte';
  import AlbumList from '#lib/components/library/AlbumList.svelte';
  import AlbumDetail from '#lib/components/library/AlbumDetail.svelte';
  import LibraryBrowsePage from '#lib/components/library/LibraryBrowsePage.svelte';
  import { useAlbumsQuery } from '#lib/queries/tracks.js';
  import { albumKey } from '#lib/utils/albumKey.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // クエリ
  const albumsQuery = useAlbumsQuery();
  const allAlbums = $derived(albumsQuery.data ?? []);

  // 絞り込みの対象（アルバム名・アーティスト）
  const albumSearchFields = (album: AlbumSummary) => [album.name, album.artist];
</script>

<LibraryBrowsePage
  title={m.library.albums}
  formatCount={m.library.albumCount}
  formatCountSummary={m.library.albumSummary}
  searchPlaceholder={m.library.searchAlbums}
  emptyPrompt={m.library.selectAlbum}
  items={allAlbums}
  searchFields={albumSearchFields}
  getItemKey={albumKey}
>
  {#snippet emptyIcon()}
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
  {/snippet}

  {#snippet gridView(displayMode)}
    <AlbumGrid {displayMode} />
  {/snippet}

  {#snippet listView(albums, selectedAlbum, onSelect)}
    <AlbumList {albums} {selectedAlbum} {onSelect} />
  {/snippet}

  {#snippet detailView(album)}
    <AlbumDetail {album} />
  {/snippet}
</LibraryBrowsePage>
