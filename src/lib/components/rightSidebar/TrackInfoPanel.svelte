<!--
  @component TrackInfoPanel
  右サイドバーの選択中の曲のパネル。一覧で選んだ曲（複数選んでいる時は、最後に選んだ曲）の情報を出す。
  選択に追随し、選択を外した後・別の画面へ移った後も、最後に選んだ曲を出し続ける（`ui.selectedTrack`。
  ADR-078・ADR-096）。曲の内容は、全曲の一覧から最新のものを引く（ライブラリから消えた曲は出さない）。
  上部に曲のカード（`SidePanelTrack`。複数選んでいる時は、ほかに選んでいる曲の数を添える）を固定して出し、その下に、
  タグ・ファイル・再生の記録の項目をスクロールして見られるように出す（値のない項目も、ないことがわかるよう「—」で出す）。
  見出しには、メインビューで曲を選んだらこのパネルに切り替えるかのスイッチ（`ui.autoShowSelectedTrack`）と、
  出している曲を編集画面（別のウィンドウ。ADR-094）で開く「曲の情報を編集」を置く。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import { useShowInFolderMutation, useTracksQuery } from '#lib/queries/tracks.js';
  import { useOpenTrackEditorMutation } from '#lib/queries/trackEditor.js';
  import { ui } from '#lib/stores/ui.svelte.js';
  import { trackCellText, trackColumnLabel, type TrackColumnId } from '#lib/utils/trackColumns.js';
  import SidePanel from './SidePanel.svelte';
  import SidePanelButton from './SidePanelButton.svelte';
  import SidePanelMessage from './SidePanelMessage.svelte';
  import SidePanelSection from './SidePanelSection.svelte';
  import SidePanelSwitch from './SidePanelSwitch.svelte';
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

  const tracksQuery = useTracksQuery();

  const selected = $derived(ui.selectedTrack);

  /**
   * 出す曲（全曲の一覧の最新の内容。一覧を取得する前は、選んだ時の内容。ライブラリから消えた曲はnull）
   *
   * 選択を外した後は、選んだ一覧が曲を取り直しても届かないため、全曲の一覧から引く。
   */
  const track = $derived.by(() => {
    if (!selected) return null;
    const all = tracksQuery.data;
    if (!all) return selected.track;
    const id = selected.track.id;
    return all.find((item) => item.id === id) ?? null;
  });

  /** 曲のカードの見出し（選択を外した後は「最後に選んだ曲」） */
  const cardLabel = $derived(
    selected && selected.count === 0
      ? m.rightSidebar.lastSelectedTrack
      : m.rightSidebar.selectedTrack
  );

  const showInFolderMutation = useShowInFolderMutation();
  const openTrackEditorMutation = useOpenTrackEditorMutation();

  /** 項目の値（値がなければ「—」。タイトルは、ファイル名で補わずにタグの値を出す） */
  function fieldText(track: Track, field: TrackColumnId): string {
    const text = field === 'title' ? (track.title ?? '') : trackCellText(track, field);
    return text || '—';
  }
</script>

<SidePanel title={m.rightSidebar.trackInfoTitle}>
  {#snippet actions()}
    <SidePanelSwitch
      label={m.trackInfo.autoShow}
      hint={m.trackInfo.autoShowHint}
      bind:checked={ui.autoShowSelectedTrack}
    />
    <SidePanelButton
      label={m.trackInfo.edit}
      onclick={() => track && openTrackEditorMutation.mutate([track.id])}
      disabled={!track || openTrackEditorMutation.isPending}
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
        <path d="M12 20h9" />
        <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z" />
      </svg>
    </SidePanelButton>
  {/snippet}
  {#snippet top()}
    <!-- 選んでいる曲（スクロールせず、常に見えるようにする） -->
    <SidePanelTrack
      label={track ? cardLabel : m.rightSidebar.selectedTrack}
      note={track && selected && selected.count > 1
        ? m.trackInfo.othersSelected(selected.count - 1)
        : undefined}
      {track}
      emptyText={m.rightSidebar.nothingSelected}
    />
  {/snippet}

  {#if track}
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
