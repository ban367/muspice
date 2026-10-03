/**
 * システム操作（OSのブラウザ・ファイルマネージャー等）のミューテーション
 *
 * キャッシュには影響しないが、失敗をトーストで通知するためミューテーションとして扱う。
 */

import { createMutation } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { withErrorToast } from './shared';

/**
 * プロジェクトのページ（GitHub）を既定のブラウザで開くミューテーション
 */
export function useOpenProjectPageMutation() {
  return createMutation(() => ({
    mutationFn: () => withErrorToast('ページを開く', () => commands.openProjectPage())
  }));
}
