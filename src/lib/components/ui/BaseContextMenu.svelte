<!--
  @component BaseContextMenu
  カーソルの位置に開くメニューの共通基盤コンポーネント。
  Bits UIのメニューを、開く側が渡した位置（x, y）に開く。画面の端での位置の調整、外側の操作と
  Escキーで閉じること、矢印キーでの移動、先頭の文字での選択、閉じた後のフォーカスの復帰は、
  Bits UIに任せる。中には`MenuItem`・`MenuCheckboxItem`・`MenuRadioGroup`・`MenuSub`などを置く。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ContextMenu } from 'bits-ui';
  import { setMenuContext } from './menuContext.js';

  // Props
  interface Props {
    x: number;
    y: number;
    onClose: () => void;
    children: Snippet;
  }

  let { x, y, onClose, children }: Props = $props();

  // 開く位置（大きさのない点）。右クリックする範囲を`ContextMenu.Trigger`で包まず、開く側が
  // 座標を渡す（仮想スクロールの行ごとにTriggerを置かずに済み、開く側が対象を決められる）
  const anchor = $derived({
    getBoundingClientRect: () => DOMRect.fromRect({ x, y, width: 0, height: 0 })
  });

  // 開く前にフォーカスがあった要素（曲の一覧など）。メニューが開くと、フォーカスはメニューへ移る
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  // 項目を選んだ時に、項目の処理より先にフォーカスを戻す。項目がダイアログを開く場合、
  // `<dialog>`は開いた時のフォーカスを覚えて閉じた後に戻すため、先に戻しておかないと、
  // ダイアログを閉じた後のフォーカスが（消えたメニューの代わりに）bodyへ行ってしまう
  setMenuContext({
    restoreFocus: () => opener?.focus({ preventScroll: true })
  });

  function handleOpenChange(open: boolean) {
    if (!open) onClose();
  }
</script>

<!-- 開いたまま別の場所を右クリックした時は、作り直して新しい位置に開く（位置だけを変えても動かない） -->
{#key `${x},${y}`}
  <ContextMenu.Root open onOpenChange={handleOpenChange}>
    <ContextMenu.Portal>
      <ContextMenu.Content customAnchor={anchor} collisionPadding={10}>
        {#snippet child({ props, wrapperProps })}
          <div {...wrapperProps}>
            <div {...props} class="menu-panel">
              {@render children()}
            </div>
          </div>
        {/snippet}
      </ContextMenu.Content>
    </ContextMenu.Portal>
  </ContextMenu.Root>
{/key}
