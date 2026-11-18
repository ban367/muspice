<!--
  @component ArtistDetail
  アーティストの画面のリスト表示の右のペインと、グリッド表示の下の中身の領域（`onClose`を渡す。ADR-097）。
  アーティストのアルバムごとの曲を表示する（`ArtistAlbumsView`）。
-->
<script lang="ts">
  import type { ArtistSummary } from '#lib/types/models.js';
  import { useArtistAlbumsQuery } from '#lib/queries/tracks.js';
  import ArtistAlbumsView from './ArtistAlbumsView.svelte';

  // Props
  interface Props {
    artist: ArtistSummary;
    /** グリッドの下の領域で出す時の、閉じるボタンの操作（渡すと、見出しを低くして閉じるボタンを出す） */
    onClose?: () => void;
  }

  let { artist, onClose }: Props = $props();

  // アーティストのアルバムと曲
  const artistName = $derived(artist.name);
  const albumsQuery = $derived(useArtistAlbumsQuery(artistName));
  const albums = $derived(albumsQuery.data ?? []);
</script>

<!-- 別のアーティストに切り替わったら、選択とスクロールを最初に戻す（データを取り直しただけでは戻さない） -->
{#key artistName}
  <ArtistAlbumsView
    title={artist.name}
    {albums}
    artist={artist.name}
    menuArtist={artist}
    compact={onClose !== undefined}
    {onClose}
  />
{/key}
