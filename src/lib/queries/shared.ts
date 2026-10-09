/**
 * クエリ／ミューテーション共通のヘルパーとキャッシュ方針
 */

import { handleError } from '#lib/stores/error.svelte.js';

const MINUTE = 60 * 1000;

/**
 * データの性質ごとのキャッシュ方針
 *
 * `staleTime`は再取得なしで使い続ける時間、`gcTime`はメモリに保持する時間。
 * 個々のクエリで数値を書かず、ここの方針を選んで適用する。
 */
export const CACHE_POLICY = {
  /** ライブラリ本体（全曲の一覧・アルバムなどの一覧・ユニーク値）: 変更はインポート/編集時のみ */
  library: { staleTime: 10 * MINUTE, gcTime: 30 * MINUTE },
  /** 開いたアルバム・アーティスト・ジャンル・プレイリストの曲: 開くたびに増えるため、閉じたら早めに手放す */
  detail: { staleTime: 10 * MINUTE, gcTime: 5 * MINUTE },
  /** 検索・フィルタ結果: 入力ごとに増えるため保持は短め */
  search: { staleTime: 5 * MINUTE, gcTime: 15 * MINUTE },
  /** 再生統計（お気に入り・よく再生する） */
  playStats: { staleTime: 5 * MINUTE, gcTime: 15 * MINUTE },
  /** 頻繁に変わるデータ（再生履歴） */
  volatile: { staleTime: 1 * MINUTE, gcTime: 5 * MINUTE }
} as const;

/**
 * コマンド呼び出しのエラーをトースト通知しつつ、Queryへ再スローする
 *
 * TanStack Queryにエラー状態を伝えるため、通知後も必ず再スローする。
 *
 * @param context ユーザーに表示する操作名（例: 'トラック一覧の取得'）
 * @param run 実行するコマンド呼び出し
 */
export async function withErrorToast<T>(context: string, run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    handleError(error, context);
    throw error;
  }
}

/** `combineQueryStates`が読む、クエリの状態 */
export interface QueryStateLike {
  readonly isLoading: boolean;
  readonly isError: boolean;
  readonly error: Error | null;
}

/**
 * 複数のクエリの状態を1つにまとめる（どれかが読み込み中・エラーなら、そうなる）
 *
 * TanStack Queryは、読んだことのあるプロパティが変わった時だけ通知する。そのため
 * `a.isLoading || b.isLoading`のように短絡評価でまとめると、`a`の読み込み中は`b.isLoading`を
 * 読まず、`b`が先に終わった時の通知を受け取れない（読み込み中の表示のまま止まる）。
 * ここでは、すべてのクエリの値を先に読んでからまとめる。`$derived`の中で呼ぶ。
 */
export function combineQueryStates(...queries: readonly QueryStateLike[]): QueryStateLike {
  const states = queries.map((query) => ({
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error
  }));
  return {
    isLoading: states.some((state) => state.isLoading),
    isError: states.some((state) => state.isError),
    error: states.find((state) => state.error !== null)?.error ?? null
  };
}
