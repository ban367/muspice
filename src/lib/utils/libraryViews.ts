/**
 * ライブラリのビュー（最近追加した曲・フォルダ別・年代別）の集計
 *
 * どれも、取得済みの曲の一覧から計算する（ファイル・データベースは読まない）。
 */
import type { Track } from '#lib/types/models.js';
import { albumKey } from './albumKey.js';
import { compareNames } from './nameSort.js';

/** 曲をまとめるアーティスト（アルバムアーティスト。なければ曲のアーティスト） */
const groupArtist = (track: Track) => track.albumArtist ?? track.artist;

/** アルバムの中の曲の並び（ディスク番号 → トラック番号 → タイトル。Rustの`ALBUM_TRACK_ORDER`と同じ） */
function compareAlbumOrder(a: Track, b: Track): number {
  return (
    (a.discNumber ?? 1) - (b.discNumber ?? 1) ||
    (a.trackNumber ?? 0) - (b.trackNumber ?? 0) ||
    compareNames(a.title ?? '', b.title ?? '')
  );
}

/**
 * 曲を、アーティスト → アルバム → アルバムの中の並びの順に並べる（元の配列は変えない）
 *
 * フォルダ・年代の画面で、曲を渡す順に使う（名前は、読みがあれば読みの順）。
 */
export function sortTracksForBrowsing(tracks: readonly Track[]): Track[] {
  return [...tracks].sort(
    (a, b) =>
      compareNames(
        a.sortTags.albumArtist || a.sortTags.artist || groupArtist(a) || '',
        b.sortTags.albumArtist || b.sortTags.artist || groupArtist(b) || ''
      ) ||
      compareNames(groupArtist(a) ?? '', groupArtist(b) ?? '') ||
      compareNames(a.sortTags.album || a.album || '', b.sortTags.album || b.album || '') ||
      compareNames(a.album ?? '', b.album ?? '') ||
      compareAlbumOrder(a, b)
  );
}

// ---------- 最近追加した曲 ----------

/** 最近追加した項目（アルバム。アルバムのない曲は、1曲で1項目） */
export interface RecentItem {
  /** 項目を見分けるキー */
  key: string;
  kind: 'album' | 'track';
  /** アルバム名（曲の項目は、曲のタイトル） */
  name: string;
  /** アルバムをまとめたアーティスト（曲の項目は、曲のアーティスト） */
  artist: string | null;
  /** 追加した日時（アルバムは、その曲のうち最も新しいもの） */
  addedAt: string;
  trackCount: number;
  /** 合計の長さ（秒） */
  totalDuration: number;
  /** アルバムアートに使う曲（アルバムの最初の曲） */
  representativeTrackId: string;
  /** 曲の項目の、その曲 */
  track?: Track;
}

/**
 * 曲を、追加した日時の新しい順の項目にまとめる
 *
 * アルバムのある曲は、アルバム（アルバムアーティスト＋アルバム名）ごとに1項目にする。
 * アルバムに後から曲を足した場合は、足した日時の位置に来る。
 */
export function recentlyAdded(tracks: readonly Track[]): RecentItem[] {
  const items = new Map<string, RecentItem & { first: Track }>();

  for (const track of tracks) {
    if (track.album === null) {
      items.set(`track:${track.id}`, {
        key: `track:${track.id}`,
        kind: 'track',
        name: track.title || track.fileName,
        artist: track.artist,
        addedAt: track.createdAt,
        trackCount: 1,
        totalDuration: track.duration ?? 0,
        representativeTrackId: track.id,
        track,
        first: track
      });
      continue;
    }

    const artist = groupArtist(track);
    const key = `album:${albumKey({ name: track.album, artist })}`;
    const item = items.get(key);
    if (!item) {
      items.set(key, {
        key,
        kind: 'album',
        name: track.album,
        artist,
        addedAt: track.createdAt,
        trackCount: 1,
        totalDuration: track.duration ?? 0,
        representativeTrackId: track.id,
        first: track
      });
      continue;
    }
    item.trackCount++;
    item.totalDuration += track.duration ?? 0;
    if (track.createdAt > item.addedAt) item.addedAt = track.createdAt;
    if (compareAlbumOrder(track, item.first) < 0) {
      item.first = track;
      item.representativeTrackId = track.id;
    }
  }

  return [...items.values()]
    .sort(
      (a, b) =>
        (a.addedAt < b.addedAt ? 1 : a.addedAt > b.addedAt ? -1 : 0) || compareNames(a.name, b.name)
    )
    .map(({ first: _first, ...item }) => item);
}

// ---------- 木（フォルダ別・年代別の左の一覧） ----------

/** 木の1項目 */
export interface TreeNode {
  /** 項目を見分けるキー（フォルダはパス、年代は`decade:1990`・`year:1994`など） */
  key: string;
  label: string;
  /** この項目と、その下の項目の曲数 */
  count: number;
  children: TreeNode[];
}

/** 表示する行（開いている項目の下だけを並べたもの） */
export interface TreeRow {
  node: TreeNode;
  depth: number;
  hasChildren: boolean;
  isExpanded: boolean;
}

/** 木を、表示する行の並びにする（開いている項目の子だけを含める） */
export function flattenTree(nodes: readonly TreeNode[], expanded: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const visit = (node: TreeNode, depth: number) => {
    const hasChildren = node.children.length > 0;
    const isExpanded = hasChildren && expanded.has(node.key);
    rows.push({ node, depth, hasChildren, isExpanded });
    if (isExpanded) node.children.forEach((child) => visit(child, depth + 1));
  };
  nodes.forEach((node) => visit(node, 0));
  return rows;
}

// ---------- フォルダ別 ----------

const SEPARATOR = /[/\\]/;

/** パスの区切り（そのパスで使われているもの。Windowsのパスは`\`） */
const separatorOf = (path: string) => (path.includes('\\') && !path.includes('/') ? '\\' : '/');

/** 末尾の区切りを除く */
const trimSeparator = (path: string) => (path.length > 1 ? path.replace(/[/\\]+$/, '') : path);

/** 曲のあるフォルダ（ファイル名を除いたパス） */
export function folderOf(filePath: string): string {
  const index = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'));
  if (index < 0) return '';
  // ルートの直下のファイル（`/a.mp3`）のフォルダは、ルート
  return index === 0 ? filePath.slice(0, 1) : filePath.slice(0, index);
}

/** パスの最後の名前（フォルダ名。ルートなど、名前がなければパスそのもの） */
function baseName(path: string): string {
  return path.split(SEPARATOR).filter(Boolean).pop() ?? path;
}

/** `path`が、`folder`かその下のフォルダか */
export function isInFolder(path: string, folder: string): boolean {
  if (path === folder) return true;
  if (!path.startsWith(folder)) return false;
  // 名前が前方一致するだけの別のフォルダ（`/music`と`/music2`）を除く
  return SEPARATOR.test(folder.at(-1) ?? '') || SEPARATOR.test(path[folder.length] ?? '');
}

/**
 * 曲のあるフォルダを、ライブラリフォルダを根にした木にする
 *
 * - 根は、ライブラリフォルダ（曲のないものは出さない）。ライブラリフォルダの外にある曲は、
 *   その曲のフォルダを根にする
 * - 途中のフォルダは、曲がなくても（下のフォルダに曲があれば）出す
 * @param tracks - 曲の一覧
 * @param libraryFolders - ライブラリフォルダのパス
 */
export function buildFolderTree(
  tracks: readonly Track[],
  libraryFolders: readonly string[]
): TreeNode[] {
  // 入れ子のライブラリフォルダがあれば、深い方（長いパス）を先に試す
  const roots = [...new Set(libraryFolders.map(trimSeparator))].sort((a, b) => b.length - a.length);
  const nodes = new Map<string, TreeNode>();
  const rootNodes: TreeNode[] = [];

  const nodeFor = (path: string, label: string, parent: TreeNode | null): TreeNode => {
    let node = nodes.get(path);
    if (!node) {
      node = { key: path, label, count: 0, children: [] };
      nodes.set(path, node);
      (parent ? parent.children : rootNodes).push(node);
    }
    return node;
  };

  for (const track of tracks) {
    const folder = folderOf(track.filePath);
    const root = roots.find((candidate) => isInFolder(folder, candidate)) ?? folder;
    // 根の表示名は、フォルダ名（パスの全体は、キーで分かる）
    let node = nodeFor(root, baseName(root), null);
    node.count++;
    // 根から曲のフォルダまで、途中のフォルダをたどる
    const separator = separatorOf(folder);
    let path = root;
    for (const segment of folder.slice(root.length).split(SEPARATOR).filter(Boolean)) {
      path = SEPARATOR.test(path.at(-1) ?? '') ? path + segment : path + separator + segment;
      node = nodeFor(path, segment, node);
      node.count++;
    }
  }

  const sortNodes = (list: TreeNode[]) => {
    list.sort((a, b) => compareNames(a.label, b.label));
    list.forEach((node) => sortNodes(node.children));
  };
  sortNodes(rootNodes);
  return rootNodes;
}

/** フォルダとその下のフォルダにある曲（アーティスト → アルバムの順ではなく、場所の順） */
export function tracksInFolder(tracks: readonly Track[], folder: string): Track[] {
  return tracks
    .filter((track) => isInFolder(folderOf(track.filePath), folder))
    .sort((a, b) => compareNames(a.filePath, b.filePath));
}

// ---------- 年代別 ----------

/** 年のない曲の項目のキー */
export const UNKNOWN_YEAR_KEY = 'year:unknown';

const decadeOf = (year: number) => Math.floor(year / 10) * 10;

/**
 * 曲を、年代 → 年の木にする（新しい年代から。年のない曲は、最後の「年不明」）
 * @param labels - 表示名（年代・年不明は、言語に合わせた表記にする）
 */
export function buildYearTree(
  tracks: readonly Track[],
  labels: { decade: (decade: number) => string; unknown: string }
): TreeNode[] {
  const years = new Map<number, number>();
  let unknown = 0;
  for (const track of tracks) {
    if (track.year === null) unknown++;
    else years.set(track.year, (years.get(track.year) ?? 0) + 1);
  }

  const decades = new Map<number, TreeNode>();
  for (const [year, count] of [...years].sort(([a], [b]) => b - a)) {
    const decade = decadeOf(year);
    let node = decades.get(decade);
    if (!node) {
      node = { key: `decade:${decade}`, label: labels.decade(decade), count: 0, children: [] };
      decades.set(decade, node);
    }
    node.count += count;
    node.children.push({ key: `year:${year}`, label: String(year), count, children: [] });
  }

  const nodes = [...decades.values()];
  if (unknown > 0) {
    nodes.push({ key: UNKNOWN_YEAR_KEY, label: labels.unknown, count: unknown, children: [] });
  }
  return nodes;
}

/** 年代・年の項目の曲（アーティスト → アルバムの順） */
export function tracksInYear(tracks: readonly Track[], key: string): Track[] {
  const [kind, value] = key.split(':');
  const number = Number(value);
  const matches =
    key === UNKNOWN_YEAR_KEY
      ? (track: Track) => track.year === null
      : kind === 'decade'
        ? (track: Track) => track.year !== null && decadeOf(track.year) === number
        : (track: Track) => track.year === number;
  return sortTracksForBrowsing(tracks.filter(matches));
}
