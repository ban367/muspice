import type { ClientInit, HandleClientError } from '@sveltejs/kit/hooks';
import { reportError, watchUnhandledErrors } from '#lib/utils/errorLog.js';

export const init: ClientInit = async () => {
  // `npm run dev:mock`（Viteのmockモード）のときだけ、Tauri IPCをブラウザ用のモックに差し替える
  // 通常の開発・本番ビルドでは条件が定数のfalseになり、モック一式はバンドルに含まれない
  if (import.meta.env.MODE === 'mock') {
    const { setupTauriMock } = await import('#lib/mocks/tauri.js');
    setupTauriMock();
  }
  // どこにも受け止められなかったエラーを、ログファイルに残す（ADR-089）
  watchUnhandledErrors();
};

/**
 * 画面の読み込み・移動の間に起きたエラーを、ログファイルに残す（表示は`+error.svelte`）
 *
 * SvelteKit自身が出すエラー（存在しない画面など）は、エラーとして残さない。
 */
export const handleError: HandleClientError = (caught) => {
  if (caught.kind !== 'framework') {
    reportError(caught.error, `Navigation error (${caught.kind})`);
  }
};
