/**
 * 曲の一覧の合計（曲数・時間・サイズ。一覧の下の表示に使う。ADR-114）
 */
import type { Track } from '#lib/types/models.js';

export interface TrackTotals {
  count: number;
  /** 長さの合計（秒。長さの分からない曲は0とする） */
  duration: number;
  /** ファイルのサイズの合計（バイト） */
  size: number;
}

/**
 * 曲の数・長さ・サイズの合計
 *
 * CUEシートの曲（アルバム1ファイルの中の曲）は、どの曲もファイル全体の大きさを持つため、同じファイルの
 * 曲は1回だけ数える（ADR-123）。
 */
export function trackTotals(
  tracks: readonly (Pick<Track, 'duration' | 'fileSize'> & { cue?: Track['cue'] })[]
): TrackTotals {
  let duration = 0;
  let size = 0;
  const countedSources = new Set<string>();
  for (const track of tracks) {
    duration += track.duration ?? 0;
    const source = track.cue?.source;
    if (source === undefined) {
      size += track.fileSize;
    } else if (!countedSources.has(source)) {
      countedSources.add(source);
      size += track.fileSize;
    }
  }
  return { count: tracks.length, duration, size };
}
