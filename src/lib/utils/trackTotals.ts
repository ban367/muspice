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

/** 曲の数・長さ・サイズの合計 */
export function trackTotals(tracks: readonly Pick<Track, 'duration' | 'fileSize'>[]): TrackTotals {
  let duration = 0;
  let size = 0;
  for (const track of tracks) {
    duration += track.duration ?? 0;
    size += track.fileSize;
  }
  return { count: tracks.length, duration, size };
}
