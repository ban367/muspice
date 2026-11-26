/**
 * トラックのドラッグ&ドロップ（曲の一覧・アルバムなどの一覧から、サイドバーのプレイリストへ）
 *
 * HTML5のドラッグ&ドロップを使う。Tauriのファイルドロップ（`dragDropEnabled`）が有効だと
 * WebViewに`dragover`・`drop`が届かないため、`tauri.conf.json`で無効にしている。
 *
 * 運ぶものは2種類ある。
 * - 曲（トラックID）: 曲の一覧の行と、曲を取得済みのまとまり（アーティストの詳細のアルバムの見出しなど）
 * - 曲のまとまり（アルバム・アーティスト・ジャンル・アルバムアーティスト）: 曲を持たない一覧の
 *   カード・行。`dragstart`の間に曲を取得できないため、まとまりを運び、落とした時に曲を取得する
 */
import type { QueryClient } from '@tanstack/svelte-query';
import { fetchAlbumArtistTracks, fetchGroupTracks } from '#lib/queries/tracks.js';

/**
 * ドラッグ中のトラックIDを運ぶデータの種類（アプリの中だけで使う）
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export const TRACK_DRAG_TYPE = 'application/x-muspice-track-ids';

/**
 * ドラッグ中の曲のまとまりを運ぶデータの種類（アプリの中だけで使う）
 *
 * @internal テストのためにexportしている（アプリでは、このモジュールの中だけで使う）
 */
export const TRACK_GROUP_DRAG_TYPE = 'application/x-muspice-track-group';

/** ドラッグで運ぶ曲のまとまり（落とした時に、その曲を表示順で取得する） */
export type TrackDragGroup =
  | {
      type: 'album';
      name: string;
      /** アルバムをまとめたアーティスト（`AlbumSummary`の`artist`） */
      albumArtist: string | null;
    }
  | { type: 'artist' | 'genre'; name: string }
  | {
      /** アルバムの画面のリスト表示の左の一覧（アルバムアーティストのアルバムすべて） */
      type: 'albumArtist';
      /** アルバムをまとめたアーティスト（アーティストのないアルバムはnull） */
      name: string | null;
    };

/** ドロップされたもの（曲か、曲のまとまり） */
export type DraggedTracks =
  { kind: 'tracks'; trackIds: string[] } | { kind: 'group'; group: TrackDragGroup };

/** ドラッグ中に表示する文言 */
export interface TrackDragLabel {
  /** 主な文言（曲は曲数、まとまりは名前） */
  label: string;
  /** 添える文言（まとまりの曲数など） */
  detail?: string;
}

/**
 * ドラッグ中に表示する画像（文言）を作って渡す。画像として写し取られた後に取り除く
 */
function setDragPreview(transfer: DataTransfer, { label, detail }: TrackDragLabel): void {
  const preview = document.createElement('div');
  preview.className = 'drag-preview';
  const icon = document.createElement('div');
  icon.className = 'drag-preview-icon';
  icon.innerHTML =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/></svg>';
  const text = document.createElement('span');
  text.className = 'drag-preview-label';
  text.textContent = label;
  preview.append(icon, text);
  if (detail !== undefined) {
    const sub = document.createElement('span');
    sub.className = 'drag-preview-detail';
    sub.textContent = detail;
    preview.append(sub);
  }
  document.body.appendChild(preview);
  transfer.setDragImage(preview, 40, 25);
  setTimeout(() => preview.remove(), 0);
}

/**
 * トラックのドラッグを始める（`dragstart`から呼ぶ）
 * @param event - `dragstart`のイベント
 * @param trackIds - 運ぶトラックID（プレイリストへ追加する順）
 * @param label - ドラッグ中に表示する文言（例: 3曲。まとまりなら、名前と曲数）
 * @param effectAllowed - 許可する操作（並び替えにも使う一覧では`copyMove`）
 */
export function startTrackDrag(
  event: DragEvent,
  trackIds: readonly string[],
  label: string | TrackDragLabel,
  effectAllowed: 'copy' | 'copyMove' = 'copy'
): void {
  const transfer = event.dataTransfer;
  if (!transfer) return;

  transfer.effectAllowed = effectAllowed;
  transfer.setData(TRACK_DRAG_TYPE, JSON.stringify(trackIds));
  setDragPreview(transfer, typeof label === 'string' ? { label } : label);
}

/**
 * 曲のまとまり（アルバム・アーティストなど）のドラッグを始める（`dragstart`から呼ぶ）
 *
 * 曲を持たない一覧の項目から始める。曲は、落とした時に取得する（`resolveDraggedTrackIds`）。
 * @param event - `dragstart`のイベント
 * @param group - 運ぶまとまり
 * @param label - ドラッグ中に表示する文言（名前と曲数）
 */
export function startTrackGroupDrag(
  event: DragEvent,
  group: TrackDragGroup,
  label: TrackDragLabel
): void {
  const transfer = event.dataTransfer;
  if (!transfer) return;

  transfer.effectAllowed = 'copy';
  transfer.setData(TRACK_GROUP_DRAG_TYPE, JSON.stringify(group));
  setDragPreview(transfer, label);
}

/**
 * 曲（または曲のまとまり）のドラッグか（`dragover`で、ドロップを受け付けるかの判定に使う）
 */
export function isTrackDrag(event: DragEvent): boolean {
  const types = event.dataTransfer?.types ?? [];
  return types.includes(TRACK_DRAG_TYPE) || types.includes(TRACK_GROUP_DRAG_TYPE);
}

/** 文字列か */
const isString = (value: unknown): value is string => typeof value === 'string';

/** 運ばれてきたまとまりのデータを確かめる（形が違えばnull） */
function parseGroup(value: unknown): TrackDragGroup | null {
  if (typeof value !== 'object' || value === null) return null;
  const { type, name, albumArtist } = value as Record<string, unknown>;
  switch (type) {
    case 'album':
      return isString(name) && (albumArtist === null || isString(albumArtist))
        ? { type, name, albumArtist }
        : null;
    case 'artist':
    case 'genre':
      return isString(name) ? { type, name } : null;
    case 'albumArtist':
      return name === null || isString(name) ? { type, name } : null;
    default:
      return null;
  }
}

/** JSONを読む（読めなければundefined） */
function parseJson(data: string): unknown {
  try {
    return JSON.parse(data);
  } catch {
    return undefined;
  }
}

/**
 * ドロップされた曲（または曲のまとまり）を読む（`drop`から呼ぶ。データは`drop`の間しか読めない）
 * @returns 曲・まとまり（トラックのドラッグでない・読めない場合はnull）
 */
export function readDraggedTracks(event: DragEvent): DraggedTracks | null {
  const transfer = event.dataTransfer;
  if (!transfer) return null;

  const ids = transfer.getData(TRACK_DRAG_TYPE);
  if (ids) {
    const parsed = parseJson(ids);
    return Array.isArray(parsed) ? { kind: 'tracks', trackIds: parsed.filter(isString) } : null;
  }

  const group = transfer.getData(TRACK_GROUP_DRAG_TYPE);
  if (group) {
    const parsed = parseGroup(parseJson(group));
    return parsed === null ? null : { kind: 'group', group: parsed };
  }
  return null;
}

/**
 * ドロップされたもののトラックIDを、表示順で求める（まとまりは、その曲を取得する。キャッシュがあれば使う）
 *
 * 取得に失敗した場合は例外を投げる（失敗はクエリ側がトーストで通知する）。
 */
export async function resolveDraggedTrackIds(
  queryClient: QueryClient,
  dragged: DraggedTracks
): Promise<string[]> {
  if (dragged.kind === 'tracks') return dragged.trackIds;

  const { group } = dragged;
  const tracks =
    group.type === 'albumArtist'
      ? await fetchAlbumArtistTracks(queryClient, group.name)
      : await fetchGroupTracks(
          queryClient,
          group.type,
          group.name,
          group.type === 'album' ? group.albumArtist : null
        );
  return tracks.map((track) => track.id);
}
