/**
 * トラックのドラッグ&ドロップ（曲の一覧から、サイドバーのプレイリストへ）
 *
 * HTML5のドラッグ&ドロップを使う。Tauriのファイルドロップ（`dragDropEnabled`）が有効だと
 * WebViewに`dragover`・`drop`が届かないため、`tauri.conf.json`で無効にしている。
 */

/** ドラッグ中のトラックIDを運ぶデータの種類（アプリの中だけで使う） */
export const TRACK_DRAG_TYPE = 'application/x-muspice-track-ids';

/**
 * トラックのドラッグを始める（`dragstart`から呼ぶ）
 * @param event - `dragstart`のイベント
 * @param trackIds - 運ぶトラックID（プレイリストへ追加する順）
 * @param label - ドラッグ中に表示する文言（例: 3曲）
 * @param effectAllowed - 許可する操作（並び替えにも使う一覧では`copyMove`）
 */
export function startTrackDrag(
  event: DragEvent,
  trackIds: readonly string[],
  label: string,
  effectAllowed: 'copy' | 'copyMove' = 'copy'
): void {
  const transfer = event.dataTransfer;
  if (!transfer) return;

  transfer.effectAllowed = effectAllowed;
  transfer.setData(TRACK_DRAG_TYPE, JSON.stringify(trackIds));

  // ドラッグ中に表示する画像（曲数）。画像として写し取られた後に取り除く
  const preview = document.createElement('div');
  preview.className = 'drag-preview';
  const icon = document.createElement('div');
  icon.className = 'drag-preview-icon';
  icon.innerHTML =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/></svg>';
  const count = document.createElement('span');
  count.className = 'drag-preview-count';
  count.textContent = label;
  preview.append(icon, count);
  document.body.appendChild(preview);
  transfer.setDragImage(preview, 40, 25);
  setTimeout(() => preview.remove(), 0);
}

/** トラックのドラッグか（`dragover`で、ドロップを受け付けるかの判定に使う） */
export function isTrackDrag(event: DragEvent): boolean {
  return event.dataTransfer?.types.includes(TRACK_DRAG_TYPE) ?? false;
}

/**
 * ドロップされたトラックIDを読む（`drop`から呼ぶ）
 * @returns トラックID（トラックのドラッグでない・読めない場合は空）
 */
export function readDraggedTrackIds(event: DragEvent): string[] {
  const data = event.dataTransfer?.getData(TRACK_DRAG_TYPE);
  if (!data) return [];
  try {
    const ids: unknown = JSON.parse(data);
    return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : [];
  } catch {
    return [];
  }
}
