<!--
  @component ArtistDetail
  アーティストの画面のリスト表示の右のペイン。アーティストのアルバムごとの曲を表示する（`ArtistAlbumsView`）。
-->
<script lang="ts">
  import type { ArtistSummary } from '#lib/types/models.js';
  import { useArtistAlbumsQuery } from '#lib/queries/tracks.js';
  import ArtistAlbumsView from './ArtistAlbumsView.svelte';

  // Props
  interface Props {
    artist: ArtistSummary;
  }

  let { artist }: Props = $props();

  // アーティストのアルバムと曲
  const artistName = $derived(artist.name);
  const albumsQuery = $derived(useArtistAlbumsQuery(artistName));
  const albums = $derived(albumsQuery.data ?? []);
</script>

<!-- 別のアーティストに切り替わったら、選択とスクロールを最初に戻す（データを取り直しただけでは戻さない） -->
{#key artistName}
  <ArtistAlbumsView title={artist.name} {albums} artist={artist.name} menuArtist={artist} />
{/key}
