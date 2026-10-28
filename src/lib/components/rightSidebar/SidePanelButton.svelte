<!--
  @component SidePanelButton
  右サイドバーのパネルの見出しに並べる、アイコンのボタン。説明（`label`）は、ツールチップと読み上げに使う。
  アイコンのsvgは`children`で渡す（大きさは、このボタンが決める）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** ボタンの説明（ツールチップと読み上げ） */
    label: string;
    onclick: () => void;
    disabled?: boolean;
    children: Snippet;
  }

  let { label, onclick, disabled = false, children }: Props = $props();
</script>

<button type="button" class="panel-button" {onclick} {disabled} title={label} aria-label={label}>
  {@render children()}
</button>

<style>
  @reference "../../../app.css";

  .panel-button {
    @apply w-7 h-7 flex items-center justify-center p-0 border-none rounded-md bg-transparent
           text-text-secondary cursor-pointer transition-colors duration-150;
  }

  .panel-button:hover:not(:disabled) {
    @apply bg-surface-active text-text-primary;
  }

  .panel-button:disabled {
    @apply opacity-30 cursor-not-allowed;
  }

  .panel-button :global(svg) {
    @apply w-4 h-4;
  }
</style>
