<!--
  @component TrackColumnMenu
  曲の一覧の、列を選ぶメニュー（列の見出しの右クリックで開く）。
  列の表示・非表示を切り替え、並び順・列を既定に戻す。切り替えてもメニューは閉じない
  （続けていくつかの列を切り替えられるようにするため）。
-->
<script lang="ts">
  import { BaseContextMenu } from '#lib/components/ui/index.js';
  import type { TrackListView } from '#lib/stores/trackListView.svelte.js';
  import { TRACK_COLUMN_IDS, toggleColumn, type TrackColumnId } from '#lib/utils/trackColumns.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    x: number;
    y: number;
    /** 列と並び順を持つ、画面の設定 */
    view: TrackListView;
    /** 列の表示名 */
    labels: Record<TrackColumnId, string>;
    onClose: () => void;
  }

  let { x, y, view, labels, onClose }: Props = $props();

  const isVisible = (column: TrackColumnId) => view.columns.includes(column);
</script>

<BaseContextMenu {x} {y} {onClose}>
  <div class="menu-scroll">
    <div class="menu-header">{m.trackList.columns}</div>
    {#each TRACK_COLUMN_IDS as column (column)}
      <button
        class="menu-item"
        role="menuitemcheckbox"
        aria-checked={isVisible(column)}
        onclick={() => (view.columns = toggleColumn(view.columns, column))}
        disabled={isVisible(column) && view.columns.length === 1}
      >
        <span class="check" aria-hidden="true">{isVisible(column) ? '✓' : ''}</span>
        <span>{labels[column]}</span>
      </button>
    {/each}

    <div class="menu-divider"></div>

    <button
      class="menu-item"
      role="menuitem"
      onclick={() => view.resetSort()}
      disabled={view.isDefaultSort}
    >
      <span class="check" aria-hidden="true"></span>
      <span>{m.trackList.resetSort}</span>
    </button>
    <button
      class="menu-item"
      role="menuitem"
      onclick={() => view.reset()}
      disabled={view.isDefault}
    >
      <span class="check" aria-hidden="true"></span>
      <span>{m.trackList.resetColumns}</span>
    </button>
  </div>
</BaseContextMenu>

<style>
  @reference "../../../app.css";

  /* 列が多いため、画面に収まらない場合はメニューの中でスクロールする */
  .menu-scroll {
    max-height: calc(100vh - 2rem);
    overflow-y: auto;
  }

  .menu-header {
    @apply py-1.5 px-4 text-xs font-semibold text-text-muted uppercase;
  }

  .menu-item {
    @apply flex items-center gap-2 w-full py-1.5 px-4 bg-transparent border-none text-text-secondary text-sm text-left cursor-pointer transition-colors duration-150;
  }

  .menu-item:hover:not(:disabled) {
    @apply bg-surface-active;
  }

  .menu-item:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .check {
    @apply w-4 shrink-0 text-primary;
  }

  .menu-divider {
    @apply h-px bg-border my-1.5;
  }
</style>
