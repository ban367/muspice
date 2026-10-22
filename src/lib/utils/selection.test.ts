import { describe, expect, it } from 'vitest';
import { computeClickSelection, resolveSelectedItem } from './selection';

const tracks = ['a', 'b', 'c', 'd', 'e'].map((id) => ({ id }));
const noModifiers = { shiftKey: false, toggleKey: false };

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
    // 起点を渡さない場合は、挿入順で最後の'd'
    const next = computeClickSelection(new Set(['e', 'd']), tracks, 'b', shift);
    expect([...next]).toEqual(['e', 'd', 'b', 'c']);
  });

  it('Shiftの範囲は、渡した起点から選ぶ（起点が選択にない場合も）', () => {
    const shift = { shiftKey: true, toggleKey: false };
    // 'a'〜'c'を選んだ後に'b'の選択だけを外した（起点は'b'）
    const next = computeClickSelection(new Set(['a', 'c']), tracks, 'd', shift, 'b');
    expect([...next].sort()).toEqual(['a', 'b', 'c', 'd']);
    // 起点が一覧にない場合は、最後に選択したトラックから
    const fallback = computeClickSelection(new Set(['a', 'c']), tracks, 'd', shift, 'x');
    expect([...fallback].sort()).toEqual(['a', 'c', 'd']);
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

describe('resolveSelectedItem', () => {
  const albums = [{ name: 'A' }, { name: 'B' }, { name: 'C' }];
  const byName = (album: { name: string }) => album.name;

  it('未選択なら先頭を返す', () => {
    expect(resolveSelectedItem(albums, null, byName)).toBe(albums[0]);
  });

  it('選択中のキーを持つアイテムを返す', () => {
    expect(resolveSelectedItem(albums, 'B', byName)).toBe(albums[1]);
  });

  it('データの再取得でオブジェクトが作り直されても、同じキーのアイテムを返す', () => {
    const refetched = albums.map((album) => ({ ...album }));
    expect(resolveSelectedItem(refetched, 'B', byName)).toBe(refetched[1]);
  });

  it('選択中のアイテムが一覧にない場合は先頭を返す', () => {
    expect(resolveSelectedItem(albums.slice(1), 'A', byName)).toBe(albums[1]);
  });

  it('一覧が空ならnullを返す', () => {
    expect(resolveSelectedItem([], 'A', byName)).toBeNull();
  });
});
