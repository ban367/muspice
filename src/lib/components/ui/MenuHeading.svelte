<!--
  @component MenuHeading
  メニューの見出し（選べない行）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="menu-heading">{@render children()}</div>
