import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listSelectionTarget,
  navigationTarget,
  openableListAction,
  treeKeyAction,
  twoColumnSideTarget
} from './listNavigation.js';

describe('navigationTarget（リスト）', () => {
  it('↑↓で前後の項目へ移る', () => {
    expect(navigationTarget('ArrowDown', 1, 5)).toBe(2);
    expect(navigationTarget('ArrowUp', 1, 5)).toBe(0);
  });

  it('端ではその場にとどまる', () => {
    expect(navigationTarget('ArrowUp', 0, 5)).toBe(0);
    expect(navigationTarget('ArrowDown', 4, 5)).toBe(4);
  });

  it('Home / End で先頭・末尾へ移る', () => {
    expect(navigationTarget('Home', 3, 5)).toBe(0);
    expect(navigationTarget('End', 1, 5)).toBe(4);
  });

  it('選択がない場合、↓は先頭、↑は末尾へ移る', () => {
    expect(navigationTarget('ArrowDown', -1, 5)).toBe(0);
    expect(navigationTarget('ArrowUp', -1, 5)).toBe(4);
  });

  it('←→・その他のキーは扱わない', () => {
    expect(navigationTarget('ArrowLeft', 1, 5)).toBeNull();
    expect(navigationTarget('ArrowRight', 1, 5)).toBeNull();
    expect(navigationTarget('Enter', 1, 5)).toBeNull();
    expect(navigationTarget('a', 1, 5)).toBeNull();
  });

  it('項目がない場合は扱わない', () => {
    expect(navigationTarget('ArrowDown', -1, 0)).toBeNull();
    expect(navigationTarget('Home', -1, 0)).toBeNull();
  });
});

describe('navigationTarget（グリッド）', () => {
  // 4列・10項目:
  //  0 1 2 3
  //  4 5 6 7
  //  8 9
  it('←→で前後、↑↓で上下の行へ移る', () => {
    expect(navigationTarget('ArrowRight', 5, 10, 4)).toBe(6);
    expect(navigationTarget('ArrowLeft', 5, 10, 4)).toBe(4);
    expect(navigationTarget('ArrowDown', 1, 10, 4)).toBe(5);
    expect(navigationTarget('ArrowUp', 5, 10, 4)).toBe(1);
  });

  it('←→は行をまたいで移り、端ではとどまる', () => {
    expect(navigationTarget('ArrowRight', 3, 10, 4)).toBe(4);
    expect(navigationTarget('ArrowLeft', 4, 10, 4)).toBe(3);
    expect(navigationTarget('ArrowLeft', 0, 10, 4)).toBe(0);
    expect(navigationTarget('ArrowRight', 9, 10, 4)).toBe(9);
  });

  it('最初の行の↑・最後の行の↓ではとどまる', () => {
    expect(navigationTarget('ArrowUp', 2, 10, 4)).toBe(2);
    expect(navigationTarget('ArrowDown', 8, 10, 4)).toBe(8);
  });

  it('下の行に項目がない列から↓を押すと、最後の項目へ移る', () => {
    expect(navigationTarget('ArrowDown', 6, 10, 4)).toBe(9);
  });

  it('1列のグリッドでも←→で前後へ移る', () => {
    expect(navigationTarget('ArrowRight', 0, 3, 1)).toBe(1);
    expect(navigationTarget('ArrowDown', 0, 3, 1)).toBe(1);
  });
});

describe('twoColumnSideTarget', () => {
  // 7項目（左の列に4つ、右の列に3つ）:
  //  0 4
  //  1 5
  //  2 6
  //  3
  it('→で右の列、←で左の列の、同じ段の項目へ移る', () => {
    expect(twoColumnSideTarget('ArrowRight', 0, 7)).toBe(4);
    expect(twoColumnSideTarget('ArrowRight', 2, 7)).toBe(6);
    expect(twoColumnSideTarget('ArrowLeft', 4, 7)).toBe(0);
    expect(twoColumnSideTarget('ArrowLeft', 6, 7)).toBe(2);
  });

  it('右の列の同じ段に項目がない時は、右の列の最後の項目へ移る', () => {
    expect(twoColumnSideTarget('ArrowRight', 3, 7)).toBe(6);
  });

  it('外側に列がない時は、その場にとどまる', () => {
    expect(twoColumnSideTarget('ArrowLeft', 1, 7)).toBe(1);
    expect(twoColumnSideTarget('ArrowRight', 5, 7)).toBe(5);
  });

  it('項目の数が偶数なら、左右に同じ数ずつ分ける', () => {
    // 0 2
    // 1 3
    expect(twoColumnSideTarget('ArrowRight', 1, 4)).toBe(3);
    expect(twoColumnSideTarget('ArrowLeft', 2, 4)).toBe(0);
  });

  it('1項目だけの時は、どちらへも動かない', () => {
    expect(twoColumnSideTarget('ArrowRight', 0, 1)).toBe(0);
    expect(twoColumnSideTarget('ArrowLeft', 0, 1)).toBe(0);
  });

  it('←→以外のキー・現在の位置が範囲外の場合は扱わない', () => {
    expect(twoColumnSideTarget('ArrowDown', 0, 7)).toBeNull();
    expect(twoColumnSideTarget('ArrowRight', -1, 7)).toBeNull();
    expect(twoColumnSideTarget('ArrowRight', 7, 7)).toBeNull();
    expect(twoColumnSideTarget('ArrowRight', 0, 0)).toBeNull();
  });
});

/** キーイベント（Node環境には`KeyboardEvent`がないため、使うプロパティだけを持つ） */
function keyEvent(key: string, init: Partial<KeyboardEvent> = {}) {
  const event = {
    key,
    target: null,
    defaultPrevented: false,
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    preventDefault() {
      event.defaultPrevented = true;
    },
    ...init
  };
  return event as unknown as KeyboardEvent;
}

describe('listSelectionTarget', () => {
  it('↑↓・Home・Endで移動先を返し、既定の動作を止める', () => {
    const down = keyEvent('ArrowDown');
    expect(listSelectionTarget(down, 1, 5)).toBe(2);
    expect(down.defaultPrevented).toBe(true);
    expect(listSelectionTarget(keyEvent('ArrowUp'), 1, 5)).toBe(0);
    expect(listSelectionTarget(keyEvent('End'), 1, 5)).toBe(4);
    expect(listSelectionTarget(keyEvent('Home'), 3, 5)).toBe(0);
  });

  it('扱わないキー・修飾キーを押している場合は何もしない', () => {
    const enter = keyEvent('Enter');
    expect(listSelectionTarget(enter, 1, 5)).toBeNull();
    expect(enter.defaultPrevented).toBe(false);

    const withShift = keyEvent('ArrowDown', { shiftKey: true });
    expect(listSelectionTarget(withShift, 1, 5)).toBeNull();
    expect(withShift.defaultPrevented).toBe(false);
    expect(listSelectionTarget(keyEvent('ArrowDown', { metaKey: true }), 1, 5)).toBeNull();
  });

  it('すでに処理されたイベント・項目がない一覧では何もしない', () => {
    expect(listSelectionTarget(keyEvent('ArrowDown', { defaultPrevented: true }), 1, 5)).toBeNull();
    expect(listSelectionTarget(keyEvent('ArrowDown'), -1, 0)).toBeNull();
  });
});

describe('openableListAction', () => {
  // Node環境には`Element`がないため、ボタンの中かどうか（`closest`）だけを持つ要素に差し替える
  class FakeElement {
    constructor(private readonly inButton: boolean) {}
    closest() {
      return this.inButton ? this : null;
    }
  }

  beforeEach(() => {
    vi.stubGlobal('Element', FakeElement);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('Enterで、現在位置の項目を開く（既定の動作を止める）', () => {
    const enter = keyEvent('Enter');
    expect(openableListAction(enter, 2, 5, 4)).toEqual({ type: 'open', index: 2 });
    expect(enter.defaultPrevented).toBe(true);
  });

  it('現在位置がない場合、Enterでは何もしない', () => {
    const enter = keyEvent('Enter');
    expect(openableListAction(enter, -1, 5, 4)).toBeNull();
    expect(enter.defaultPrevented).toBe(false);
  });

  it('項目の中のボタンでのEnterは、ボタンの操作に任せる', () => {
    const onButton = keyEvent('Enter', { target: new FakeElement(true) as unknown as Element });
    expect(openableListAction(onButton, 2, 5, 4)).toBeNull();
    expect(onButton.defaultPrevented).toBe(false);

    const onCard = keyEvent('Enter', { target: new FakeElement(false) as unknown as Element });
    expect(openableListAction(onCard, 2, 5, 4)).toEqual({ type: 'open', index: 2 });
  });

  it('矢印キー・Home・Endで、移動先へ現在位置を移す（グリッドは上下左右）', () => {
    const down = keyEvent('ArrowDown');
    expect(openableListAction(down, 1, 10, 4)).toEqual({ type: 'move', index: 5 });
    expect(down.defaultPrevented).toBe(true);
    expect(openableListAction(keyEvent('ArrowRight'), 1, 10, 4)).toEqual({
      type: 'move',
      index: 2
    });
    expect(openableListAction(keyEvent('End'), 1, 10, 4)).toEqual({ type: 'move', index: 9 });
    // リストでは←→を扱わない
    expect(openableListAction(keyEvent('ArrowRight'), 1, 10, null)).toBeNull();
    expect(openableListAction(keyEvent('ArrowDown'), 1, 10, null)).toEqual({
      type: 'move',
      index: 2
    });
  });

  it('修飾キーを押している場合・すでに処理されたイベントでは何もしない', () => {
    const withShift = keyEvent('ArrowDown', { shiftKey: true });
    expect(openableListAction(withShift, 1, 10, 4)).toBeNull();
    expect(withShift.defaultPrevented).toBe(false);
    expect(openableListAction(keyEvent('Enter', { metaKey: true }), 1, 10, 4)).toBeNull();
    expect(openableListAction(keyEvent('Enter', { defaultPrevented: true }), 1, 10, 4)).toBeNull();
  });
});

describe('treeKeyAction', () => {
  // 1990年代（開いている）
  //   1994
  //   1999
  // 1980年代（閉じている）
  // 不明（子がない）
  const rows = [
    { depth: 0, hasChildren: true, isExpanded: true },
    { depth: 1, hasChildren: false, isExpanded: false },
    { depth: 1, hasChildren: false, isExpanded: false },
    { depth: 0, hasChildren: true, isExpanded: false },
    { depth: 0, hasChildren: false, isExpanded: false }
  ];

  it('↑↓・Home・Endで、前後・先頭・末尾の行を選ぶ（既定の動作を止める）', () => {
    const down = keyEvent('ArrowDown');
    expect(treeKeyAction(down, rows, 0)).toEqual({ type: 'select', index: 1 });
    expect(down.defaultPrevented).toBe(true);
    expect(treeKeyAction(keyEvent('ArrowUp'), rows, 3)).toEqual({ type: 'select', index: 2 });
    expect(treeKeyAction(keyEvent('End'), rows, 0)).toEqual({ type: 'select', index: 4 });
    expect(treeKeyAction(keyEvent('Home'), rows, 4)).toEqual({ type: 'select', index: 0 });
  });

  it('→で、閉じている項目は開き、開いている項目は最初の子を選ぶ', () => {
    expect(treeKeyAction(keyEvent('ArrowRight'), rows, 3)).toEqual({ type: 'expand', index: 3 });
    expect(treeKeyAction(keyEvent('ArrowRight'), rows, 0)).toEqual({ type: 'select', index: 1 });
    // 子のない項目では何もしない
    const leaf = keyEvent('ArrowRight');
    expect(treeKeyAction(leaf, rows, 1)).toBeNull();
    expect(leaf.defaultPrevented).toBe(false);
  });

  it('←で、開いている項目は閉じ、ほかの項目は親を選ぶ（根の項目では何もしない）', () => {
    expect(treeKeyAction(keyEvent('ArrowLeft'), rows, 0)).toEqual({ type: 'collapse', index: 0 });
    expect(treeKeyAction(keyEvent('ArrowLeft'), rows, 2)).toEqual({ type: 'select', index: 0 });
    expect(treeKeyAction(keyEvent('ArrowLeft'), rows, 3)).toBeNull();
    expect(treeKeyAction(keyEvent('ArrowLeft'), rows, 4)).toBeNull();
  });

  it('Enter・Spaceで現在位置の行を選び、何も選んでいない時は先頭の行を選ぶ', () => {
    expect(treeKeyAction(keyEvent('Enter'), rows, 2)).toEqual({ type: 'select', index: 2 });
    expect(treeKeyAction(keyEvent(' '), rows, -1)).toEqual({ type: 'select', index: 0 });
    expect(treeKeyAction(keyEvent('ArrowRight'), rows, -1)).toEqual({ type: 'select', index: 0 });
    expect(treeKeyAction(keyEvent('ArrowDown'), rows, -1)).toEqual({ type: 'select', index: 0 });
  });

  it('修飾キー・扱わないキー・行がない一覧では何もしない', () => {
    const withShift = keyEvent('ArrowDown', { shiftKey: true });
    expect(treeKeyAction(withShift, rows, 0)).toBeNull();
    expect(withShift.defaultPrevented).toBe(false);
    expect(treeKeyAction(keyEvent('a'), rows, 0)).toBeNull();
    expect(treeKeyAction(keyEvent('ArrowDown', { defaultPrevented: true }), rows, 0)).toBeNull();
    expect(treeKeyAction(keyEvent('ArrowDown'), [], -1)).toBeNull();
  });
});
