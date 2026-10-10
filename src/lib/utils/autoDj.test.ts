import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import { pickAutoDjTracks } from './autoDj.js';

function track(id: string, isMissing = false): Track {
  return { id, title: id, isMissing } as Track;
}

const ids = (tracks: readonly Track[]) => tracks.map((t) => t.id);

/** 決まった順に値を返す乱数（足りなくなったら0） */
function sequence(...values: number[]): () => number {
  let index = 0;
  return () => values[index++] ?? 0;
}

describe('pickAutoDjTracks', () => {
  const library = ['a', 'b', 'c', 'd', 'e'].map((id) => track(id));

  it('再生キューにない曲から、指定した数だけ選ぶ', () => {
    const picked = pickAutoDjTracks(library, [track('a'), track('c')], 2, sequence(0, 0));

    expect(ids(picked)).toEqual(['b', 'd']);
  });

  it('乱数に従って選び、同じ曲を2回選ばない', () => {
    // 候補: b, c, d, e。0.99は、残っている候補の最後（先に選んだ曲と入れ替わった曲）を選ぶ
    const picked = pickAutoDjTracks(library, [track('a')], 3, sequence(0.99, 0.99, 0.99));

    expect(ids(picked)).toEqual(['e', 'b', 'c']);
    expect(new Set(ids(picked)).size).toBe(3);
  });

  it('候補が足りなければ、ある分だけ選ぶ', () => {
    const queue = ['a', 'b', 'c', 'd'].map((id) => track(id));

    expect(ids(pickAutoDjTracks(library, queue, 3))).toEqual(['e']);
  });

  it('ファイルが見つからない曲は選ばない', () => {
    const candidates = [track('a', true), track('b'), track('c', true)];

    expect(ids(pickAutoDjTracks(candidates, [], 3))).toEqual(['b']);
  });

  it('元の曲がすべてキューにある場合は、最後の曲だけを避けて選び直す', () => {
    const queue = ['c', 'a', 'b', 'd', 'e'].map((id) => track(id));

    const picked = pickAutoDjTracks(library, queue, 10, sequence(0, 0, 0, 0));
    expect(ids(picked)).toEqual(['a', 'b', 'c', 'd']);
    expect(ids(picked)).not.toContain('e');
  });

  it('選べる曲がなければ、空の配列を返す', () => {
    expect(pickAutoDjTracks([], [track('a')], 3)).toEqual([]);
    // 元の曲が、いま再生している1曲だけ
    expect(pickAutoDjTracks([track('a')], [track('a')], 3)).toEqual([]);
    expect(pickAutoDjTracks(library, [], 0)).toEqual([]);
  });

  it('元の配列を変えない', () => {
    const before = ids(library);
    pickAutoDjTracks(library, [], 5, sequence(0.9, 0.5, 0.1));

    expect(ids(library)).toEqual(before);
  });
});
