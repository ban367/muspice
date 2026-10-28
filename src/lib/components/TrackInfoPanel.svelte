<!--
  @component TrackInfoPanel
  右サイドバーの曲情報パネル。一覧で選んでいる曲（複数選んでいる時は、最後に選んだ曲）の情報を出す。
  選択に追随し、選んでいない時は何も出さない（`ui.selectedTrack`。ADR-078）。
  上部に曲名・アーティスト・アルバム・お気に入りと評価を固定して出し、その下に、タグ・ファイル・再生の記録の
  項目をスクロールして見られるように出す（値のない項目も、ないことがわかるよう「—」で出す）。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { useSetRatingMutation, useShowInFolderMutation } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { albumArtDialog } from '#lib/stores/albumArt.svelte.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { trackCellText, trackColumnLabel, type TrackColumnId } from '#lib/utils/trackColumns.js';
  import AlbumArt from './AlbumArt.svelte';
  import RatingStars from './library/RatingStars.svelte';
  import FavoriteButton from './library/FavoriteButton.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  /** 項目のまとまり（見出しと、出す項目。見出しは`m.trackInfo`の名前） */
  const SECTIONS: { heading: 'tags' | 'file' | 'history'; fields: TrackColumnId[] }[] = [
    {
      heading: 'tags',
      fields: [
        'title',
        'artist',
        'album',
        'albumArtist',
        'genre',
        'year',
        'trackNumber',
        'discNumber'
      ]
    },
    {
      heading: 'file',
      fields: ['duration', 'format', 'bitrate', 'sampleRate', 'fileSize', 'filePath']
    },
    { heading: 'history', fields: ['playCount', 'skipCount', 'lastPlayedAt', 'createdAt'] }
  ];

  const selected = $derived(ui.selectedTrack);

  const setRatingMutation = useSetRatingMutation();
  const showInFolderMutation = useShowInFolderMutation();

  /** 項目の値（値がなければ「—」。タイトルは、ファイル名で補わずにタグの値を出す） */
  function fieldText(track: Track, field: TrackColumnId): string {
    const text = field === 'title' ? (track.title ?? '') : trackCellText(track, field);
    return text || '—';
  }
</script>

<div class="panel-header">
  <h3>{m.rightSidebar.trackInfoTitle}</h3>
</div>

{#if selected}
  {@const track = selected.track}
  <!-- 選んでいる曲（スクロールせず、常に見えるようにする） -->
  <section class="summary" aria-label={track.title || track.fileName}>
    <button
      type="button"
      class="summary-art"
      title={m.albumArtDialog.open}
      aria-label={m.albumArtDialog.open}
      onclick={() => albumArtDialog.open([track])}
    >
      <AlbumArt
        src={albumArtUrl(track.id)}
        alt={m.common.albumArt}
        rounded="md"
        placeholderType="music"
      />
    </button>
    <div class="summary-text">
      <h2 class="summary-title" title={track.title || track.fileName}>
        {track.title || track.fileName}
      </h2>
      <p class="summary-artist" title={track.artist ?? undefined}>
        {track.artist || m.common.unknownArtist}
      </p>
      {#if track.album}
        <p class="summary-album" title={track.album}>{track.album}</p>
      {/if}
      <div class="summary-actions">
        <FavoriteButton trackId={track.id} isFavorite={track.isFavorite} size="medium" />
        <RatingStars
          rating={track.rating}
          onChange={(value) => setRatingMutation.mutateAsync({ trackId: track.id, rating: value })}
        />
      </div>
    </div>
  </section>

  {#if selected.count > 1 || track.isMissing}
    <div class="notes">
      {#if selected.count > 1}
        <p class="note">{m.trackInfo.othersSelected(selected.count - 1)}</p>
      {/if}
      {#if track.isMissing}
        <p class="note warning">{m.common.fileMissing}</p>
      {/if}
    </div>
  {/if}

  <!-- 曲の項目（スクロールする） -->
  <div class="fields-body">
    {#each SECTIONS as section (section.heading)}
      <section class="field-section">
        <h4 class="field-heading">{m.trackInfo[section.heading]}</h4>
        <dl class="field-list">
          {#each section.fields as field (field)}
            <dt>{trackColumnLabel(field)}</dt>
            <dd class:path={field === 'filePath'}>
              <span class="field-value">{fieldText(track, field)}</span>
              {#if field === 'filePath'}
                <button
                  type="button"
                  class="field-action"
                  onclick={() => showInFolderMutation.mutate(track.id)}
                  disabled={track.isMissing}
                  title={m.contextMenu.showInFolder}
                  aria-label={m.contextMenu.showInFolder}
                >
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  >
                    <path
                      d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"
                    />
                  </svg>
                </button>
              {/if}
            </dd>
          {/each}
        </dl>
      </section>
    {/each}
  </div>
{:else}
  <p class="panel-message">{m.trackInfo.empty}</p>
{/if}

<style>
  @reference "../../app.css";

  .panel-header {
    @apply flex items-center px-4 py-3 border-b border-border shrink-0;
  }

  .panel-header h3 {
    @apply m-0 text-sm font-semibold text-text-primary;
  }

  .panel-message {
    @apply m-0 py-8 px-4 text-center text-sm text-text-dimmed;
  }

  /* 選んでいる曲（ジャケット画像・曲名・アーティスト・アルバム・お気に入りと評価） */
  .summary {
    @apply flex items-start gap-3 px-4 py-3 border-b border-border shrink-0;
  }

  .summary-art {
    @apply w-18 h-18 shrink-0 rounded-md overflow-hidden p-0 border-none bg-transparent cursor-pointer transition-opacity;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .summary-art:hover {
    @apply opacity-85;
  }

  .summary-text {
    @apply flex flex-col flex-1 min-w-0;
  }

  .summary-title {
    @apply m-0 text-sm font-bold leading-tight text-text-primary break-words line-clamp-2;
  }

  .summary-artist {
    @apply m-0 mt-1 text-xs text-text-secondary truncate;
  }

  .summary-album {
    @apply m-0 mt-0.5 text-xs text-text-muted truncate;
  }

  .summary-actions {
    @apply flex items-center gap-2 mt-1.5;
  }

  /* 複数選んでいる時・ファイルが見つからない時の知らせ */
  .notes {
    @apply flex flex-col gap-0.5 px-4 py-2 border-b border-border shrink-0;
  }

  .note {
    @apply m-0 text-xs text-text-muted;
  }

  .note.warning {
    @apply text-warning;
  }

  .fields-body {
    @apply flex-1 min-h-0 overflow-y-auto px-4 pb-3;
  }

  .field-heading {
    @apply m-0 mt-3 mb-1 text-[0.625rem] font-semibold uppercase text-text-muted;
  }

  /* 項目名と値を2列に並べる（項目名は、いちばん長い名前の幅にそろえる） */
  .field-list {
    @apply m-0 grid gap-x-3 gap-y-1 text-xs;
    grid-template-columns: max-content minmax(0, 1fr);
  }

  .field-list dt {
    @apply text-text-dimmed;
  }

  .field-list dd {
    @apply m-0 min-w-0 text-text-primary break-words;
  }

  /* 場所は長いため、どこででも折り返し、ファイルの場所を開くボタンを添える */
  .field-list dd.path {
    @apply flex items-start gap-1;
  }

  .field-list dd.path .field-value {
    @apply flex-1 min-w-0 break-all;
  }

  .field-action {
    @apply w-5 h-5 shrink-0 flex items-center justify-center p-0.5 border-none rounded bg-transparent
           text-text-secondary cursor-pointer transition-colors;
  }

  .field-action:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .field-action:disabled {
    @apply opacity-40 cursor-not-allowed;
  }

  .field-action svg {
    @apply w-3.5 h-3.5;
  }
</style>
