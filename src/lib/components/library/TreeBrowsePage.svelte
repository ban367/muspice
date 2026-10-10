<!--
  @component TreeBrowsePage
  左に木の形の一覧、右にその項目の曲の一覧を出す画面（フォルダ別・年代別）。
  木と、選んだ項目の曲は、呼び出し側が全曲の一覧から計算して渡す。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Track } from '#lib/types/models.js';
  import type { TreeNode } from '#lib/utils/libraryViews.js';
  import type { TrackColumnId } from '#lib/utils/trackColumns.js';
  import TrackList from './TrackList.svelte';
  import TreeBrowser from './TreeBrowser.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    title: string;
    /** 見出しのアイコン（SVGの中身） */
    icon: Snippet;
    /** 木の根の項目（取得するまではnull） */
    nodes: TreeNode[] | null;
    selectedKey: string | null;
    onSelect: (key: string) => void;
    /** 選んだ項目の曲（選んでいなければnull） */
    tracks: Track[] | null;
    isLoading?: boolean;
    isError?: boolean;
    error?: Error | null;
    /** 最初から開いておく項目のキー */
    initiallyExpanded?: readonly string[];
    /** 曲の一覧の、列と並び順を覚える単位 */
    viewId: string;
    defaultColumns: readonly TrackColumnId[];
    /** 何も選んでいない時の案内 */
    prompt: string;
  }

  let {
    title,
    icon,
    nodes,
    selectedKey,
    onSelect,
    tracks,
    isLoading = false,
    isError = false,
    error = null,
    initiallyExpanded,
    viewId,
    defaultColumns,
    prompt
  }: Props = $props();
</script>

<div class="tree-page">
  <div class="page-header">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      class="header-icon"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      {@render icon()}
    </svg>
    <h1 class="page-title">{title}</h1>
    {#if tracks}
      <span class="text-sm text-text-muted">{m.common.trackCount(tracks.length)}</span>
    {/if}
  </div>

  {#if isLoading}
    <div class="empty-state">
      <div class="spinner"></div>
      <p>{m.common.loading}</p>
    </div>
  {:else if isError}
    <div class="empty-state text-error">
      <p>{m.common.errorOccurred}</p>
      <p class="text-sm text-text-muted">{error?.message || m.common.unknownError}</p>
    </div>
  {:else if !nodes || nodes.length === 0}
    <div class="empty-state">
      <p>{m.library.emptyLibrary}</p>
      <p class="text-sm text-text-dimmed">{m.library.emptyLibraryHint}</p>
    </div>
  {:else}
    <div class="panes">
      <div class="tree-pane">
        <TreeBrowser {nodes} {selectedKey} {onSelect} label={title} {initiallyExpanded} />
      </div>
      <div class="track-pane">
        {#if tracks}
          <!-- 項目を切り替えたら、選択ごと作り直す -->
          {#key selectedKey}
            <TrackList {tracks} {viewId} {defaultColumns} defaultSort={null} />
          {/key}
        {:else}
          <div class="empty-state">
            <p>{prompt}</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../../app.css";

  .tree-page {
    @apply flex flex-col h-full;
  }

  .page-header {
    @apply flex items-center gap-3 p-4 border-b border-border;
  }

  .header-icon {
    @apply w-6 h-6 text-primary;
  }

  .page-title {
    @apply text-xl font-bold text-text-primary m-0;
  }

  .panes {
    @apply flex flex-1 min-h-0;
  }

  .tree-pane {
    @apply w-64 shrink-0 min-h-0 border-r border-border;
  }

  .track-pane {
    @apply flex-1 min-w-0 min-h-0 px-4;
  }
</style>
