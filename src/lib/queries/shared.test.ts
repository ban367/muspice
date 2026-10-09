import { describe, expect, it } from 'vitest';
import { combineQueryStates, type QueryStateLike } from './shared';

function state(overrides: Partial<QueryStateLike> = {}): QueryStateLike {
  return { isLoading: false, isError: false, error: null, ...overrides };
}

/** 読まれたプロパティを記録するクエリ（TanStack Queryが、読んだプロパティだけを通知の対象にするのを模す） */
function tracked(value: QueryStateLike): { query: QueryStateLike; read: Set<string> } {
  const read = new Set<string>();
  const query = new Proxy(value, {
    get(target, key, receiver) {
      read.add(String(key));
      return Reflect.get(target, key, receiver);
    }
  });
  return { query, read };
}

describe('combineQueryStates', () => {
  it('どれかが読み込み中なら、読み込み中にする', () => {
    expect(combineQueryStates(state(), state()).isLoading).toBe(false);
    expect(combineQueryStates(state(), state({ isLoading: true })).isLoading).toBe(true);
    expect(combineQueryStates(state({ isLoading: true }), state()).isLoading).toBe(true);
  });

  it('どれかがエラーなら、エラーにして最初のエラーを返す', () => {
    const first = new Error('first');
    const second = new Error('second');

    expect(combineQueryStates(state(), state())).toMatchObject({ isError: false, error: null });
    expect(
      combineQueryStates(
        state(),
        state({ isError: true, error: first }),
        state({ isError: true, error: second })
      )
    ).toMatchObject({ isError: true, error: first });
  });

  it('先のクエリが読み込み中・エラーでも、すべてのクエリの値を読む', () => {
    const loading = tracked(state({ isLoading: true, isError: true, error: new Error('x') }));
    const other = tracked(state());

    combineQueryStates(loading.query, other.query);

    // 読んでいない値は変わっても通知されないため、短絡評価で読み飛ばさない
    expect([...other.read].sort()).toEqual(['error', 'isError', 'isLoading']);
  });
});
