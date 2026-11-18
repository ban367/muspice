<!--
  @component SidePanelSwitch
  右サイドバーのパネルの見出しに置く、名前の付いたオン・オフのスイッチ（選択中の曲のパネルの「選択で切り替え」）。
  説明（`hint`）は、ツールチップに出す。
-->
<script lang="ts">
  interface Props {
    /** スイッチの名前（スイッチの左に出す） */
    label: string;
    /** 何が切り替わるかの説明（ツールチップ） */
    hint: string;
    checked: boolean;
  }

  let { label, hint, checked = $bindable() }: Props = $props();
</script>

<button
  type="button"
  class="panel-switch"
  class:active={checked}
  role="switch"
  aria-checked={checked}
  title={hint}
  onclick={() => (checked = !checked)}
>
  <span class="switch-label">{label}</span>
  <span class="switch-track" aria-hidden="true">
    <span class="switch-thumb"></span>
  </span>
</button>

<style>
  @reference "../../../app.css";

  .panel-switch {
    @apply flex items-center gap-1.5 h-7 px-1.5 border-none rounded-md bg-transparent cursor-pointer
           text-text-secondary transition-colors duration-150;
  }

  .panel-switch:hover {
    @apply bg-surface-active text-text-primary;
  }

  .switch-label {
    @apply text-xs whitespace-nowrap;
  }

  /* オフの間は、テーマの文字の色のつまみを、少し濃い面に置く（イコライザのスイッチと同じ色） */
  .switch-track {
    @apply relative block w-7 h-4 shrink-0 rounded-full bg-surface-strong transition-colors duration-200;
  }

  .panel-switch.active .switch-track {
    @apply bg-secondary;
  }

  .switch-thumb {
    @apply absolute left-0.5 top-0.5 w-3 h-3 rounded-full bg-text-primary transition-transform duration-200;
  }

  /* オンの間は、色の付いた面の上の白（どちらのテーマでも同じ） */
  .panel-switch.active .switch-thumb {
    @apply translate-x-3 bg-white;
  }
</style>
