import { describe, expect, it } from 'vitest';
import { listSelectionTarget, navigationTarget } from './listNavigation.js';

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

describe('listSelectionTarget', () => {
  function keyEvent(key: string, init: Partial<KeyboardEvent> = {}) {
    const event = {
      key,
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
