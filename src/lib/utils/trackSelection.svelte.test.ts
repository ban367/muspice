import { describe, expect, it } from 'vitest';
import { TrackSelection } from './trackSelection.svelte.js';

const tracks = ['t1', 't2', 't3', 't4', 't5'].map((id) => ({ id }));
const plain = { shiftKey: false, toggleKey: false };

function createSelection(list = tracks) {
  return new TrackSelection(() => list);
}

describe('TrackSelection.move', () => {
  it('選択がない場合、↓で先頭・↑で末尾を選択する', () => {
    const down = createSelection();
    expect(down.move('ArrowDown', false)).toBe('t1');
    expect([...down.ids]).toEqual(['t1']);

    const up = createSelection();
    expect(up.move('ArrowUp', false)).toBe('t5');
    expect([...up.ids]).toEqual(['t5']);
  });

  it('クリックで選択したトラックから、隣のトラックへ選択を移す', () => {
    const selection = createSelection();
    selection.click('t2', plain);

    expect(selection.move('ArrowDown', false)).toBe('t3');
    expect([...selection.ids]).toEqual(['t3']);
    expect(selection.activeId).toBe('t3');

    expect(selection.move('ArrowUp', false)).toBe('t2');
    expect(selection.move('ArrowUp', false)).toBe('t1');
    // 端ではとどまる
    expect(selection.move('ArrowUp', false)).toBe('t1');
    expect([...selection.ids]).toEqual(['t1']);
  });

  it('Shift+矢印で、起点から移動先までを選択する', () => {
    const selection = createSelection();
    selection.click('t3', plain);

    selection.move('ArrowDown', true);
    selection.move('ArrowDown', true);
    expect([...selection.ids]).toEqual(['t3', 't4', 't5']);

    // 戻ると範囲が縮み、起点を越えると反対側へ伸びる
    selection.move('ArrowUp', true);
    expect([...selection.ids]).toEqual(['t3', 't4']);
    selection.move('ArrowUp', true);
    selection.move('ArrowUp', true);
    expect([...selection.ids].sort()).toEqual(['t2', 't3']);
    expect(selection.activeId).toBe('t2');
  });

  it('Shiftなしで動かすと、範囲選択を解いて移動先だけを選択する', () => {
    const selection = createSelection();
    selection.click('t2', plain);
    selection.move('ArrowDown', true);
    selection.move('ArrowDown', true);

    expect(selection.move('ArrowDown', false)).toBe('t5');
    expect([...selection.ids]).toEqual(['t5']);
  });

  it('グリッドでは列数に応じて上下の行へ移る', () => {
    const selection = createSelection();
    selection.click('t1', plain);

    expect(selection.move('ArrowRight', false, 3)).toBe('t2');
    expect(selection.move('ArrowDown', false, 3)).toBe('t5');
    expect(selection.move('ArrowUp', false, 3)).toBe('t2');
  });

  it('扱わないキー・空の一覧では何もしない', () => {
    const selection = createSelection();
    selection.click('t2', plain);
    expect(selection.move('ArrowLeft', false)).toBeNull();
    expect(selection.move('x', false)).toBeNull();
    expect([...selection.ids]).toEqual(['t2']);

    expect(createSelection([]).move('ArrowDown', false)).toBeNull();
  });

  it('現在位置のトラックが一覧からなくなった場合は、端から選び直す', () => {
    let list = tracks;
    const selection = new TrackSelection(() => list);
    selection.click('t2', plain);
    list = tracks.filter((track) => track.id !== 't2');

    expect(selection.move('ArrowDown', false)).toBe('t1');
  });
});

describe('TrackSelection（クリック・その他）', () => {
  it('Shift+クリックの後も、キーボードの移動はクリックした位置から続く', () => {
    const selection = createSelection();
    selection.click('t1', plain);
    selection.click('t3', { shiftKey: true, toggleKey: false });
    expect([...selection.ids].sort()).toEqual(['t1', 't2', 't3']);

    expect(selection.move('ArrowDown', false)).toBe('t4');
  });

  it('ensureSelectedは、選択中のトラックなら選択を変えない', () => {
    const selection = createSelection();
    selection.click('t1', plain);
    selection.click('t2', { shiftKey: false, toggleKey: true });

    selection.ensureSelected('t2');
    expect([...selection.ids]).toEqual(['t1', 't2']);

    selection.ensureSelected('t4');
    expect([...selection.ids]).toEqual(['t4']);
  });

  it('beginDragは、選択中のトラックから始めると選択中のすべてを表示順で返す', () => {
    const selection = createSelection();
    // 選んだ順は t4 → t2 → t3
    selection.click('t4', plain);
    selection.click('t2', { shiftKey: false, toggleKey: true });
    selection.click('t3', { shiftKey: false, toggleKey: true });

    expect(selection.beginDrag('t2')).toEqual(['t2', 't3', 't4']);
    expect(selection.size).toBe(3);
  });

  it('beginDragは、選択中でないトラックから始めるとそのトラックだけを選択して返す', () => {
    const selection = createSelection();
    selection.click('t1', plain);
    selection.click('t2', { shiftKey: false, toggleKey: true });

    expect(selection.beginDrag('t5')).toEqual(['t5']);
    expect([...selection.ids]).toEqual(['t5']);
  });

  it('selectAllで一覧のすべてを選択し、clearで解除する', () => {
    const selection = createSelection();
    selection.click('t2', plain);

    selection.selectAll();
    expect(selection.size).toBe(5);

    selection.clear();
    expect(selection.size).toBe(0);
    // 現在位置は残り、続けて移動できる
    expect(selection.move('ArrowDown', false)).toBe('t3');
  });
});
