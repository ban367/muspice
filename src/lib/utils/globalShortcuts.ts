/**
 * ウィンドウ全体のキー操作（プレーヤーのショートカット）を行うかどうか
 *
 * プレーヤー（`Player.svelte`）はウィンドウでキーを受け取るため、フォーカスのある要素がそのキーを
 * 自分の操作に使う場合（ボタンのSpace・メニューの頭文字など）は、ショートカットを行わずに譲る。
 */

/** フォーカスのある要素として調べる性質（`HTMLElement`が当てはまる。テストではこの形のオブジェクトを渡す） */
export interface ShortcutTarget {
  readonly tagName: string;
  readonly isContentEditable: boolean;
  readonly parentElement: ShortcutTarget | null;
  readonly ownerDocument: { querySelector(selectors: string): unknown };
  getAttribute(name: string): string | null;
}

/** ショートカットを行うかを決めるのに使う、キーイベントの性質 */
export type ShortcutKeyEvent = Pick<KeyboardEvent, 'defaultPrevented' | 'ctrlKey' | 'metaKey'>;

/** キーをすべて自分で使う入力の部品 */
const FORM_FIELD_TAGS = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

/** 中にフォーカスがある間、修飾キーなしのキーを中の部品に譲る範囲（メニュー・ボタンから開くパネル） */
const KEY_OWNING_ROLES = new Set(['menu', 'dialog', 'alertdialog']);

/**
 * ウィンドウ全体のショートカットを、このキーで行わないかどうか
 *
 * - ほかの処理（メニュー・一覧など）がすでに処理したキー（`defaultPrevented`）は行わない
 * - 入力の部品（input・textarea・select）と編集できる要素では、どのショートカットも行わない
 * - モーダル（`<dialog open>`）が開いている間は、どのショートカットも行わない（背面の画面と同じく
 *   操作できなくする。Cmd/Ctrl+Lで背面の画面を移動して、ダイアログを開いた画面を閉じないように）
 * - Cmd/Ctrlなしのキー（Space・S・R・M）は、ボタン（Spaceで押す）と、メニュー（`role="menu"`。
 *   頭文字で項目を選ぶ）・ボタンから開くパネル（`role="dialog"`）の中では行わない。
 *   一覧の項目をボタンで作ったもの（`role="option"`）は、一覧と同じに扱う（Spaceで再生・一時停止する）
 * @param event - キーイベント
 * @param target - フォーカスのある要素（キーイベントの`target`。要素でない場合はnull）
 * @returns ショートカットを行わない場合はtrue
 */
export function isGlobalShortcutBlocked(
  event: ShortcutKeyEvent,
  target: ShortcutTarget | null
): boolean {
  if (event.defaultPrevented) return true;
  if (!target) return false;

  if (FORM_FIELD_TAGS.has(target.tagName) || target.isContentEditable) return true;
  if (target.ownerDocument.querySelector('dialog[open]')) return true;

  if (event.ctrlKey || event.metaKey) return false;

  if (target.tagName === 'BUTTON' && target.getAttribute('role') !== 'option') return true;
  for (let element: ShortcutTarget | null = target; element; element = element.parentElement) {
    if (KEY_OWNING_ROLES.has(element.getAttribute('role') ?? '')) return true;
  }
  return false;
}
