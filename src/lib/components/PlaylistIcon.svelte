<!--
  @component PlaylistIcon
  プレイリストのアイコン。自動プレイリスト（条件で曲を集めるプレイリスト）は、別の形で出す。
-->
<script lang="ts">
  interface Props {
    /** 自動プレイリストか */
    smart?: boolean;
    /** 大きさ・色のクラス */
    class?: string;
  }

  let { smart = false, class: className = 'w-5 h-5' }: Props = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  class={className}
  fill="none"
  viewBox="0 0 24 24"
  stroke="currentColor"
  aria-hidden="true"
>
  <path
    stroke-linecap="round"
    stroke-linejoin="round"
    stroke-width="2"
    d={smart ? 'M13 10V3L4 14h7v7l9-11h-7z' : 'M4 6h16M4 10h16M4 14h16M4 18h16'}
  />
</svg>
