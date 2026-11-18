<!--
  @component SidePanelTrackDetails
  右サイドバーのアルバム情報パネルで、ジャケット画像のカードとアルバムの曲の間に出す、曲の情報（ADR-096）。
  曲名・アーティストと、タグ（トラック番号・ディスク・ジャンル・年）・ファイル（時間・形式・サンプルレート・
  ビットレート）・再生の記録（再生回数・最終再生日）を、それぞれ1行にまとめて出す。
  高さは中身によらず一定にする（どの行も1行に収めて省略し、値のない行も高さを空ける）。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { trackCellText } from '#lib/utils/trackColumns.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    track: Track;
    /** 見出しに添える文（「再生中」「選択中」） */
    note: string;
  }

  let { track, note }: Props = $props();

  /** 値のある項目を「 · 」でつなぐ */
  function joinParts(parts: (string | null | undefined | false)[]): string {
    return parts.filter(Boolean).join(' · ');
  }

  const tagsLine = $derived(
    joinParts([
      track.trackNumber !== null && m.albumInfo.trackNumber(track.trackNumber),
      track.discNumber !== null && m.common.disc(track.discNumber),
      track.genre,
      trackCellText(track, 'year')
    ])
  );

  const fileLine = $derived(
    joinParts([
      trackCellText(track, 'duration'),
      trackCellText(track, 'format'),
      trackCellText(track, 'sampleRate'),
      trackCellText(track, 'bitrate')
    ])
  );

  const historyLine = $derived(
    joinParts([
      m.albumInfo.playCount(track.playCount),
      track.lastPlayedAt && m.albumInfo.lastPlayed(trackCellText(track, 'lastPlayedAt'))
    ])
  );
</script>

<section class="track-details" aria-label={m.albumInfo.trackDetails}>
  <p class="details-label">
    {m.albumInfo.trackDetails}<span class="details-note">· {note}</span>
  </p>
  <p class="details-title" title={track.title || track.fileName}>
    {track.title || track.fileName}
  </p>
  <p class="details-line primary" title={track.artist ?? undefined}>
    {track.artist || m.common.unknownArtist}
  </p>
  <p class="details-line" title={tagsLine}>{tagsLine}</p>
  <p class="details-line" title={fileLine}>{fileLine}</p>
  <p class="details-line" title={historyLine}>{historyLine}</p>
</section>

<style>
  @reference "../../../app.css";

  /* 各行の高さを決めておき、値のない行も同じ高さを空ける */
  .track-details {
    @apply flex flex-col px-4 py-2.5 border-t border-border;
  }

  .details-label {
    @apply m-0 h-3.5 leading-3.5 text-[0.625rem] font-semibold uppercase tracking-wide text-text-muted truncate;
  }

  .details-note {
    @apply ml-1 font-normal normal-case tracking-normal;
  }

  .details-title {
    @apply m-0 mt-1 h-5 leading-5 text-[0.8125rem] font-semibold text-text-primary truncate;
  }

  .details-line {
    @apply m-0 h-4 leading-4 text-xs text-text-dimmed truncate;
  }

  .details-line.primary {
    @apply text-text-secondary;
  }
</style>
