<script lang="ts">
  interface Props {
    src: string | null;
    alt: string;
    rounded?: 'none' | 'sm' | 'md' | 'lg' | 'full';
    placeholderType?: 'disc' | 'person' | 'music';
  }

  let { src, alt, rounded = 'md', placeholderType = 'disc' }: Props = $props();

  // ラウンド値をTailwindクラスに変換
  const roundedClass = $derived(
    {
      none: '',
      sm: 'rounded-sm',
      md: 'rounded-md',
      lg: 'rounded-lg',
      full: 'rounded-full'
    }[rounded]
  );
</script>

<div class="album-art-container {roundedClass}">
  {#if src}
    <img {src} {alt} class="album-art-image {roundedClass}" loading="lazy" />
  {:else}
    <div class="album-art-placeholder {roundedClass}">
      {#if placeholderType === 'disc'}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <circle cx="12" cy="12" r="10" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      {:else if placeholderType === 'person'}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
      {:else}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path d="M9 18V5l12-2v13" />
          <circle cx="6" cy="18" r="3" />
          <circle cx="18" cy="16" r="3" />
        </svg>
      {/if}
    </div>
  {/if}
</div>

<style>
  @reference "../../app.css";

  .album-art-container {
    @apply w-full h-full relative overflow-hidden;
  }

  .album-art-image {
    @apply w-full h-full object-contain bg-base-300;
  }

  .album-art-placeholder {
    @apply w-full h-full flex items-center justify-center;
    background: linear-gradient(135deg, var(--color-base-400), var(--color-base-200));
  }

  .album-art-placeholder svg {
    @apply w-2/5 h-2/5 text-text-dimmed;
  }
</style>
