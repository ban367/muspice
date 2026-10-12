<!--
  @component TrackColumnMenu
  曲の一覧の、列を選ぶメニュー（列の見出しの右クリックで開く）。
  列の表示・非表示を切り替え、並び順・列を既定に戻す。切り替えてもメニューは閉じない
  （続けていくつかの列を切り替えられるようにするため）。
-->
<script lang="ts">
  import {
    BaseContextMenu,
    MenuCheckboxItem,
    MenuHeading,
    MenuItem,
    MenuSeparator
  } from '#lib/components/ui/index.js';
  import type { TrackListView } from '#lib/stores/trackListView.svelte.js';
  import { TRACK_COLUMN_IDS, toggleColumn, type TrackColumnId } from '#lib/utils/trackColumns.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    x: number;
    y: number;
    /** 列を持つ、画面の設定 */
    view: TrackListView;
    /** 並び順を持つ設定（列と同じ単位で覚える場合は、`view`と同じもの） */
    sortView: TrackListView;
    /** 列の表示名 */
    labels: Record<TrackColumnId, string>;
    onClose: () => void;
  }

  let { x, y, view, sortView, labels, onClose }: Props = $props();

  // 列が既定のままか（並び順を別に持つ場合、`view`の並び順は使っていない）
  const isDefaultColumns = $derived(view === sortView ? view.isDefault : view.isDefaultColumns);

  function resetAll() {
    view.reset();
    sortView.resetSort();
  }

  const isVisible = (column: TrackColumnId) => view.columns.includes(column);
</script>

<BaseContextMenu {x} {y} {onClose}>
  <MenuHeading>{m.trackList.columns}</MenuHeading>
  {#each TRACK_COLUMN_IDS as column (column)}
    <MenuCheckboxItem
      checked={isVisible(column)}
      onCheckedChange={() => (view.columns = toggleColumn(view.columns, column))}
      disabled={isVisible(column) && view.columns.length === 1}
    >
      {labels[column]}
    </MenuCheckboxItem>
  {/each}

  <MenuSeparator />

  <MenuItem
    onSelect={() => sortView.resetSort()}
    disabled={sortView.isDefaultSort}
    closeOnSelect={false}
    inset
  >
    {m.trackList.resetSort}
  </MenuItem>
  <MenuItem
    onSelect={resetAll}
    disabled={isDefaultColumns && sortView.isDefaultSort}
    closeOnSelect={false}
    inset
  >
    {m.trackList.resetColumns}
  </MenuItem>
</BaseContextMenu>
