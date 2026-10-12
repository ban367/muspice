/**
 * OSからウィンドウへドロップされたファイルの扱い
 *
 * WebViewのドロップ（HTML5）では、ファイルのパスは得られない（名前と中身だけ）。インポートには、
 * 名前だけをRust側へ伝える（パスは、Rust側がOSから読む。`commands.importDroppedFiles`）。
 */

/** アプリの中のドラッグ（曲・プレイリスト・キュー・列）が運ぶデータの種類の、先頭の部分 */
const APP_DRAG_TYPE_PREFIX = 'application/x-muspice-';

/**
 * OSからのファイルのドラッグか
 *
 * アプリの中のドラッグ（曲の並べ替えなど）は含めない。ページの中の画像をドラッグした場合も
 * 「ファイル」として届くため、ページの中で始まったドラッグかどうかは、呼び出し側が
 * `dragstart`で見分ける。
 */
export function isFileDrag(dataTransfer: DataTransfer | null): boolean {
  if (!dataTransfer) return false;
  const types = Array.from(dataTransfer.types);
  return types.includes('Files') && !types.some((type) => type.startsWith(APP_DRAG_TYPE_PREFIX));
}

/** ドロップされたファイル・フォルダの名前（重複は除く） */
export function droppedNames(dataTransfer: DataTransfer): string[] {
  const names: string[] = [];
  const add = (name: string | undefined) => {
    if (name && !names.includes(name)) names.push(name);
  };

  // フォルダは`files`に入らないことがあるため、項目（エントリ）から名前を読む
  for (const item of Array.from(dataTransfer.items ?? [])) {
    if (item.kind !== 'file') continue;
    add(item.webkitGetAsEntry?.()?.name ?? item.getAsFile()?.name);
  }
  for (const file of Array.from(dataTransfer.files ?? [])) add(file.name);
  return names;
}
