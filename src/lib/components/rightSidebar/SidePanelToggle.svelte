<!--
  @component SidePanelToggle
  右サイドバーのまとまり（`SidePanelSection`）の見出しの右端に置く、オン・オフのスイッチ（リプレイゲイン・イコライザ・クロスフェードなど）。
  名前は付けず、何を切り替えるかは`label`（読み上げ）と`title`（ツールチップ）で伝える。
  名前の付いたスイッチ（パネルの見出しに置くもの）は`SidePanelSwitch`。
-->
<script lang="ts">
  interface Props {
    checked: boolean;
    /** 切り替えるものの名前（読み上げ） */
    label: string;
    /** ツールチップ（「イコライザをオフ」など、押した時に起きること） */
    title: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  }

  let { checked, label, title, disabled = false, onchange }: Props = $props();
</script>

<button
  type="button"
  class="toggle"
  class:active={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {title}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="toggle-track" aria-hidden="true">
    <span class="toggle-thumb"></span>
  </span>
</button>

<style>
  @reference "../../../app.css";

  .toggle {
    @apply p-0 bg-transparent border-none cursor-pointer;
  }

  .toggle:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  /* オフの間は、テーマの文字の色のつまみを、少し濃い面に置く（白のつまみは、ライトテーマで見えない） */
  .toggle-track {
    @apply relative block w-10 h-5 rounded-full bg-surface-strong transition-colors duration-200;
  }

  .toggle.active .toggle-track {
    @apply bg-secondary;
  }

  .toggle-thumb {
    @apply absolute left-0.5 top-0.5 w-4 h-4 rounded-full bg-text-primary transition-transform duration-200;
  }

  /* オンの間は、色の付いた面の上の白（どちらのテーマでも同じ） */
  .toggle.active .toggle-thumb {
    @apply translate-x-5 bg-white;
  }
</style>
