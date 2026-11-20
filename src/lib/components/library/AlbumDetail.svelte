<!--
  @component AlbumDetail
  グリッド表示の下の中身の領域に出す、1枚のアルバムの曲（ADR-097）。アルバムの画面と最近追加した曲の画面で使う。
  曲は、アルバム名とまとめたアーティストで取得し（`useAlbumTracksQuery`）、アーティストの画面と同じ形
  （`ArtistAlbumsView`。ディスクごとの区切り・選択・再生・ドラッグ・メニュー）で出す。見出しの名前がアルバム名の
  ため、アルバムの見出し（ジャケット画像・名前）は出さず、アルバムの情報とメニューを見出しに出す（`singleAlbum`。ADR-107）。
-->
<script lang="ts">
  import type { AlbumGroup, AlbumSummary } from '#lib/types/models.js';
  import { useAlbumTracksQuery } from '#lib/queries/tracks.js';
  import { albumKey } from '#lib/utils/albumKey.js';
  import ArtistAlbumsView from './ArtistAlbumsView.svelte';

  interface Props {
    album: AlbumSummary;
    /** 閉じるボタンの操作 */
    onClose: () => void;
  }

  let { album, onClose }: Props = $props();

  const tracksQuery = $derived(useAlbumTracksQuery(album.name, album.artist));

  // 曲を取得するまでは、見出しだけを出す（取得して1曲もなければ、アルバムの区切りも出さない）
  const albums = $derived.by((): AlbumGroup[] => {
    const tracks = tracksQuery.data ?? [];
    if (tracks.length === 0) return [];
    return [
      {
        name: album.name,
        sortName: album.sortName,
        artist: album.artist,
        trackCount: tracks.length,
        totalDuration: tracks.reduce((sum, track) => sum + (track.duration ?? 0), 0),
        representativeTrackId: album.representativeTrackId,
        tracks
      }
    ];
  });
</script>

<!-- 別のアルバムに切り替わったら、選択とスクロールを最初に戻す（データを取り直しただけでは戻さない） -->
{#key albumKey(album)}
  <ArtistAlbumsView
    title={album.name}
    {albums}
    artist={album.artist}
    compact
    singleAlbum
    {onClose}
  />
{/key}
