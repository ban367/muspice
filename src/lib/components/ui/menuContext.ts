import { getContext, setContext } from 'svelte';

/** `BaseContextMenu`が、中の項目（`MenuItem`など）へ渡すもの */
export interface MenuContext {
  /** メニューを開く前にフォーカスがあった要素へ、フォーカスを戻す */
  restoreFocus: () => void;
}

const MENU_CONTEXT = Symbol('menu');

export function setMenuContext(context: MenuContext): void {
  setContext(MENU_CONTEXT, context);
}

export function getMenuContext(): MenuContext | undefined {
  return getContext<MenuContext | undefined>(MENU_CONTEXT);
}
