/**
 * サイドバーのプレイリスト・オートプレイリストの一覧の状態（並び順と、閉じているフォルダ）
 *
 * どれもlocalStorageに保存する（画面の表示の好みのため、Rust側の設定には入れない）。
 */
import { isPlaylistSort, type PlaylistSort } from '#lib/utils/playlistTree.js';

const SORT_KEY = 'muspice:playlistSort';
const SMART_SORT_KEY = 'muspice:smartPlaylistSort';
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
  // 既定は、プレイリストの並び順（プレイリストと同じ一覧に並べていた時と、同じ順から始める）
  #smartSort = $state<PlaylistSort>(
    load(SMART_SORT_KEY, (value) => (isPlaylistSort(value) ? value : null), this.#sort)
  );

  /** プレイリストとフォルダの並び順（localStorageに保存） */
  get sort(): PlaylistSort {
    return this.#sort;
  }

  set sort(value: PlaylistSort) {
    this.#sort = value;
    save(SORT_KEY, value);
  }

  /** オートプレイリストの並び順（localStorageに保存。フォルダはない） */
  get smartSort(): PlaylistSort {
    return this.#smartSort;
  }

  set smartSort(value: PlaylistSort) {
    this.#smartSort = value;
    save(SMART_SORT_KEY, value);
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

/** サイドバーのプレイリスト・オートプレイリストの一覧の状態 */
export const playlistSidebar = new PlaylistSidebarState();

/** 名前と説明を編集するもの（プレイリスト・オートプレイリスト） */
export interface PlaylistInfoTarget {
  kind: 'playlist' | 'smartPlaylist';
  id: string;
}

/**
 * プレイリスト・オートプレイリストの情報（名前と説明）の編集画面の状態
 */
class PlaylistInfoDialogState {
  /** 編集するもの（閉じている間はnull） */
  target = $state.raw<PlaylistInfoTarget | null>(null);

  /** プレイリストの情報を編集する */
  open(playlistId: string): void {
    this.target = { kind: 'playlist', id: playlistId };
  }

  /** オートプレイリストの名前と説明を編集する */
  openSmartPlaylist(smartPlaylistId: string): void {
    this.target = { kind: 'smartPlaylist', id: smartPlaylistId };
  }

  close(): void {
    this.target = null;
  }
}

/** 情報の編集画面（メニュー・プレイリストの画面から開き、`(app)/+layout.svelte`が表示する） */
export const playlistInfoDialog = new PlaylistInfoDialogState();
