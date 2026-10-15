import { describe, expect, it } from 'vitest';
import {
  isGlobalShortcutBlocked,
  type ShortcutKeyEvent,
  type ShortcutTarget
} from './globalShortcuts.js';

describe('isGlobalShortcutBlocked', () => {
  /** モーダルが開いているかを返す、文書の代わり */
  function fakeDocument(isModalOpen = false) {
    return {
      querySelector: (selectors: string) =>
        isModalOpen && selectors === 'dialog[open]' ? {} : null
    };
  }

  type FakeElement = { tag: string; attributes?: Record<string, string>; editable?: boolean };

  /** 要素を、外側（先頭）から内側（末尾。フォーカスのある要素）の順に並べて作る */
  function focusOn(elements: FakeElement[], { isModalOpen = false } = {}): ShortcutTarget {
    const ownerDocument = fakeDocument(isModalOpen);
    let parent: ShortcutTarget | null = null;
    for (const { tag, attributes = {}, editable = false } of elements) {
      parent = {
        tagName: tag.toUpperCase(),
        isContentEditable: editable,
        parentElement: parent,
        ownerDocument,
        getAttribute: (name) => attributes[name] ?? null
      };
    }
    if (!parent) throw new Error('要素がありません');
    return parent;
  }

  function key(init: Partial<ShortcutKeyEvent> = {}): ShortcutKeyEvent {
    return { defaultPrevented: false, ctrlKey: false, metaKey: false, ...init };
  }

  const plain = key();
  const withMeta = key({ metaKey: true });
  const withCtrl = key({ ctrlKey: true });

  it('本文・一覧にフォーカスがある時は、どのショートカットも行う', () => {
    const body = focusOn([{ tag: 'body' }]);
    const trackList = focusOn([
      { tag: 'body' },
      { tag: 'div', attributes: { role: 'listbox', tabindex: '0' } }
    ]);
    for (const target of [body, trackList, null]) {
      expect(isGlobalShortcutBlocked(plain, target)).toBe(false);
      expect(isGlobalShortcutBlocked(withMeta, target)).toBe(false);
    }
  });

  it('ほかの処理が処理したキーは行わない', () => {
    const body = focusOn([{ tag: 'body' }]);
    expect(isGlobalShortcutBlocked(key({ defaultPrevented: true }), body)).toBe(true);
    expect(isGlobalShortcutBlocked(key({ defaultPrevented: true, metaKey: true }), body)).toBe(
      true
    );
  });

  it('入力の部品と編集できる要素では、Cmd/Ctrl付きのキーも行わない', () => {
    for (const tag of ['input', 'textarea', 'select']) {
      const field = focusOn([{ tag: 'body' }, { tag }]);
      expect(isGlobalShortcutBlocked(plain, field)).toBe(true);
      expect(isGlobalShortcutBlocked(withMeta, field)).toBe(true);
    }
    const editable = focusOn([{ tag: 'body' }, { tag: 'div', editable: true }]);
    expect(isGlobalShortcutBlocked(plain, editable)).toBe(true);
    expect(isGlobalShortcutBlocked(withCtrl, editable)).toBe(true);
  });

  it('モーダルが開いている間は、中の要素・本文のどちらにフォーカスがあっても行わない', () => {
    const inDialog = focusOn(
      [{ tag: 'body' }, { tag: 'dialog', attributes: { open: '' } }, { tag: 'p' }],
      { isModalOpen: true }
    );
    const body = focusOn([{ tag: 'body' }], { isModalOpen: true });
    for (const target of [inDialog, body]) {
      expect(isGlobalShortcutBlocked(plain, target)).toBe(true);
      expect(isGlobalShortcutBlocked(withMeta, target)).toBe(true);
    }
  });

  it('ボタンでは、Cmd/Ctrlなしのキーをボタンに譲り、Cmd/Ctrl付きのキーは行う', () => {
    const button = focusOn([{ tag: 'body' }, { tag: 'div' }, { tag: 'button' }]);
    expect(isGlobalShortcutBlocked(plain, button)).toBe(true);
    expect(isGlobalShortcutBlocked(withMeta, button)).toBe(false);
    expect(isGlobalShortcutBlocked(withCtrl, button)).toBe(false);

    const tab = focusOn([{ tag: 'body' }, { tag: 'button', attributes: { role: 'tab' } }]);
    expect(isGlobalShortcutBlocked(plain, tab)).toBe(true);
  });

  it('一覧の項目をボタンで作ったもの（role="option"）は、一覧と同じに扱う', () => {
    const albumItem = focusOn([
      { tag: 'body' },
      { tag: 'div', attributes: { role: 'listbox' } },
      { tag: 'button', attributes: { role: 'option' } }
    ]);
    expect(isGlobalShortcutBlocked(plain, albumItem)).toBe(false);
    expect(isGlobalShortcutBlocked(withMeta, albumItem)).toBe(false);
  });

  it('メニューの中では、Cmd/Ctrlなしのキーを行わない', () => {
    const menu = focusOn([{ tag: 'body' }, { tag: 'div', attributes: { role: 'menu' } }]);
    const menuItem = focusOn([
      { tag: 'body' },
      { tag: 'div', attributes: { role: 'menu' } },
      { tag: 'div', attributes: { role: 'menuitemcheckbox' } }
    ]);
    for (const target of [menu, menuItem]) {
      expect(isGlobalShortcutBlocked(plain, target)).toBe(true);
      expect(isGlobalShortcutBlocked(withMeta, target)).toBe(false);
    }
  });

  it('ボタンから開くパネル（role="dialog"）の中では、Cmd/Ctrlなしのキーを行わない', () => {
    const panel = focusOn([
      { tag: 'body' },
      { tag: 'div', attributes: { role: 'dialog' } },
      { tag: 'div' },
      { tag: 'span', attributes: { tabindex: '0' } }
    ]);
    expect(isGlobalShortcutBlocked(plain, panel)).toBe(true);
    expect(isGlobalShortcutBlocked(withMeta, panel)).toBe(false);
  });

  it('スライダー（role="slider"）では、どのショートカットも行う', () => {
    const slider = focusOn([{ tag: 'body' }, { tag: 'div', attributes: { role: 'slider' } }]);
    expect(isGlobalShortcutBlocked(plain, slider)).toBe(false);
    expect(isGlobalShortcutBlocked(withMeta, slider)).toBe(false);
  });
});
