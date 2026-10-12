<!--
  @component MenuItem
  メニューの項目（`BaseContextMenu`・`MenuSub`の中で使う）。
  選ぶと`onSelect`を呼び、メニューを閉じる（`closeOnSelect`がfalseなら開いたまま）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';
  import { getMenuContext } from './menuContext.js';

  interface Props {
    onSelect?: () => void;
    disabled?: boolean;
    /** 削除など、取り消せない操作の項目 */
    danger?: boolean;
    closeOnSelect?: boolean;
    /** 右端に出すショートカットキーの表記 */
    shortcut?: string;
    /** 先頭に出すアイコン（`menu-icon`のクラスを付けたsvg） */
    icon?: Snippet;
    /** アイコンがなくても、先頭を空ける（チェックの印を持つ項目と、文字の位置を揃える） */
    inset?: boolean;
    children: Snippet;
  }

  let {
    onSelect,
    disabled = false,
    danger = false,
    closeOnSelect = true,
    shortcut,
    icon,
    inset = false,
    children
  }: Props = $props();

  const menu = getMenuContext();

  function handleSelect() {
    // メニューを閉じる項目は、処理より先にフォーカスを元の要素へ戻す（`BaseContextMenu`参照）
    if (closeOnSelect) menu?.restoreFocus();
    onSelect?.();
  }
</script>

<ContextMenu.Item {disabled} {closeOnSelect} onSelect={handleSelect}>
  {#snippet child({ props })}
    <div {...props} class={['menu-item', danger && 'menu-item-danger']}>
      {#if icon}
        {@render icon()}
      {:else if inset}
        <span class="menu-leading"></span>
      {/if}
      <span class="menu-label">{@render children()}</span>
      {#if shortcut}
        <span class="menu-shortcut">{shortcut}</span>
      {/if}
    </div>
  {/snippet}
</ContextMenu.Item>
