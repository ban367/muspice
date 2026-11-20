import { QueryObserver, type QueryClient } from '@tanstack/svelte-query';
import { describe, expect, it, vi } from 'vitest';
import { combineQueryStates, createMainQueryClient, type QueryStateLike } from './shared.js';

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

describe('createMainQueryClient', () => {
  const key = ['albums', 'list'] as const;

  /** クエリを表示する（使う部品を付ける）。外す関数を返す */
  function show(client: QueryClient, queryFn: () => Promise<string[]>): () => void {
    return new QueryObserver(client, { queryKey: key, queryFn }).subscribe(() => {});
  }

  it('表示していない間に無効化したクエリは、次に表示した時に取り直す', async () => {
    const client = createMainQueryClient();
    client.setQueryData(key, ['old']);

    // 再スキャンなどで、表示していないアルバムの一覧を無効化する（すぐには取り直さない）
    await client.invalidateQueries({ queryKey: key });
    expect(client.getQueryData(key)).toEqual(['old']);

    const queryFn = vi.fn(async () => ['new']);
    const hide = show(client, queryFn);
    await vi.waitFor(() => expect(client.getQueryData(key)).toEqual(['new']));
    expect(queryFn).toHaveBeenCalledTimes(1);
    hide();
  });

  it('無効化していないキャッシュは、表示しても取り直さない', async () => {
    vi.useFakeTimers();
    try {
      const client = createMainQueryClient();
      client.setQueryData(key, ['cached']);
      // 新鮮とみなす時間（10分）を過ぎても、無効化されるまでは使い続ける（30分で手放す前）
      vi.advanceTimersByTime(20 * 60 * 1000);

      const queryFn = vi.fn(async () => ['new']);
      const hide = show(client, queryFn);
      await vi.advanceTimersByTimeAsync(0);
      expect(queryFn).not.toHaveBeenCalled();
      expect(client.getQueryData(key)).toEqual(['cached']);
      hide();
    } finally {
      vi.useRealTimers();
    }
  });
});
