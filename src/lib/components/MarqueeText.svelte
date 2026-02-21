<script lang="ts">
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
  let overflowAmount = $state(0);

  // テキストがコンテナをはみ出しているかチェック
  function checkOverflow() {
    if (containerRef && textRef) {
      const textWidth = textRef.scrollWidth;
      const containerWidth = containerRef.clientWidth;
      isOverflowing = textWidth > containerWidth;
      overflowAmount = textWidth - containerWidth;
    }
  }

  // コンテナサイズの変更を監視
  $effect(() => {
    if (!containerRef) return;

    checkOverflow();

    const resizeObserver = new ResizeObserver(() => {
      checkOverflow();
    });

    resizeObserver.observe(containerRef);

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
  style={isOverflowing ? `--overflow-amount: -${overflowAmount}px` : ''}
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

  /* 自身へのホバーでアニメーション */
  .marquee-container:hover .marquee-text.overflowing {
    animation: marquee-scroll 4s ease-in-out infinite;
  }

  /* 親カード(.grid-card)へのホバーでもアニメーション */
  :global(.grid-card:hover) .marquee-text.overflowing,
  :global(.track-card:hover) .marquee-text.overflowing,
  :global(.list-row:hover) .marquee-text.overflowing,
  :global(.artist-card:hover) .marquee-text.overflowing,
  :global(.queue-track:hover) .marquee-text.overflowing,
  :global(.nav-item-base:hover) .marquee-text.overflowing {
    animation: marquee-scroll 4s ease-in-out infinite;
  }

  @keyframes marquee-scroll {
    0%,
    5% {
      transform: translateX(0);
    }
    35%,
    65% {
      transform: translateX(var(--overflow-amount, 0));
    }
    95%,
    100% {
      transform: translateX(0);
    }
  }
</style>
