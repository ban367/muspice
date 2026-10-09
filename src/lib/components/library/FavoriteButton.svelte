<!--
  @component FavoriteButton
  お気に入りのハート。押すと、その曲をお気に入りにする・お気に入りから外す。
  お気に入りの曲は塗ったハート、そうでない曲は薄い枠のハートで示す。
-->
<script lang="ts">
  import { useSetFavoriteMutation } from '#lib/queries/tracks.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    trackId: string;
    isFavorite: boolean;
    /** 大きさ（一覧の行は`small`、プレーヤーは`medium`） */
    size?: 'small' | 'medium';
  }

  let { trackId, isFavorite, size = 'small' }: Props = $props();

  const setFavoriteMutation = useSetFavoriteMutation();
  const label = $derived(isFavorite ? m.common.removeFromFavorites : m.common.addToFavorites);

  function toggle(event: MouseEvent) {
    // 行のクリック（選択）として扱わせない
    event.stopPropagation();
    setFavoriteMutation.mutate({ trackIds: [trackId], favorite: !isFavorite });
  }
</script>

<!-- 続けて押しても、行のダブルクリック（再生）として扱わせない -->
<button
  type="button"
  class="favorite-button {size}"
  class:active={isFavorite}
  onclick={toggle}
  ondblclick={(event) => event.stopPropagation()}
  title={label}
  aria-label={label}
  aria-pressed={isFavorite}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill={isFavorite ? 'currentColor' : 'none'}
    stroke="currentColor"
    stroke-width="2"
    aria-hidden="true"
  >
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
    />
  </svg>
</button>

<style>
  @reference "../../../app.css";

  .favorite-button {
    @apply inline-flex items-center justify-center shrink-0 bg-transparent border-none p-0 text-base-400 cursor-pointer transition-all;
  }

  .favorite-button:hover {
    @apply scale-125 text-text-secondary;
  }

  .favorite-button.active,
  .favorite-button.active:hover {
    @apply text-error;
  }

  .favorite-button.small svg {
    @apply w-4 h-4;
  }

  .favorite-button.medium svg {
    @apply w-5 h-5;
  }
</style>
