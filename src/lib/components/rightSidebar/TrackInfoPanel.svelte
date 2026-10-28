<!--
  @component TrackInfoPanel
  右サイドバーの曲情報パネル。一覧で選んでいる曲（複数選んでいる時は、最後に選んだ曲）の情報を出す。
  選択に追随し、選んでいない時は何も出さない（`ui.selectedTrack`。ADR-078）。
  上部に曲のカード（`SidePanelTrack`。複数選んでいる時は、ほかに選んでいる曲の数を添える）を固定して出し、その下に、
  タグ・ファイル・再生の記録の項目をスクロールして見られるように出す（値のない項目も、ないことがわかるよう「—」で出す）。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { useShowInFolderMutation } from '#lib/queries/tracks.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { trackCellText, trackColumnLabel, type TrackColumnId } from '#lib/utils/trackColumns.js';
  import SidePanel from './SidePanel.svelte';
  import SidePanelMessage from './SidePanelMessage.svelte';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelTrack from './SidePanelTrack.svelte';
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

  const showInFolderMutation = useShowInFolderMutation();

  /** 項目の値（値がなければ「—」。タイトルは、ファイル名で補わずにタグの値を出す） */
  function fieldText(track: Track, field: TrackColumnId): string {
    const text = field === 'title' ? (track.title ?? '') : trackCellText(track, field);
    return text || '—';
  }
</script>

<SidePanel title={m.rightSidebar.trackInfoTitle}>
  {#snippet top()}
    <!-- 選んでいる曲（スクロールせず、常に見えるようにする） -->
    <SidePanelTrack
      label={m.rightSidebar.selectedTrack}
      note={selected && selected.count > 1
        ? m.trackInfo.othersSelected(selected.count - 1)
        : undefined}
      track={selected?.track ?? null}
      emptyText={m.rightSidebar.nothingSelected}
    />
  {/snippet}

  {#if selected}
    {@const track = selected.track}
    <!-- 曲の項目（スクロールする） -->
    {#each SECTIONS as section (section.heading)}
      <SidePanelSection title={m.trackInfo[section.heading]}>
        {#if section.heading === 'file' && track.isMissing}
          <p class="warning">{m.common.fileMissing}</p>
        {/if}
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
      </SidePanelSection>
    {/each}
  {:else}
    <SidePanelMessage text={m.trackInfo.empty} />
  {/if}
</SidePanel>

<style>
  @reference "../../../app.css";

  /* ファイルが見つからない曲の知らせ（ファイルの項目の上） */
  .warning {
    @apply m-0 mb-1.5 text-xs text-warning;
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
