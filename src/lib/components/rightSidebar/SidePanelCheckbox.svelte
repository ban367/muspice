<!--
  @component SidePanelCheckbox
  右サイドバーのまとまりに置く、説明の付いたチェックボックス（ギャップレス再生など）。
-->
<script lang="ts">
  interface Props {
    label: string;
    checked: boolean;
    /** 説明（名前の下に出す） */
    hint?: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  }

  let { label, checked, hint, disabled = false, onchange }: Props = $props();

  const hintId = $props.id();
</script>

<label class="checkbox-field" class:disabled>
  <input
    type="checkbox"
    class="checkbox"
    {checked}
    {disabled}
    aria-describedby={hint ? hintId : undefined}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
  <span class="checkbox-text">
    <span class="checkbox-label">{label}</span>
    {#if hint}
      <span class="checkbox-hint" id={hintId}>{hint}</span>
    {/if}
  </span>
</label>

<style>
  @reference "../../../app.css";

  .checkbox-field {
    @apply flex items-start gap-2 cursor-pointer;
  }

  .checkbox-field.disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .checkbox {
    @apply w-3.5 h-3.5 mt-px shrink-0 accent-primary cursor-pointer;
  }

  .checkbox:disabled {
    @apply cursor-not-allowed;
  }

  .checkbox-text {
    @apply flex flex-col gap-0.5 min-w-0;
  }

  .checkbox-label {
    @apply text-xs text-text-secondary;
  }

  .checkbox-hint {
    @apply text-[0.6875rem] leading-snug text-text-muted;
  }
</style>
