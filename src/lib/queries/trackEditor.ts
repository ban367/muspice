/**
 * 曲の情報の編集画面（別のウィンドウ。ADR-094）を開く操作と、編集画面で扱う曲の取得
 *
 * 編集画面は、メインウィンドウを親にした別のウィンドウで開き、開いている間はメインウィンドウを
 * 操作できない。編集する曲はRust側が預かり、編集画面のウィンドウは`useTrackEditorTracksQuery`で受け取る。
 * 保存した内容は、Rustが送る`TracksEdited`をメインウィンドウが受け取り、`applyTracksEdited`で反映する。
 */

import { createMutation, createQuery, type QueryClient } from '@tanstack/svelte-query';
import { commands } from '#lib/bindings.js';
import type { TracksEdited } from '#lib/types/models.js';
import { refreshAlbumArt } from '#lib/stores/albumArt.svelte.js';
import { ui } from '#lib/stores/ui.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { queryKeys } from './keys.js';
import { withErrorToast } from './shared.js';
import { invalidateTagQueries, invalidateTrackListQueries } from './tracks.js';

/**
 * 曲の情報の編集画面を開くミューテーション（曲のIDを渡す。すでに開いていれば、前面に出す）
 */
export function useOpenTrackEditorMutation() {
  return createMutation(() => ({
    mutationFn: openTrackEditor
  }));
}

/**
 * 曲の情報の編集画面を開く（右クリックのメニューと、メニューバーの「曲の情報を編集...」が使う）
 *
 * 失敗はトーストで知らせてから、エラーを投げ直す。
 */
export async function openTrackEditor(trackIds: string[]): Promise<void> {
  // 再生履歴など、同じ曲が何度も入る一覧から開く時は、1回にまとめる（開いた順は保つ）
  await withErrorToast(m.metadataEditor.openFailed, () =>
    commands.openTrackEditor([...new Set(trackIds)])
  );
  // 開いたら、すぐに覆いを重ねる（Rustの`TrackEditorChanged`も同じ値にする。閉じた時は、それで外す）
  ui.isTrackEditorOpen = true;
}

/**
 * 編集画面で扱う曲を、開いた時の順に取得するクエリ（編集画面のウィンドウで使う）
 *
 * 開いている間に一覧が変わっても、入力を保つため、取り直さない。
 */
export function useTrackEditorTracksQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.trackEditorTracks,
    queryFn: () => commands.getTrackEditorTracks(),
    staleTime: Infinity,
    retry: false
  }));
}

/**
 * 編集画面で保存した内容を、メインウィンドウに反映する（`TracksEdited`を受け取った時）
 *
 * 曲の一覧とタグ・歌詞を取り直し、アルバムアートを書き換えた曲は、表示中の画像を読み直させる。
 */
export function applyTracksEdited(queryClient: QueryClient, edited: TracksEdited): void {
  invalidateTrackListQueries(queryClient);
  invalidateTagQueries(queryClient);
  if (edited.albumArt) {
    refreshAlbumArt(edited.trackIds);
    void queryClient.invalidateQueries({ queryKey: queryKeys.albumArtInfo.all });
  }
}
