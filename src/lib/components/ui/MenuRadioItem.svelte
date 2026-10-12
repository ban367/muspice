<!--
  @component MenuRadioItem
  `MenuRadioGroup`の中の、選べる項目。選ぶとメニューを閉じる。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';

  interface Props {
    value: string;
    disabled?: boolean;
    closeOnSelect?: boolean;
    children: Snippet;
  }

  let { value, disabled = false, closeOnSelect = true, children }: Props = $props();
</script>

<ContextMenu.RadioItem {value} {disabled} {closeOnSelect}>
  {#snippet child({ props, checked })}
    <div {...props} class="menu-item">
      <span class="menu-leading" aria-hidden="true">{checked ? '✓' : ''}</span>
      <span class="menu-label">{@render children()}</span>
    </div>
  {/snippet}
</ContextMenu.RadioItem>
