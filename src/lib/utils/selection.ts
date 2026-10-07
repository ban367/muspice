/**
 * 一覧の選択状態の計算
 *
 * トラック一覧の複数選択では、いずれの関数も引数のSetを変更せず、新しいSetを返す
 * （`$state`へ再代入して更新を伝える）。Setの挿入順は「最後に選択したトラック」の判定
 * （Shift+クリックの範囲選択の起点）に使う。
 * 選択の状態とキーボードでの移動は`./trackSelection.svelte.ts`の`TrackSelection`が持つ。
 */

/** クリック時の修飾キー */
export interface SelectionModifiers {
  /** Shiftキー（範囲選択） */
  shiftKey: boolean;
  /** Ctrlキー（Windows/Linux）またはCmdキー（macOS）（個別の追加・解除） */
  toggleKey: boolean;
}

/**
 * クリックによる選択の更新
 *
 * - Shift: 最後に選択したトラックからクリックしたトラックまでを追加で選択
 * - Ctrl/Cmd: クリックしたトラックの選択を切り替え（他の選択は維持）
 * - 修飾キーなし: クリックしたトラックだけを選択（それが唯一の選択中トラックなら解除）
 * @param current - 現在の選択
 * @param orderedTracks - 表示順のトラック一覧（範囲選択に使用）
 * @param trackId - クリックしたトラックのID
 * @param modifiers - 修飾キーの状態
 */
export function computeClickSelection(
  current: ReadonlySet<string>,
  orderedTracks: readonly { id: string }[] | null,
  trackId: string,
  modifiers: SelectionModifiers
): Set<string> {
  if (modifiers.shiftKey && current.size > 0 && orderedTracks) {
    const lastSelectedId = Array.from(current).pop();
    const lastIndex = orderedTracks.findIndex((t) => t.id === lastSelectedId);
    const currentIndex = orderedTracks.findIndex((t) => t.id === trackId);

    if (lastIndex === -1 || currentIndex === -1) {
      return new Set(current);
    }

    const start = Math.min(lastIndex, currentIndex);
    const end = Math.max(lastIndex, currentIndex);
    return new Set([...current, ...orderedTracks.slice(start, end + 1).map((t) => t.id)]);
  }

  if (modifiers.toggleKey) {
    return current.has(trackId)
      ? new Set([...current].filter((id) => id !== trackId))
      : new Set([...current, trackId]);
  }

  return current.has(trackId) && current.size === 1 ? new Set() : new Set([trackId]);
}

/**
 * 2ペイン表示（一覧＋詳細）で詳細に表示するアイテムを決める
 *
 * 選択はアイテムのオブジェクトではなくキーで保持する。データの再取得でオブジェクトが
 * 作り直されても、同じキーのアイテムを選択し続けられる。
 * - 選択中のキーを持つアイテムがあればそれを返す
 * - 未選択、または選択中のアイテムが一覧にない（検索で絞り込まれた等）場合は先頭を返す
 * - 一覧が空ならnullを返す
 * @param items - 表示中のアイテム一覧
 * @param selectedKey - 選択中のアイテムのキー（未選択ならnull）
 * @param getKey - アイテムからキーを取り出す関数
 */
export function resolveSelectedItem<T, K>(
  items: readonly T[],
  selectedKey: K | null,
  getKey: (item: T) => K
): T | null {
  const selected =
    selectedKey === null ? undefined : items.find((item) => getKey(item) === selectedKey);
  return selected ?? items[0] ?? null;
}
