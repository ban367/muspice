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
    mutationFn: (variables: { trackIds: string[]; sequence?: readonly string[] }) =>
      openTrackEditor(variables.trackIds, variables.sequence)
  }));
}

/**
 * 曲の情報の編集画面を開く（右クリックのメニューと、メニューバーの「曲の情報を編集...」が使う）
 *
 * 失敗はトーストで知らせてから、エラーを投げ直す。
 * @param sequence 開いた一覧の曲の並び（1曲の編集で、編集画面から前後の曲へ移るのに使う。ADR-119）
 */
export async function openTrackEditor(
  trackIds: string[],
  sequence: readonly string[] = []
): Promise<void> {
  // 再生履歴など、同じ曲が何度も入る一覧から開く時は、1回にまとめる（開いた順は保つ）
  const ids = [...new Set(trackIds)];
  await withErrorToast(m.metadataEditor.openFailed, () =>
    commands.openTrackEditor(ids, ids.length === 1 ? [...new Set(sequence)] : [])
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
 * 編集している曲の、開いた一覧の並びでの位置を取得するクエリ（並びのない編集ではnull。ADR-119）
 */
export function useTrackEditorPositionQuery() {
  return createQuery(() => ({
    queryKey: queryKeys.trackEditorPosition,
    queryFn: () => commands.getTrackEditorPosition(),
    staleTime: Infinity,
    retry: false
  }));
}

/**
 * 編集する曲を、開いた一覧の並びの前後の曲へ移し、編集画面の曲と位置を取り直す（ADR-119）
 *
 * @returns 移ったか（並びの端では移らない）
 */
export async function moveTrackEditor(queryClient: QueryClient, offset: number): Promise<boolean> {
  const moved = await withErrorToast(m.metadataEditor.moveFailed, () =>
    commands.moveTrackEditor(offset)
  );
  if (moved) {
    await Promise.all([
      queryClient.refetchQueries({ queryKey: queryKeys.trackEditorTracks }),
      queryClient.refetchQueries({ queryKey: queryKeys.trackEditorPosition })
    ]);
  }
  return moved;
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
