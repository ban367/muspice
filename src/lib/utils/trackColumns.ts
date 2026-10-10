/**
 * 曲の一覧の列
 *
 * 一覧に出せる列の定義と、表示する列の選択（表示・非表示・順番）の操作。
 * 番号の列（行の番号・再生中の印）は常に先頭に出すため、ここには含めない。
 */
import type { Track } from '#lib/types/models.js';
import { formatDate, formatDuration, formatFileSize } from './format.js';

/** 列のID（この並びが、列を選ぶメニューの順と、列を足した時の位置の基準になる） */
export const TRACK_COLUMN_IDS = [
  'title',
  'artist',
  'album',
  'albumArtist',
  'genre',
  'year',
  'trackNumber',
  'discNumber',
  'favorite',
  'rating',
  'playCount',
  'skipCount',
  'lastPlayedAt',
  'createdAt',
  'duration',
  'format',
  'bitrate',
  'sampleRate',
  'fileSize'
] as const;
export type TrackColumnId = (typeof TRACK_COLUMN_IDS)[number];

export interface TrackColumn {
  /** 既定の幅（px） */
  width: number;
  /** 幅を変える時の下限（px） */
  minWidth: number;
  align: 'left' | 'right' | 'center';
}

/** 列の定義 */
export const TRACK_COLUMNS: Record<TrackColumnId, TrackColumn> = {
  title: { width: 300, minWidth: 80, align: 'left' },
  artist: { width: 200, minWidth: 60, align: 'left' },
  album: { width: 200, minWidth: 60, align: 'left' },
  albumArtist: { width: 180, minWidth: 60, align: 'left' },
  genre: { width: 110, minWidth: 50, align: 'left' },
  year: { width: 56, minWidth: 44, align: 'right' },
  trackNumber: { width: 64, minWidth: 44, align: 'right' },
  discNumber: { width: 64, minWidth: 44, align: 'right' },
  favorite: { width: 28, minWidth: 28, align: 'center' },
  rating: { width: 80, minWidth: 80, align: 'center' },
  playCount: { width: 72, minWidth: 48, align: 'right' },
  skipCount: { width: 72, minWidth: 48, align: 'right' },
  lastPlayedAt: { width: 110, minWidth: 80, align: 'left' },
  createdAt: { width: 110, minWidth: 80, align: 'left' },
  duration: { width: 64, minWidth: 52, align: 'right' },
  format: { width: 64, minWidth: 48, align: 'left' },
  bitrate: { width: 88, minWidth: 64, align: 'right' },
  sampleRate: { width: 88, minWidth: 64, align: 'right' },
  fileSize: { width: 80, minWidth: 60, align: 'right' }
};

/** 既定で表示する列 */
export const DEFAULT_TRACK_COLUMNS: readonly TrackColumnId[] = [
  'title',
  'artist',
  'favorite',
  'rating',
  'duration'
];

const isColumnId = (value: unknown): value is TrackColumnId =>
  typeof value === 'string' && (TRACK_COLUMN_IDS as readonly string[]).includes(value);

/**
 * 保存していた列の一覧を、今ある列だけにする（重複・知らない列を除く）
 *
 * 配列でない・1つも残らない場合は、`fallback`を返す。
 */
export function normalizeColumns(
  value: unknown,
  fallback: readonly TrackColumnId[]
): TrackColumnId[] {
  if (!Array.isArray(value)) return [...fallback];
  const columns = [...new Set(value.filter(isColumnId))];
  return columns.length > 0 ? columns : [...fallback];
}

/**
 * 列の表示・非表示を切り替える
 *
 * 表示する列は、基準の並び（`TRACK_COLUMN_IDS`）で次に来る表示中の列の前に入れる
 * （なければ末尾）。最後の1列は、非表示にしない。
 */
export function toggleColumn(
  columns: readonly TrackColumnId[],
  id: TrackColumnId
): TrackColumnId[] {
  if (columns.includes(id)) {
    return columns.length > 1 ? columns.filter((column) => column !== id) : [...columns];
  }
  const later = TRACK_COLUMN_IDS.slice(TRACK_COLUMN_IDS.indexOf(id) + 1);
  const next = later.find((column) => columns.includes(column));
  const index = next === undefined ? columns.length : columns.indexOf(next);
  return [...columns.slice(0, index), id, ...columns.slice(index)];
}

/**
 * 列を、別の列の位置へ動かす（見出しのドラッグ）
 *
 * 右へ動かす時は相手の列の後ろに、左へ動かす時は相手の列の前に入る（落とした列の位置に来る）。
 */
export function moveColumn(
  columns: readonly TrackColumnId[],
  id: TrackColumnId,
  targetId: TrackColumnId
): TrackColumnId[] {
  const from = columns.indexOf(id);
  const to = columns.indexOf(targetId);
  if (from < 0 || to < 0 || from === to) return [...columns];
  const next = columns.filter((column) => column !== id);
  next.splice(to, 0, id);
  return next;
}

/** 文字で出す列の、セルの文字列（値がなければ空） */
export function trackCellText(track: Track, id: TrackColumnId): string {
  switch (id) {
    case 'title':
      return track.title || track.fileName;
    case 'artist':
    case 'album':
    case 'albumArtist':
    case 'genre':
      return track[id] ?? '';
    case 'year':
    case 'trackNumber':
    case 'discNumber':
      return track[id]?.toString() ?? '';
    case 'playCount':
    case 'skipCount':
      return track[id].toString();
    case 'lastPlayedAt':
      return track.lastPlayedAt ? formatDate(track.lastPlayedAt) : '';
    case 'createdAt':
      return formatDate(track.createdAt);
    case 'duration':
      return formatDuration(track.duration);
    case 'format':
      return track.format.toUpperCase();
    case 'bitrate':
      return track.bitrate ? `${track.bitrate} kbps` : '';
    case 'sampleRate':
      return track.sampleRate ? `${track.sampleRate / 1000} kHz` : '';
    case 'fileSize':
      return formatFileSize(track.fileSize);
    // ハート・星は、文字ではなく部品で出す
    case 'favorite':
    case 'rating':
      return '';
  }
}
