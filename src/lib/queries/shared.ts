/**
 * クエリ／ミューテーション共通のヘルパーとキャッシュ方針
 */

import { QueryClient } from '@tanstack/svelte-query';
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
 * ネットワークの状態によらず実行する設定（`QueryClient`の、クエリとミューテーションの既定値に入れる）
 *
 * クエリ・ミューテーションが呼ぶのは、ローカルのコマンド（Tauri IPC）だけ。TanStack Queryの既定
 * （`online`）のままだと、Wi-Fiが切れるなどしてWebViewがオフラインになった時に、取得も保存も
 * 止まったままになる（まだ取得していない一覧は、空として表示される）。
 */
export const LOCAL_NETWORK_MODE = { networkMode: 'always' } as const;

/**
 * 表示した時に取り直すか（`refetchOnMount`）: 無効化されたクエリだけを取り直す
 *
 * `invalidateQueries`がすぐに取り直すのは、表示している（使う部品がある）クエリだけ。表示していない
 * クエリは印が付くだけで、次に表示した時に取り直すかは`refetchOnMount`で決まる。`false`では、
 * 再スキャンなどの間に開いていなかった画面（アルバムの一覧など）が、古いキャッシュを出し続ける。
 * `true`（TanStack Queryの既定）では、`staleTime`を過ぎただけのキャッシュも取り直すため、
 * 全曲の一覧（数万曲）を画面を移るたびに取り直しかねない。変更は無効化で知らせるため、無効化された
 * キャッシュだけを取り直せば足りる。
 */
export const refetchInvalidatedOnMount = (query: { state: { isInvalidated: boolean } }) =>
  query.state.isInvalidated;

/**
 * メインウィンドウの`QueryClient`を作る（`(app)/+layout.svelte`が1つだけ作る）
 */
export function createMainQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 10 * MINUTE, // 10分間キャッシュを新鮮とみなす
        gcTime: 30 * MINUTE, // 30分間メモリに保持
        retry: 1, // 失敗時に1回だけリトライ
        refetchOnWindowFocus: false, // ウィンドウフォーカス時の自動再取得を無効化
        refetchOnMount: refetchInvalidatedOnMount, // 表示した時は、無効化されたキャッシュだけを取り直す
        refetchOnReconnect: false, // ネットワークの再接続では再取得しない（外部と通信しないため）
        ...LOCAL_NETWORK_MODE
      },
      mutations: {
        retry: 0, // ミューテーションは失敗時にリトライしない
        ...LOCAL_NETWORK_MODE
      }
    }
  });
}

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
