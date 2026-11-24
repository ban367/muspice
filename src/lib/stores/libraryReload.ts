/**
 * ライブラリの再読み込み（メニューバーの「ライブラリの再読み込み...」。ADR-128）
 *
 * すべての曲の情報（タイトル・評価など）を、ファイルから読み込み直す（`refresh_library_metadata`）。
 * 曲が多いと時間がかかるため、始めた時と終わった時にトーストで知らせる。確かめている間・読み込んでいる間に
 * もう一度選ばれても、始め直さない。
 */
import type { QueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import { withErrorToast } from '#lib/queries/shared.js';
import { invalidateTrackListQueries } from '#lib/queries/tracks.js';
import { showSuccess, showWarning } from '#lib/stores/error.svelte.js';
import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/** 再読み込みの状態（確かめている間・読み込んでいる間） */
let state: 'idle' | 'confirming' | 'running' = 'idle';

/** 確かめてから、ライブラリを再読み込みする（読み込んだら、曲の一覧のキャッシュを取り直させる） */
export async function reloadLibrary(queryClient: QueryClient): Promise<void> {
  if (state === 'running') {
    showSuccess(m.libraryReload.running);
    return;
  }
  if (state === 'confirming') return;

  state = 'confirming';
  try {
    // アプリ内だけにある内容（以前の版で保存した編集など）は、ファイルの内容に置き換わるため、確かめる
    if (!(await confirmDestructive(m.libraryReload.confirm, m.libraryReload.run))) return;

    state = 'running';
    showSuccess(m.libraryReload.running);
    const result = await withErrorToast(m.operations.reloadLibrary, () =>
      commands.refreshLibraryMetadata()
    ).catch(() => null);
    // 失敗はトーストで知らせ済み
    if (result === null) return;

    invalidateTrackListQueries(queryClient);
    const summary = m.libraryReload.done(result.skippedCount, result.errorCount);
    if (result.errorCount > 0) {
      showWarning(summary);
    } else {
      showSuccess(summary);
    }
  } finally {
    state = 'idle';
  }
}
