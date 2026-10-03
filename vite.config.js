/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import adapter from "@sveltejs/adapter-static";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const isVitest = Boolean(process.env.VITEST);

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    // SvelteKit 3ではsvelte.config.jsを使わず、設定をプラグインに渡す
    sveltekit({
      preprocess: vitePreprocess(),
      // TauriにはSSR用のNode.jsサーバーがないため、adapter-staticでindex.htmlに
      // フォールバックするSPAとしてビルドする
      // See: https://svelte.dev/docs/kit/single-page-apps
      // See: https://v2.tauri.app/start/frontend/sveltekit/
      adapter: adapter({
        fallback: "index.html",
      }),
    }),
    tailwindcss(),
  ],

  // Vitest: ロジック（ストア・ユーティリティ）の単体テストをNode環境で実行する
  // （Runesの$effectを動かすため、Svelteはクライアント向けにコンパイルする。vitest.environment.ts参照）
  test: {
    include: ["src/**/*.test.ts"],
    environment: "./vitest.environment.ts",
  },
  // テスト時はsvelte本体もクライアント用（browser条件）を読み込む（サーバー用では$effectが動かない）
  resolve: isVitest ? { conditions: ["browser"] } : undefined,

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
