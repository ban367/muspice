import { describe, expect, it } from 'vitest';
import type { PlayHistoryEntry, Track } from '#lib/types/models.js';
import { buildPlayHistoryRows, daysAgo, distinctTracks } from './playHistory.js';

function track(id: string): Track {
  return { id, title: id } as Track;
}

/** 表示している環境のタイムゾーンでの日時から、履歴の1件を作る */
function entry(id: number, trackId: string, ...time: [number, number, number, number, number]) {
  return { id, trackId, playedAt: new Date(...time).toISOString() } satisfies PlayHistoryEntry;
}

const tracks = [track('t1'), track('t2'), track('t3')];

describe('buildPlayHistoryRows', () => {
  it('日付ごとに見出しを入れ、同じ曲も再生した回数だけ並べる', () => {
    const rows = buildPlayHistoryRows(
      [
        entry(5, 't1', 2026, 9, 9, 21, 30),
        entry(4, 't2', 2026, 9, 9, 8, 0),
        entry(3, 't1', 2026, 9, 9, 0, 0),
        entry(2, 't1', 2026, 9, 8, 23, 59),
        entry(1, 't3', 2026, 8, 30, 12, 0)
      ],
      tracks
    );

    expect(
      rows.map((row) =>
        row.kind === 'date'
          ? `${row.date.getMonth() + 1}/${row.date.getDate()} (${row.count})`
          : row.track.id
      )
    ).toEqual(['10/9 (3)', 't1', 't2', 't1', '10/8 (1)', 't1', '9/30 (1)', 't3']);
    // 行のIDは、同じ曲でも違う
    expect(new Set(rows.map((row) => row.id)).size).toBe(rows.length);
    expect(rows[1]).toMatchObject({ kind: 'play', id: 'play:5' });
    expect((rows[1] as { playedAt: Date }).playedAt.getHours()).toBe(21);
  });

  it('ライブラリにない曲と、日時を読めない履歴は除く（見出しも作らない）', () => {
    const rows = buildPlayHistoryRows(
      [
        entry(3, 'gone', 2026, 9, 9, 10, 0),
        { id: 2, trackId: 't1', playedAt: 'not a date' },
        entry(1, 't2', 2026, 9, 8, 10, 0)
      ],
      tracks
    );

    expect(rows.map((row) => row.kind)).toEqual(['date', 'play']);
    expect(rows[0]).toMatchObject({ count: 1 });
  });

  it('履歴がなければ、空になる', () => {
    expect(buildPlayHistoryRows([], tracks)).toEqual([]);
  });
});

describe('distinctTracks', () => {
  it('出てくる順に、重複なく並べる', () => {
    const rows = buildPlayHistoryRows(
      [
        entry(4, 't2', 2026, 9, 9, 12, 0),
        entry(3, 't1', 2026, 9, 9, 11, 0),
        entry(2, 't2', 2026, 9, 8, 10, 0),
        entry(1, 't3', 2026, 9, 7, 10, 0)
      ],
      tracks
    );

    expect(distinctTracks(rows).map((t) => t.id)).toEqual(['t2', 't1', 't3']);
  });
});

describe('daysAgo', () => {
  const now = new Date(2026, 9, 9, 0, 5);

  it('日付の違いを数える（時刻は見ない）', () => {
    expect(daysAgo(new Date(2026, 9, 9, 23, 59), now)).toBe(0);
    expect(daysAgo(new Date(2026, 9, 8, 23, 59), now)).toBe(1);
    expect(daysAgo(new Date(2026, 9, 2), now)).toBe(7);
    expect(daysAgo(new Date(2025, 9, 9), now)).toBe(365);
  });
});
