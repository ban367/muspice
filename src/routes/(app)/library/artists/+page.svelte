<script lang="ts">
  import type { ArtistGroup } from '$lib/types/models';
  import ArtistGrid from '$lib/components/library/ArtistGrid.svelte';
  import ArtistList from '$lib/components/library/ArtistList.svelte';
  import ArtistDetail from '$lib/components/library/ArtistDetail.svelte';
  import LibraryBrowsePage from '$lib/components/library/LibraryBrowsePage.svelte';
  import { useArtistsGroupedQuery } from '$lib/queries/tracks';

  // クエリ
  const artistsQuery = useArtistsGroupedQuery();
  const allArtists = $derived(artistsQuery.data ?? []);

  // 検索フィルター
  function filterArtist(artist: ArtistGroup, query: string): boolean {
    return artist.name.toLowerCase().includes(query);
  }
</script>

<LibraryBrowsePage
  title="アーティスト"
  countUnit="人"
  searchPlaceholder="アーティストを検索..."
  emptyPrompt="アーティストを選択してください"
  items={allArtists}
  filterFn={filterArtist}
  listPaneWidth="16rem"
>
  {#snippet emptyIcon()}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
    >
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  {/snippet}

  {#snippet gridView(displayMode)}
    <ArtistGrid {displayMode} />
  {/snippet}

  {#snippet listView(artists, selectedArtist, onSelect)}
    <ArtistList {artists} {selectedArtist} {onSelect} />
  {/snippet}

  {#snippet detailView(artist)}
    <ArtistDetail {artist} />
  {/snippet}
</LibraryBrowsePage>
