<!--
  @component TextPromptDialog
  `promptText()`で要求されたテキスト入力ダイアログを表示する。レイアウトに1つだけ置く。
-->
<script lang="ts">
  import Modal from './Modal.svelte';
  import { textPrompt } from '#lib/utils/dialog.svelte.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  const id = $props.id();
  const request = $derived(textPrompt.request);

  let value = $state('');
  let error = $state<string | null>(null);

  // 新しい要求ごとに入力欄を初期化する
  $effect.pre(() => {
    if (request) {
      value = request.defaultValue ?? '';
      error = null;
    }
  });

  function cancel() {
    request?.resolve(null);
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!request) return;

    const trimmed = value.trim();
    const message = trimmed ? (request.validate?.(trimmed) ?? null) : m.validation.inputRequired;
    if (message) {
      error = message;
      return;
    }
    request.resolve(trimmed);
  }
</script>

<Modal open={request !== null} onClose={cancel} title={request?.title} class="max-w-md">
  <form id="{id}-form" onsubmit={submit} novalidate>
    <label for="{id}-input" class="form-label">{request?.label}</label>
    <input
      id="{id}-input"
      type="text"
      class="form-input"
      bind:value
      oninput={() => (error = null)}
      aria-invalid={error !== null}
      aria-describedby={error ? `${id}-error` : undefined}
      data-autofocus
    />
    {#if error}
      <p id="{id}-error" class="message-error mt-3 mb-0" role="alert">{error}</p>
    {/if}
  </form>

  {#snippet footer()}
    <button type="button" class="btn-secondary" onclick={cancel}>{m.common.cancel}</button>
    <button type="submit" form="{id}-form" class="btn-primary">
      {request?.confirmLabel ?? 'OK'}
    </button>
  {/snippet}
</Modal>
