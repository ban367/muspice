/**
 * 一覧の中での並べ替え（ドラッグ&ドロップ）
 */

/**
 * 運んだ項目を、落とした項目の位置へ動かした後の並びを返す（元の配列は変えない）
 *
 * - 運んだ項目が複数ある場合は、元の並びの順のまま、まとめて動かす
 * - 下へ動かす時は落とした項目の後ろに、上へ動かす時は前に入る（落とした行の位置に来る）
 * - 落とした項目が運んだ項目に含まれる・一覧にない場合は、何も変えない
 * @param order - 今の並び（項目のID）
 * @param movedIds - 運んだ項目のID
 * @param targetId - 落とした項目のID
 */
export function moveItems(
  order: readonly string[],
  movedIds: readonly string[],
  targetId: string
): string[] {
  const moved = new Set(movedIds);
  const movedInOrder = order.filter((id) => moved.has(id));
  const targetIndex = order.indexOf(targetId);
  if (movedInOrder.length === 0 || targetIndex < 0 || moved.has(targetId)) return [...order];

  // 運んだ項目のうち最初のものが、落とした項目より上にあれば、下へ動かしている
  const movingDown = order.indexOf(movedInOrder[0]) < targetIndex;
  const rest = order.filter((id) => !moved.has(id));
  const insertAt = rest.indexOf(targetId) + (movingDown ? 1 : 0);
  return [...rest.slice(0, insertAt), ...movedInOrder, ...rest.slice(insertAt)];
}
