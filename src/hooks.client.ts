import type { ClientInit } from '@sveltejs/kit/hooks';

export const init: ClientInit = async () => {
  // `npm run dev:mock`（Viteのmockモード）のときだけ、Tauri IPCをブラウザ用のモックに差し替える
  // 通常の開発・本番ビルドでは条件が定数のfalseになり、モック一式はバンドルに含まれない
  if (import.meta.env.MODE === 'mock') {
    const { setupTauriMock } = await import('#lib/mocks/tauri.js');
    setupTauriMock();
  }
};
