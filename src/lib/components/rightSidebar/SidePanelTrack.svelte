<!--
  @component SidePanelTrack
  右サイドバーのパネルの上部に出す、曲のカード（再生キュー・アルバム情報の再生中の曲、曲情報の選んでいる曲）。
  ジャケット画像（押すとアルバムアートの画面）・見出し・曲名・アーティスト・アルバム・お気に入りと評価を出す。
  どのパネルでも同じ形・同じ高さにする。曲名などの長さ・アルバムの有無・曲がない時でも高さを変えない
  （どの行も1行に収めて省略し、値のない行も高さを空ける）。
  評価とお気に入りは、ライブラリの曲の一覧（変更のたびに書き換わる）とお気に入りの一覧から調べる
  （再生キューの曲は、キューに入れた時点の内容のため）。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    useFavoriteTracksQuery,
    useSetRatingMutation,
    useTracksQuery
  } from '#lib/queries/tracks.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import AlbumArt from '../AlbumArt.svelte';
  import RatingStars from '../library/RatingStars.svelte';
  import FavoriteButton from '../library/FavoriteButton.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    /** カードの見出し（「再生中」「選択中の曲」） */
    label: string;
    /** 見出しに添える文（「ほか2曲」） */
    note?: string;
    /** 出す曲（ない時は、プレースホルダーの絵と`emptyText`を、同じ高さで出す） */
    track: Track | null;
    /** 曲がない時に、曲名の位置に出す文 */
    emptyText: string;
    /** ジャケット画像を押した時に、アルバムアートの画面で見せる曲（省略時は、この曲だけ） */
    artTracks?: Track[];
  }

  let { label, note, track, emptyText, artTracks }: Props = $props();

  const tracksQuery = useTracksQuery();
  const favoritesQuery = useFavoriteTracksQuery();
  const setRatingMutation = useSetRatingMutation();

  const rating = $derived(
    track ? (tracksQuery.data?.find((item) => item.id === track.id)?.rating ?? track.rating) : 0
  );
  const isFavorite = $derived(
    track
      ? favoritesQuery.data
        ? favoritesQuery.data.some((item) => item.id === track.id)
        : track.isFavorite
      : false
  );

  function openAlbumArt(current: Track) {
    albumArtDialog.open(artTracks && artTracks.length > 0 ? artTracks : [current]);
  }
</script>

<section class="track-card" aria-label={label}>
  {#if track}
    <button
      type="button"
      class="card-art"
      title={m.albumArtDialog.open}
      aria-label={m.albumArtDialog.open}
      onclick={() => openAlbumArt(track)}
    >
      <AlbumArt
        src={albumArtUrl(track.id)}
        alt={m.common.albumArt}
        rounded="md"
        placeholderType="music"
      />
    </button>
  {:else}
    <div class="card-art">
      <AlbumArt src={null} alt="" rounded="md" placeholderType="music" />
    </div>
  {/if}

  <div class="card-text">
    <p class="card-label">
      {label}{#if note}<span class="card-note">· {note}</span>{/if}
    </p>
    {#if track}
      <p class="card-title" title={track.title || track.fileName}>
        {track.title || track.fileName}
      </p>
      <p class="card-artist" title={track.artist ?? undefined}>
        {track.artist || m.common.unknownArtist}
      </p>
      <p class="card-album" title={track.album ?? undefined}>{track.album ?? ''}</p>
      <div class="card-actions">
        <FavoriteButton trackId={track.id} {isFavorite} size="medium" />
        <RatingStars
          {rating}
          onChange={(value) => setRatingMutation.mutateAsync({ trackId: track.id, rating: value })}
        />
      </div>
    {:else}
      <p class="card-title empty">{emptyText}</p>
    {/if}
  </div>
</section>

<style>
  @reference "../../../app.css";

  /* 高さは中身によらず決める（ジャケット画像の高さ＋上下の余白） */
  .track-card {
    @apply flex gap-3 h-28 px-4 py-3;
    /* アクセントカラーを薄くした色から透明へ */
    background: linear-gradient(
      to bottom,
      color-mix(in oklab, var(--color-primary) 12%, transparent),
      transparent
    );
  }

  .card-art {
    @apply w-22 h-22 shrink-0 p-0 border-none rounded-md overflow-hidden bg-transparent;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  button.card-art {
    @apply cursor-pointer transition-opacity;
  }

  button.card-art:hover {
    @apply opacity-85;
  }

  /* 各行の高さを決めておき、値のない行（アルバムのない曲）も同じ高さを空ける */
  .card-text {
    @apply flex flex-col flex-1 min-w-0;
  }

  .card-label {
    @apply m-0 h-3.5 leading-3.5 text-[0.625rem] font-semibold uppercase tracking-wide text-text-muted truncate;
  }

  .card-note {
    @apply ml-1 font-normal normal-case tracking-normal;
  }

  .card-title {
    @apply m-0 mt-0.5 h-5 leading-5 text-sm font-semibold text-text-primary truncate;
  }

  .card-title.empty {
    @apply font-normal text-text-dimmed;
  }

  .card-artist {
    @apply m-0 h-4 leading-4 text-xs text-text-secondary truncate;
  }

  .card-album {
    @apply m-0 h-4 leading-4 text-xs text-text-muted truncate;
  }

  .card-actions {
    @apply flex items-center gap-2 h-5 mt-auto;
  }
</style>
