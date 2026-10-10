/**
 * プレイリストの一覧（フォルダ分けと並び順）
 *
 * フォルダは1階層（フォルダの中にフォルダはない）。一覧には、フォルダを先に、フォルダの外の
 * プレイリストを後に出す。並び順は、名前・作成日（新しい順）・手動から選ぶ。
 */
import type { Playlist, PlaylistFolder } from '#lib/types/models.js';
import { compareNames } from './nameSort.js';

/** プレイリストの並び順 */
export const PLAYLIST_SORTS = ['name', 'createdAt', 'manual'] as const;
export type PlaylistSort = (typeof PLAYLIST_SORTS)[number];

export const isPlaylistSort = (value: unknown): value is PlaylistSort =>
  (PLAYLIST_SORTS as readonly unknown[]).includes(value);

/** 並べる項目（プレイリスト・フォルダ）に共通の値 */
interface Sortable {
  id: string;
  name: string;
  position: number;
  createdAt: string;
}

/** 作成日の新しい順（同じ日時なら、IDの順で決める） */
const compareNewestFirst = (a: Sortable, b: Sortable) =>
  b.createdAt.localeCompare(a.createdAt) || a.id.localeCompare(b.id);

/**
 * プレイリスト（またはフォルダ）を、選んだ並び順に並べる（元の配列は変えない）
 *
 * 手動の並び順で位置が同じもの（手動で並べたことがないもの）は、作成日の新しい順にする。
 */
export function sortPlaylistItems<T extends Sortable>(
  items: readonly T[],
  sort: PlaylistSort
): T[] {
  const compare: (a: T, b: T) => number =
    sort === 'name'
      ? (a, b) => compareNames(a.name, b.name) || compareNewestFirst(a, b)
      : sort === 'manual'
        ? (a, b) => a.position - b.position || compareNewestFirst(a, b)
        : compareNewestFirst;
  return [...items].sort(compare);
}

/** フォルダと、その中のプレイリスト */
export interface PlaylistFolderNode {
  folder: PlaylistFolder;
  playlists: Playlist[];
}

/** フォルダ分けしたプレイリストの一覧 */
export interface PlaylistTree {
  folders: PlaylistFolderNode[];
  /** フォルダの外のプレイリスト */
  root: Playlist[];
}

/**
 * プレイリストを、フォルダ分けして並べる
 *
 * 入っているフォルダが見つからないプレイリストは、フォルダの外に出す。
 */
export function buildPlaylistTree(
  folders: readonly PlaylistFolder[],
  playlists: readonly Playlist[],
  sort: PlaylistSort
): PlaylistTree {
  const sorted = sortPlaylistItems(playlists, sort);
  const nodes = sortPlaylistItems(folders, sort).map((folder) => ({
    folder,
    playlists: sorted.filter((playlist) => playlist.folderId === folder.id)
  }));
  const folderIds = folders.map((folder) => folder.id);
  return {
    folders: nodes,
    root: sorted.filter(
      (playlist) => playlist.folderId === null || !folderIds.includes(playlist.folderId)
    )
  };
}

/** フォルダ（nullはフォルダの外）の中のプレイリスト */
export function playlistsIn(tree: PlaylistTree, folderId: string | null): Playlist[] {
  if (folderId === null) return tree.root;
  return tree.folders.find((node) => node.folder.id === folderId)?.playlists ?? [];
}

/**
 * 項目を、別の項目の前（または後ろ）へ動かした後の並びを返す（元の配列は変えない）
 *
 * 動かす項目が一覧になくてもよい（別のフォルダから運んできた場合）。落とした項目が一覧にない・
 * 動かす項目と同じ場合は、何も変えない。
 * @param order - 今の並び（項目のID）
 * @param movedId - 動かす項目のID
 * @param targetId - 落とした項目のID
 * @param after - 落とした項目の後ろに入れるか
 */
export function placeRelative(
  order: readonly string[],
  movedId: string,
  targetId: string,
  after: boolean
): string[] {
  if (movedId === targetId || !order.includes(targetId)) return [...order];
  const rest = order.filter((id) => id !== movedId);
  const insertAt = rest.indexOf(targetId) + (after ? 1 : 0);
  return [...rest.slice(0, insertAt), movedId, ...rest.slice(insertAt)];
}
