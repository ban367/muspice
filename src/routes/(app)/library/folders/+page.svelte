<!--
  フォルダ別の画面。ライブラリフォルダの中のフォルダをたどり、その中（下のフォルダを含む）の
  曲を表示する。木と曲は、取得済みの全曲の一覧の場所（パス）から計算する。
-->
<script lang="ts">
  import { useTracksQuery } from '#lib/queries/tracks.js';
  import { useLibraryFoldersQuery } from '#lib/queries/libraryFolders.js';
  import { combineQueryStates } from '#lib/queries/shared.js';
  import TreeBrowsePage from '#lib/components/library/TreeBrowsePage.svelte';
  import { buildFolderTree, tracksInFolder } from '#lib/utils/libraryViews.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const tracksQuery = useTracksQuery();
  const foldersQuery = useLibraryFoldersQuery();
  const queryState = $derived(combineQueryStates(tracksQuery, foldersQuery));

  const allTracks = $derived(tracksQuery.data ?? null);
  const libraryFolders = $derived(foldersQuery.data?.folders.map((folder) => folder.path) ?? null);
  const nodes = $derived(
    allTracks && libraryFolders ? buildFolderTree(allTracks, libraryFolders) : null
  );

  let selectedKey = $state<string | null>(null);
  const tracks = $derived(
    allTracks && selectedKey !== null ? tracksInFolder(allTracks, selectedKey) : null
  );
</script>

<TreeBrowsePage
  title={m.library.folders}
  {nodes}
  {selectedKey}
  onSelect={(key) => (selectedKey = key)}
  {tracks}
  isLoading={queryState.isLoading}
  isError={queryState.isError}
  error={queryState.error}
  initiallyExpanded={nodes?.map((node) => node.key)}
  viewId="folders"
  defaultColumns={['title', 'artist', 'album', 'duration']}
  prompt={m.library.selectFolder}
>
  {#snippet icon()}
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2"
      d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
    />
  {/snippet}
</TreeBrowsePage>
