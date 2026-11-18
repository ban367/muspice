<!--
  @component SidePanelOptions
  右サイドバーのまとまりに置く、いくつかの中から1つを選ぶボタンの並び（リプレイゲインの単位など）。
  選んでいるボタンは`aria-pressed`で伝える（再生の補助のメニューの選択肢と同じ形）。
-->
<script lang="ts" generics="T extends string">
  interface Props {
    /** 選択肢のまとまりの名前（読み上げ） */
    label: string;
    options: readonly { value: T; label: string }[];
    value: T;
    disabled?: boolean;
    onchange: (value: T) => void;
  }

  let { label, options, value, disabled = false, onchange }: Props = $props();
</script>

<div class="options" role="group" aria-label={label} style:--columns={options.length}>
  {#each options as option (option.value)}
    <button
      type="button"
      class="option"
      class:selected={option.value === value}
      aria-pressed={option.value === value}
      {disabled}
      onclick={() => {
        if (option.value !== value) onchange(option.value);
      }}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  @reference "../../../app.css";

  .options {
    @apply grid gap-1;
    grid-template-columns: repeat(var(--columns), minmax(0, 1fr));
  }

  .option {
    @apply h-7 px-1 bg-surface border border-border rounded-md text-xs text-text-secondary truncate
           cursor-pointer transition-colors duration-150;
  }

  .option:hover:not(:disabled) {
    @apply text-text-primary border-border-light;
  }

  .option.selected {
    @apply border-primary text-primary;
  }

  .option:disabled {
    @apply opacity-50 cursor-not-allowed;
  }
</style>
