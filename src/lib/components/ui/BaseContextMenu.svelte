<!--
  @component BaseContextMenu
  コンテキストメニューの共通基盤コンポーネント。
  位置調整（画面外はみ出し防止）、クリック外側閉じ、Escキー閉じを提供する。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  // Props
  interface Props {
    x: number;
    y: number;
    onClose: () => void;
    children: Snippet;
  }

  let { x, y, onClose, children }: Props = $props();

  let menuElement: HTMLDivElement | null = null;

  // メニュー位置の調整
  let adjustedX = $state(0);
  let adjustedY = $state(0);

  // propsからの初期位置を設定
  $effect(() => {
    adjustedX = x;
    adjustedY = y;
  });

  // メニュー位置調整 + イベントリスナー登録
  $effect(() => {
    if (!menuElement) return;

    // メニューが画面外に出ないように位置を調整
    const rect = menuElement.getBoundingClientRect();
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    if (x + rect.width > viewportWidth) {
      adjustedX = viewportWidth - rect.width - 10;
    }
    if (y + rect.height > viewportHeight) {
      adjustedY = viewportHeight - rect.height - 10;
    }

    // クリックイベントでメニューを閉じる
    const handleClick = (e: MouseEvent) => {
      if (menuElement && !menuElement.contains(e.target as Node)) {
        onClose();
      }
    };

    // Escキーでメニューを閉じる
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('click', handleClick);
    document.addEventListener('keydown', handleKeydown);

    return () => {
      document.removeEventListener('click', handleClick);
      document.removeEventListener('keydown', handleKeydown);
    };
  });
</script>

<div
  class="context-menu"
  bind:this={menuElement}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  role="menu"
>
  {@render children()}
</div>

<style>
  @reference "../../../app.css";
  .context-menu {
    @apply fixed z-[10000] min-w-[200px] bg-base-300 border border-border rounded-lg py-2;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
    animation: fadeIn 0.1s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
