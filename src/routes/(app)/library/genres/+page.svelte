<script lang="ts">
  import GenreGrid from '#lib/components/library/GenreGrid.svelte';
  import LibraryBrowsePage from '#lib/components/library/LibraryBrowsePage.svelte';
  import { useGenresGroupedQuery } from '#lib/queries/tracks.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // クエリ
  const genresQuery = useGenresGroupedQuery();
  const genreCount = $derived(genresQuery.data?.length ?? 0);
</script>

<LibraryBrowsePage
  title={m.library.genres}
  formatCount={m.library.genreCount}
  formatCountSummary={m.library.genreSummary}
  searchPlaceholder={m.library.searchGenres}
  initialDisplayMode="grid"
  count={genreCount}
  enableCardSizeSlider={false}
>
  {#snippet gridView(displayMode)}
    <GenreGrid {displayMode} />
  {/snippet}
</LibraryBrowsePage>
