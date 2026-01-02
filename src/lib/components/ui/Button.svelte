<script lang="ts">
  interface Props {
    variant?: 'primary' | 'secondary' | 'accent' | 'ghost' | 'link';
    size?: 'xs' | 'sm' | 'md' | 'lg';
    disabled?: boolean;
    loading?: boolean;
    onclick?: () => void;
    type?: 'button' | 'submit' | 'reset';
    class?: string;
    children?: any;
  }

  let {
    variant = 'primary',
    size = 'md',
    disabled = false,
    loading = false,
    onclick,
    type = 'button',
    class: className = '',
    children
  }: Props = $props();

  const variantClasses: Record<string, string> = {
    primary: 'btn-primary',
    secondary: 'btn-secondary',
    accent: 'btn-accent',
    ghost: 'btn-ghost',
    link: 'btn-link'
  };

  const sizeClasses: Record<string, string> = {
    xs: 'btn-xs',
    sm: 'btn-sm',
    md: 'btn-md',
    lg: 'btn-lg'
  };

  const classes = $derived(`btn ${variantClasses[variant]} ${sizeClasses[size]} ${className}`);
</script>

<button {type} class={classes} {disabled} class:loading {onclick}>
  {@render children?.()}
</button>
