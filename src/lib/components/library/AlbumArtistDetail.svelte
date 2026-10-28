<!--
  @component AlbumArtistDetail
  アルバムの画面のリスト表示の右のペイン。アルバムアーティストのアルバムごとの曲を表示する（`ArtistAlbumsView`）。
  検索で絞り込んでいる間は、左の一覧と同じく、検索語に合うアルバムだけを出す。
-->
<script lang="ts">
  import { useAlbumArtistAlbumsQuery } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumSearchFields, type AlbumArtistEntry } from '#lib/utils/albumArtists.js';
  import { matchesSearchTerms, searchTerms } from '#lib/utils/searchText.js';
  import ArtistAlbumsView from './ArtistAlbumsView.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  // Props
  interface Props {
    albumArtist: AlbumArtistEntry;
  }

  let { albumArtist }: Props = $props();

  // アルバムアーティストのアルバムと曲
  const artistName = $derived(albumArtist.name);
  const albumsQuery = $derived(useAlbumArtistAlbumsQuery(artistName));
  const albums = $derived.by(() => {
    const all = albumsQuery.data ?? [];
    const terms = searchTerms(ui.browseSearchQuery);
    if (terms.length === 0) return all;
    return all.filter((album) => matchesSearchTerms(albumSearchFields(album), terms));
  });
</script>

<!-- 別のアルバムアーティストに切り替わったら、選択とスクロールを最初に戻す（データを取り直しただけでは戻さない） -->
{#key albumArtist.key}
  <ArtistAlbumsView title={artistName ?? m.common.unknownArtist} {albums} artist={artistName} />
{/key}
