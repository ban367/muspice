/**
 * ライブラリのデータ（DB・設定）のバックアップ・復元のミューテーション
 *
 * 設定ウィンドウの「ライブラリ」で使う。保存先・復元するファイルはRust側のダイアログで選ぶ
 * （WebViewからパスを渡さない）。復元はアプリを再起動するため、キャッシュは読み直さない。
 */

import { createMutation } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { withErrorToast } from './shared.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/**
 * ライブラリのデータをバックアップするミューテーション
 *
 * 保存した結果（ファイルと件数）を返す。選ばずに閉じた場合はnull。
 */
export function useExportLibraryBackupMutation() {
  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.exportLibraryBackup, () => commands.exportLibraryBackup())
  }));
}

/**
 * 復元するファイルを選ぶミューテーション
 *
 * 復元できるか（できなければ、その理由）と中身の件数を返す（選ばずに閉じた場合はnull）。復元できる
 * ファイルはRust側が預かる。復元する時は`useRestoreLibraryBackupMutation`、やめる時は
 * `cancelLibraryRestore`。
 */
export function usePlanLibraryRestoreMutation() {
  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.planLibraryRestore, () => commands.planLibraryRestore())
  }));
}

/**
 * 預かっているファイルから復元するミューテーション（成功すると、アプリが再起動する）
 */
export function useRestoreLibraryBackupMutation() {
  return createMutation(() => ({
    mutationFn: () =>
      withErrorToast(m.operations.restoreLibraryBackup, () => commands.restoreLibraryBackup())
  }));
}

/** 預かっている復元するファイルを手放す（復元をやめた時。失敗しても画面は続ける） */
export function cancelLibraryRestore(): void {
  commands.cancelLibraryRestore().catch((error: unknown) => {
    console.error('Failed to release the pending restore file:', error);
  });
}
