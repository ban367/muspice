<!--
  @component SidePanelCheckbox
  右サイドバーのまとまりに置く、名前の付いたチェックボックス（リプレイゲインのクリップ防止など）。
-->
<script lang="ts">
  interface Props {
    label: string;
    checked: boolean;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  }

  let { label, checked, disabled = false, onchange }: Props = $props();
</script>

<label class="checkbox-field" class:disabled>
  <input
    type="checkbox"
    class="checkbox"
    {checked}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
  <span class="checkbox-label">{label}</span>
</label>

<style>
  @reference "../../../app.css";

  .checkbox-field {
    @apply flex items-center gap-2 cursor-pointer;
  }

  .checkbox-field.disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .checkbox {
    @apply w-3.5 h-3.5 m-0 shrink-0 accent-primary cursor-pointer;
  }

  .checkbox:disabled {
    @apply cursor-not-allowed;
  }

  .checkbox-label {
    @apply min-w-0 text-xs text-text-secondary;
  }
</style>
