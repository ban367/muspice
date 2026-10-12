<!--
  年代別の画面。年代 → 年をたどり、その年代・年の曲を表示する。
  木と曲は、取得済みの全曲の一覧から計算する。
-->
<script lang="ts">
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import TreeBrowsePage from '#lib/components/library/TreeBrowsePage.svelte';
  import { buildYearTree, tracksInYear } from '#lib/utils/libraryViews.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const tracksQuery = useTracksQuery();
  const allTracks = $derived(tracksQuery.data ?? null);
  const nodes = $derived(
    allTracks
      ? buildYearTree(allTracks, { decade: m.library.decade, unknown: m.library.unknownYear })
      : null
  );

  let selectedKey = $state<string | null>(null);
  const tracks = $derived(
    allTracks && selectedKey !== null ? tracksInYear(allTracks, selectedKey) : null
  );
</script>

<TreeBrowsePage
  title={m.library.years}
  {nodes}
  {selectedKey}
  onSelect={(key) => (selectedKey = key)}
  {tracks}
  isLoading={tracksQuery.isLoading}
  isError={tracksQuery.isError}
  error={tracksQuery.error}
  viewId="years"
  defaultColumns={['title', 'artist', 'album', 'year', 'duration']}
  prompt={m.library.selectYear}
>
  {#snippet icon()}
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2"
      d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
    />
  {/snippet}
</TreeBrowsePage>
