/**
 * 一覧から、引くための集まり（`Set`・`Map`）を作る
 *
 * コンポーネント（`.svelte`）の中では`Map`・`Set`を作らない規約のため（`$derived`の中で作り直すと、
 * 中身を書き換えても画面が変わらない`SvelteSet`との違いが分かりにくい）、ここで作る。返した集まりは
 * 読むだけで使う（書き換えない）。
 */

/** 空のIDの集まり（省略できる引数の既定値） */
export const EMPTY_ID_SET: ReadonlySet<string> = new Set();

/** 値の集まり（重複は1つにまとまる） */
export function setOf<T>(values: Iterable<T>): ReadonlySet<T> {
  return new Set(values);
}

/** 項目のIDの集まり */
export function idSetOf(items: readonly { id: string }[]): ReadonlySet<string> {
  return new Set(items.map((item) => item.id));
}

/** IDから項目を引く表（同じIDがあれば、後の項目） */
export function mapById<T extends { id: string }>(items: readonly T[]): ReadonlyMap<string, T> {
  return new Map(items.map((item) => [item.id, item]));
}

/** IDから一覧の中の位置を引く表（同じIDがあれば、後の位置） */
export function indexById(items: readonly { id: string }[]): ReadonlyMap<string, number> {
  return new Map(items.map((item, index) => [item.id, index]));
}
