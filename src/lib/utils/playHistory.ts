/**
 * 再生履歴の一覧の組み立て
 *
 * 再生履歴（再生した日時とトラックID。新しい順）を、全曲の一覧と突き合わせて、日付の見出しと
 * 1回の再生を並べた行にする。同じ曲が何度も出るため、行は履歴のIDで見分ける。
 */
import type { PlayHistoryEntry, Track } from '#lib/types/models.js';

/** 日付の見出し */
export interface PlayHistoryDateRow {
  kind: 'date';
  /** 行を見分けるID（日付ごと） */
  id: string;
  /** その日の0時（表示している環境のタイムゾーン） */
  date: Date;
  /** その日に再生した回数 */
  count: number;
}

/** 1回の再生 */
export interface PlayHistoryPlayRow {
  kind: 'play';
  /** 行を見分けるID（履歴の1件ごと。同じ曲でも違う） */
  id: string;
  /** 再生回数に数えた日時 */
  playedAt: Date;
  track: Track;
}

export type PlayHistoryRow = PlayHistoryDateRow | PlayHistoryPlayRow;

/** 日付（表示している環境のタイムゾーン）ごとのキー */
function dateKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

/**
 * 再生履歴を、日付の見出しと再生の行にする
 *
 * ライブラリにない曲（一覧を取り直す前に外した曲）と、日時を読めない履歴は除く。
 * @param entries - 再生履歴（新しい順）
 * @param tracks - 全曲の一覧
 */
export function buildPlayHistoryRows(
  entries: readonly PlayHistoryEntry[],
  tracks: readonly Track[]
): PlayHistoryRow[] {
  const tracksById = new Map(tracks.map((track) => [track.id, track]));
  const rows: PlayHistoryRow[] = [];
  let currentDate: PlayHistoryDateRow | null = null;

  for (const entry of entries) {
    const track = tracksById.get(entry.trackId);
    const playedAt = new Date(entry.playedAt);
    if (!track || Number.isNaN(playedAt.getTime())) continue;

    const key = dateKey(playedAt);
    if (currentDate?.id !== `date:${key}`) {
      currentDate = {
        kind: 'date',
        id: `date:${key}`,
        date: new Date(playedAt.getFullYear(), playedAt.getMonth(), playedAt.getDate()),
        count: 0
      };
      rows.push(currentDate);
    }
    currentDate.count++;
    rows.push({ kind: 'play', id: `play:${entry.id}`, playedAt, track });
  }

  return rows;
}

/**
 * 行に出てくる曲を、重複なく、出てくる順（最近再生した順）に並べる
 *
 * 再生キューに入れる・右クリックのメニューの対象にする時に使う（キューには、同じ曲を
 * 2回入れない）。
 */
export function distinctTracks(rows: readonly PlayHistoryRow[]): Track[] {
  const seen = new Set<string>();
  const tracks: Track[] = [];
  for (const row of rows) {
    if (row.kind !== 'play' || seen.has(row.track.id)) continue;
    seen.add(row.track.id);
    tracks.push(row.track);
  }
  return tracks;
}

/** その日が、`now`から見て何日前か（今日は0、昨日は1） */
export function daysAgo(date: Date, now: Date): number {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  // 夏時間の切り替わりで1日が24時間でない日があるため、丸める
  return Math.round((today.getTime() - day.getTime()) / (24 * 60 * 60 * 1000));
}
