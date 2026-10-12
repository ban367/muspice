<!--
  @component MenuCheckboxItem
  メニューの、チェックを切り替える項目。切り替えてもメニューは閉じない
  （続けていくつかを切り替えられるようにするため。`closeOnSelect`で変えられる）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';

  interface Props {
    checked: boolean;
    onCheckedChange: (checked: boolean) => void;
    disabled?: boolean;
    closeOnSelect?: boolean;
    children: Snippet;
  }

  let {
    checked,
    onCheckedChange,
    disabled = false,
    closeOnSelect = false,
    children
  }: Props = $props();
</script>

<ContextMenu.CheckboxItem {checked} {onCheckedChange} {disabled} {closeOnSelect}>
  {#snippet child({ props })}
    <div {...props} class="menu-item">
      <span class="menu-leading" aria-hidden="true">{checked ? '✓' : ''}</span>
      <span class="menu-label">{@render children()}</span>
    </div>
  {/snippet}
</ContextMenu.CheckboxItem>
