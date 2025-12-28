<script lang="ts">
  interface Props {
    open?: boolean;
    title?: string;
    onclose?: () => void;
    children?: any;
  }

  let { open = $bindable(false), title, onclose, children }: Props = $props();

  function handleClose() {
    open = false;
    onclose?.();
  }
</script>

{#if open}
  <div class="modal modal-open">
    <div class="modal-box">
      {#if title}
        <h3 class="text-lg font-bold">{title}</h3>
      {/if}
      <div class="py-4">
        {@render children?.()}
      </div>
      <div class="modal-action">
        <button class="btn" onclick={handleClose}>閉じる</button>
      </div>
    </div>
    <button class="modal-backdrop" onclick={handleClose} aria-label="モーダルを閉じる"></button>
  </div>
{/if}
