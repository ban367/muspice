import { describe, expect, it } from 'vitest';
import { computeClickSelection, toggleKeyboardSelection } from './selection';

const tracks = ['a', 'b', 'c', 'd', 'e'].map((id) => ({ id }));
const noModifiers = { shiftKey: false, toggleKey: false };

describe('toggleKeyboardSelection', () => {
  it('未選択のトラックはそのトラックだけを選択する', () => {
    expect([...toggleKeyboardSelection(new Set(['a', 'b']), 'c')]).toEqual(['c']);
  });

  it('選択中のトラックなら選択をすべて解除する', () => {
    expect(toggleKeyboardSelection(new Set(['a', 'b']), 'a').size).toBe(0);
  });
});

describe('computeClickSelection', () => {
  it('修飾キーなしではクリックしたトラックだけを選択する', () => {
    const next = computeClickSelection(new Set(['a', 'b']), tracks, 'c', noModifiers);
    expect([...next]).toEqual(['c']);
  });

  it('唯一の選択中トラックを修飾キーなしでクリックすると解除する', () => {
    const next = computeClickSelection(new Set(['a']), tracks, 'a', noModifiers);
    expect(next.size).toBe(0);
  });

  it('複数選択中に選択中のトラックをクリックするとそのトラックだけを選択する', () => {
    const next = computeClickSelection(new Set(['a', 'b']), tracks, 'a', noModifiers);
    expect([...next]).toEqual(['a']);
  });

  it('Ctrl/Cmdでトラックを追加・解除し、他の選択は維持する', () => {
    const toggle = { shiftKey: false, toggleKey: true };
    const added = computeClickSelection(new Set(['a']), tracks, 'c', toggle);
    expect([...added]).toEqual(['a', 'c']);

    const removed = computeClickSelection(added, tracks, 'a', toggle);
    expect([...removed]).toEqual(['c']);
  });

  it('Shiftで最後に選択したトラックからクリック位置までを追加する', () => {
    const shift = { shiftKey: true, toggleKey: false };
    const next = computeClickSelection(new Set(['b']), tracks, 'd', shift);
    expect([...next]).toEqual(['b', 'c', 'd']);
  });

  it('Shiftの範囲選択は表示順で上方向にも働き、既存の選択を残す', () => {
    const shift = { shiftKey: true, toggleKey: false };
    // 起点は挿入順で最後の'd'
    const next = computeClickSelection(new Set(['e', 'd']), tracks, 'b', shift);
    expect([...next]).toEqual(['e', 'd', 'b', 'c']);
  });

  it('Shiftでも選択が空なら単一選択になる', () => {
    const shift = { shiftKey: true, toggleKey: false };
    const next = computeClickSelection(new Set(), tracks, 'c', shift);
    expect([...next]).toEqual(['c']);
  });

  it('範囲の起点が一覧にない場合は選択を変えない', () => {
    const shift = { shiftKey: true, toggleKey: false };
    const next = computeClickSelection(new Set(['x']), tracks, 'c', shift);
    expect([...next]).toEqual(['x']);
  });

  it('引数のSetを変更せず新しいSetを返す', () => {
    const current = new Set(['a']);
    const next = computeClickSelection(current, tracks, 'b', { shiftKey: false, toggleKey: true });
    expect(next).not.toBe(current);
    expect([...current]).toEqual(['a']);
  });
});
