<script lang="ts">
  import GenreGrid from '$lib/components/library/GenreGrid.svelte';
  import LibraryBrowsePage from '$lib/components/library/LibraryBrowsePage.svelte';
  import { useGenresGroupedQuery } from '$lib/queries/tracks';

  // クエリ
  const genresQuery = useGenresGroupedQuery();
  const genreCount = $derived(genresQuery.data?.length ?? 0);
</script>

<LibraryBrowsePage
  title="ジャンル"
  countUnit="種類"
  searchPlaceholder="ジャンルを検索..."
  initialDisplayMode="grid"
  count={genreCount}
  enableCardSizeSlider={false}
>
  {#snippet gridView(displayMode)}
    <GenreGrid {displayMode} />
  {/snippet}
</LibraryBrowsePage>
