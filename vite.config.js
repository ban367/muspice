/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import adapter from '@sveltejs/adapter-static';
import tailwindcss from '@tailwindcss/vite';
import {
  buildLicenseList,
  collectNpmPackages,
  collectRustPackages
} from './scripts/third-party-licenses.mjs';

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const isVitest = Boolean(process.env.VITEST);

/** 依存ライブラリのライセンスの一覧を置く場所（ビルドした成果物の直下。アプリが`fetch`で読む） */
const LICENSES_FILE = 'third-party-licenses.json';

/**
 * アプリに組み込む依存ライブラリのライセンスの一覧を、ビルドの成果物に足す（ADR-091）
 *
 * - npm: クライアントのバンドルに入ったモジュールから集める。`tailwindcss`は、CSSの土台（preflight）が
 *   成果物に入るが、モジュールとしては現れないため、名前で足す
 * - Rust: `cargo metadata`で集める。`cargo`を呼べない時は、アプリのビルド（Tauriのビルド・CI）では
 *   失敗にし、手元の`npm run build`だけの時は、警告して、Rustの分がない一覧にする（画面が、その旨を出す）
 *
 * 受け入れるライセンスにないものが入っていたら、ビルドを失敗にする（`scripts/third-party-licenses.mjs`）。
 * @returns {import('vite').Plugin}
 */
function thirdPartyLicenses() {
  // @ts-expect-error process is a nodejs global
  const strict = Boolean(process.env.CI || process.env.TAURI_ENV_PLATFORM);
  return {
    name: 'muspice:third-party-licenses',
    apply: 'build',
    generateBundle(_options, bundle) {
      if (this.environment && this.environment.name !== 'client') return;
      const moduleIds = Object.values(bundle).flatMap((chunk) =>
        chunk.type === 'chunk' ? chunk.moduleIds : []
      );
      // このビルドで何もまとめていない（成果物のない段階）なら、一覧を作らない
      if (moduleIds.length === 0) return;
      const packages = collectNpmPackages(moduleIds, ['tailwindcss']);

      let target = null;
      let rustMissing = false;
      try {
        const rust = collectRustPackages();
        target = rust.target;
        packages.push(...rust.packages);
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        // ライセンスの検査の失敗は、いつも失敗にする（`cargo`を呼べない場合だけ、手元では続ける）
        const cargoUnavailable = /ENOENT|spawnSync|Command failed/.test(reason);
        if (strict || !cargoUnavailable) {
          this.error(`Cannot collect the licenses of the Rust dependencies: ${reason}`);
        }
        this.warn(
          `The license list does not include the Rust dependencies (cargo is not available): ${reason.split('\n')[0]}`
        );
        rustMissing = true;
      }

      this.emitFile({
        type: 'asset',
        fileName: LICENSES_FILE,
        source: JSON.stringify(buildLicenseList({ target, packages, rustMissing }))
      });
    }
  };
}

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
        fallback: 'index.html'
      })
    }),
    tailwindcss(),
    thirdPartyLicenses()
  ],

  // Vitest: ロジック（ストア・ユーティリティ）の単体テストをNode環境で実行する
  // （Runesの$effectを動かすため、Svelteはクライアント向けにコンパイルする。vitest.environment.ts参照）
  test: {
    include: ['src/**/*.test.ts', 'scripts/**/*.test.mjs'],
    environment: './vitest.environment.ts'
  },
  // テスト時はsvelte本体もクライアント用（browser条件）を読み込む（サーバー用では$effectが動かない）
  resolve: isVitest ? { conditions: ['browser'] } : undefined,

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
          protocol: 'ws',
          host,
          port: 1421
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**']
    }
  }
}));
