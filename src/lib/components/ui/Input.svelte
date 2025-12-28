<script lang="ts">
  interface Props {
    type?: 'text' | 'email' | 'password' | 'number' | 'search';
    placeholder?: string;
    value?: string;
    disabled?: boolean;
    required?: boolean;
    class?: string;
    oninput?: (e: Event) => void;
    onchange?: (e: Event) => void;
  }

  let {
    type = 'text',
    placeholder = '',
    value = $bindable(''),
    disabled = false,
    required = false,
    class: className = '',
    oninput,
    onchange
  }: Props = $props();

  const classes = `input input-bordered w-full ${className}`;
</script>

<input {type} {placeholder} bind:value {disabled} {required} class={classes} {oninput} {onchange} />
