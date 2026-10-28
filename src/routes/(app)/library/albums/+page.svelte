<!--
  アルバムの画面。グリッド表示はアルバムのカード、リスト表示はアルバムアーティストごとのアルバムと曲
  （左にアルバムアーティスト、右にそのアルバムアーティストのアルバムと曲。アーティストの画面と同じ形）。
  どのアルバムも、まとめたアーティスト（アルバムアーティスト。なければ曲のアーティスト）の下に1回だけ出る。
-->
<script lang="ts">
  import AlbumGrid from '#lib/components/library/AlbumGrid.svelte';
  import ArtistList from '#lib/components/library/ArtistList.svelte';
  import AlbumArtistDetail from '#lib/components/library/AlbumArtistDetail.svelte';
  import LibraryBrowsePage from '#lib/components/library/LibraryBrowsePage.svelte';
  import { useAlbumsQuery } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumSearchFields, groupAlbumsByArtist } from '#lib/utils/albumArtists.js';
  import { matchesSearchTerms, searchTerms } from '#lib/utils/searchText.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // クエリ
  const albumsQuery = useAlbumsQuery();
  const allAlbums = $derived(albumsQuery.data ?? []);

  // リスト表示の左の一覧（検索語に合うアルバムのある、アルバムアーティスト）
  // アルバム名でも探せるよう、アルバムを絞り込んでから分ける（右のペインも、合うアルバムだけを出す）
  const albumArtists = $derived.by(() => {
    const terms = searchTerms(ui.browseSearchQuery);
    const albums =
      terms.length === 0
        ? allAlbums
        : allAlbums.filter((album) => matchesSearchTerms(albumSearchFields(album), terms));
    return groupAlbumsByArtist(albums);
  });
</script>

<LibraryBrowsePage
  title={m.library.albums}
  formatCount={m.library.albumCount}
  formatCountSummary={m.library.albumSummary}
  searchPlaceholder={m.library.searchAlbums}
  emptyPrompt={m.library.selectAlbumArtist}
  items={albumArtists}
  count={allAlbums.length}
  getItemKey={(albumArtist) => albumArtist.key}
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
      <circle cx="12" cy="12" r="10" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  {/snippet}

  {#snippet gridView(displayMode)}
    <AlbumGrid {displayMode} />
  {/snippet}

  {#snippet listView(albumArtists, selectedAlbumArtist, onSelect)}
    <ArtistList
      artists={albumArtists}
      selectedArtist={selectedAlbumArtist}
      {onSelect}
      getKey={(albumArtist) => albumArtist.key}
      label={m.fields.albumArtist}
    />
  {/snippet}

  {#snippet detailView(albumArtist)}
    <AlbumArtistDetail {albumArtist} />
  {/snippet}
</LibraryBrowsePage>
