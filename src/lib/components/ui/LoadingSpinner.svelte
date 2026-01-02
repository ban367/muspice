<script lang="ts">
  interface Props {
    size?: 'xs' | 'sm' | 'md' | 'lg';
    class?: string;
  }

  let { size = 'md', class: className = '' }: Props = $props();

  const sizeClasses = {
    xs: 'loading-xs',
    sm: 'loading-sm',
    md: 'loading-md',
    lg: 'loading-lg'
  };

  const classes = $derived(`loading loading-spinner ${sizeClasses[size]} ${className}`);
</script>

<span class={classes}></span>
