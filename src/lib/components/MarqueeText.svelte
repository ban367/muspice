<script lang="ts">
  import { onMount } from 'svelte';

  // Props
  interface Props {
    text: string;
    class?: string;
  }

  let { text, class: className = '' }: Props = $props();

  // 要素参照
  let containerRef = $state<HTMLElement | null>(null);
  let textRef = $state<HTMLElement | null>(null);

  // オーバーフロー状態
  let isOverflowing = $state(false);

  // テキストがコンテナをはみ出しているかチェック
  function checkOverflow() {
    if (containerRef && textRef) {
      isOverflowing = textRef.scrollWidth > containerRef.clientWidth;
    }
  }

  // マウントとテキスト変更時にオーバーフローをチェック
  onMount(() => {
    checkOverflow();

    // ResizeObserverでコンテナサイズの変更を監視
    const resizeObserver = new ResizeObserver(() => {
      checkOverflow();
    });

    if (containerRef) {
      resizeObserver.observe(containerRef);
    }

    return () => {
      resizeObserver.disconnect();
    };
  });

  // テキストが変更されたらオーバーフローを再チェック
  $effect(() => {
    text;
    // 次のフレームでチェック（DOMが更新された後）
    requestAnimationFrame(checkOverflow);
  });
</script>

<div
  bind:this={containerRef}
  class="marquee-container {className}"
  title={isOverflowing ? text : undefined}
>
  <span bind:this={textRef} class="marquee-text" class:overflowing={isOverflowing}>
    {text}
  </span>
</div>

<style>
  @reference "../../app.css";

  .marquee-container {
    @apply overflow-hidden whitespace-nowrap;
    position: relative;
  }

  .marquee-text {
    @apply inline-block whitespace-nowrap;
    will-change: transform;
  }

  .marquee-container:hover .marquee-text.overflowing {
    animation: marquee-scroll var(--marquee-duration, 5s) linear infinite;
    animation-delay: 0.3s;
  }

  @keyframes marquee-scroll {
    0%,
    10% {
      transform: translateX(0);
    }
    45%,
    55% {
      transform: translateX(calc(-100% + var(--container-width, 100%)));
    }
    90%,
    100% {
      transform: translateX(0);
    }
  }
</style>
