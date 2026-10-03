import { builtinEnvironments, type Environment } from 'vitest/runtime';

/**
 * Vitestのテスト環境: Nodeのグローバルのまま、モジュールをクライアント向け（Viteのclient環境）に変換する
 *
 * 組み込みのnode環境はSSR向けに変換するため、`*.svelte.ts`の`$state`・`$effect`がサーバー用の
 * コードになり、`$effect`が実行されない。アプリと同じクライアント用のコードでテストするために使う。
 */
export default {
  ...builtinEnvironments.node,
  name: 'node-client',
  viteEnvironment: 'client'
} satisfies Environment;
