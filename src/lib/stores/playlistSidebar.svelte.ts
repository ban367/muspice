/**
 * サイドバーのプレイリストの一覧の状態（並び順と、閉じているフォルダ）
 *
 * どちらもlocalStorageに保存する（画面の表示の好みのため、Rust側の設定には入れない）。
 */
import { isPlaylistSort, type PlaylistSort } from '#lib/utils/playlistTree.js';

const SORT_KEY = 'muspice:playlistSort';
const COLLAPSED_KEY = 'muspice:collapsedPlaylistFolders';

function load<T>(key: string, parse: (value: unknown) => T | null, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return (raw === null ? null : parse(JSON.parse(raw))) ?? fallback;
  } catch {
    return fallback;
  }
}

function save(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 保存できなくても動作には影響しない
  }
}

const parseIds = (value: unknown): string[] | null =>
  Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : null;

class PlaylistSidebarState {
  // 既定は、これまでと同じ作成日の新しい順
  #sort = $state<PlaylistSort>(
    load(SORT_KEY, (value) => (isPlaylistSort(value) ? value : null), 'createdAt')
  );
  #collapsedFolderIds = $state.raw<string[]>(load(COLLAPSED_KEY, parseIds, []));

  /** プレイリストとフォルダの並び順（localStorageに保存） */
  get sort(): PlaylistSort {
    return this.#sort;
  }

  set sort(value: PlaylistSort) {
    this.#sort = value;
    save(SORT_KEY, value);
  }

  /** フォルダを閉じているか */
  isCollapsed(folderId: string): boolean {
    return this.#collapsedFolderIds.includes(folderId);
  }

  /** フォルダの開閉を切り替える（localStorageに保存） */
  toggleFolder(folderId: string): void {
    this.setCollapsed(folderId, !this.isCollapsed(folderId));
  }

  setCollapsed(folderId: string, collapsed: boolean): void {
    const others = this.#collapsedFolderIds.filter((id) => id !== folderId);
    this.#collapsedFolderIds = collapsed ? [...others, folderId] : others;
    save(COLLAPSED_KEY, this.#collapsedFolderIds);
  }
}

/** サイドバーのプレイリストの一覧の状態 */
export const playlistSidebar = new PlaylistSidebarState();

/**
 * プレイリストの情報（名前と説明）の編集画面の状態
 */
class PlaylistInfoDialogState {
  /** 編集するプレイリストのID（閉じている間はnull） */
  playlistId = $state<string | null>(null);

  open(playlistId: string): void {
    this.playlistId = playlistId;
  }

  close(): void {
    this.playlistId = null;
  }
}

/** プレイリストの情報の編集画面（メニュー・プレイリストの画面から開き、`(app)/+layout.svelte`が表示する） */
export const playlistInfoDialog = new PlaylistInfoDialogState();
