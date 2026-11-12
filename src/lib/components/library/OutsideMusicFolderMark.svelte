<!--
  @component OutsideMusicFolderMark
  音楽フォルダ（アプリの領域）の外にある曲の印（ADR-084）。曲の一覧のタイトルの前と、カードの画像の隅に出す。
  色は、置く場所の文字の色に合わせる（`currentColor`）。
-->
<script lang="ts">
  import { m } from '#lib/i18n/i18n.svelte.js';
</script>

<span
  class="outside-mark"
  title={m.trackList.outsideMusicFolder}
  aria-label={m.trackList.outsideMusicFolder}
  role="img"
>
  <!-- フォルダから外へ出る矢印 -->
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-4" />
    <path d="M14 4h6v6" />
    <path d="M20 4l-8 8" />
  </svg>
</span>

<style>
  @reference "../../../app.css";

  .outside-mark {
    @apply inline-flex shrink-0 items-center justify-center;
  }

  .outside-mark svg {
    @apply w-3.5 h-3.5;
  }
</style>
