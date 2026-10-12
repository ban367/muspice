<!--
  @component MenuRadioGroup
  メニューの、1つだけを選ぶ項目のまとまり。中に`MenuRadioItem`を置く。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';

  interface Props {
    value: string;
    onValueChange: (value: string) => void;
    class?: string;
    children: Snippet;
  }

  let { value, onValueChange, class: className, children }: Props = $props();
</script>

<ContextMenu.RadioGroup {value} {onValueChange} class={className}>
  {@render children()}
</ContextMenu.RadioGroup>
