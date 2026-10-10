import { describe, expect, it } from 'vitest';
import type { Track } from '#lib/types/models.js';
import {
  DEFAULT_TRACK_COLUMNS,
  TRACK_COLUMNS,
  TRACK_COLUMN_IDS,
  moveColumn,
  normalizeColumns,
  toggleColumn,
  trackCellText,
  type TrackColumnId
} from './trackColumns.js';

const track: Track = {
  id: 'a',
  filePath: '/music/a.flac',
  fileName: 'a.flac',
  title: 'タイトル',
  artist: 'アーティスト',
  album: 'アルバム',
  albumArtist: null,
  genre: 'Jazz',
  year: 2021,
  trackNumber: 3,
  discNumber: null,
  duration: 185,
  fileSize: 3 * 1024 * 1024,
  format: 'flac',
  bitrate: 1411,
  sampleRate: 44100,
  isFavorite: true,
  rating: 4,
  playCount: 12,
  skipCount: 0,
  lastPlayedAt: null,
  createdAt: '2026-03-05T10:00:00.000Z',
  updatedAt: '2026-03-05T10:00:00.000Z',
  replayGain: { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null },
  sortTags: { title: null, artist: null, album: null, albumArtist: null },
  isMissing: false
};

describe('列の定義', () => {
  it('すべての列に定義があり、既定の幅は下限以上', () => {
    for (const id of TRACK_COLUMN_IDS) {
      expect(TRACK_COLUMNS[id].width).toBeGreaterThanOrEqual(TRACK_COLUMNS[id].minWidth);
    }
    expect(DEFAULT_TRACK_COLUMNS.every((id) => TRACK_COLUMN_IDS.includes(id))).toBe(true);
  });
});

describe('normalizeColumns', () => {
  const fallback: TrackColumnId[] = ['title', 'duration'];

  it('今ある列だけを、保存していた順に残す', () => {
    expect(normalizeColumns(['duration', 'title', 'album'], fallback)).toEqual([
      'duration',
      'title',
      'album'
    ]);
  });

  it('知らない列・重複・文字列でない値を除く', () => {
    expect(normalizeColumns(['title', 'status', 'title', 3, null, 'year'], fallback)).toEqual([
      'title',
      'year'
    ]);
  });

  it('配列でない・1つも残らない場合は、既定の列にする', () => {
    expect(normalizeColumns(undefined, fallback)).toEqual(fallback);
    expect(normalizeColumns('title', fallback)).toEqual(fallback);
    expect(normalizeColumns(['status'], fallback)).toEqual(fallback);
    // 既定の配列そのものは返さない（書き換えても既定が変わらない）
    expect(normalizeColumns([], fallback)).not.toBe(fallback);
  });
});

describe('toggleColumn', () => {
  const columns: TrackColumnId[] = ['title', 'artist', 'rating', 'duration'];

  it('表示中の列は、非表示にする', () => {
    expect(toggleColumn(columns, 'rating')).toEqual(['title', 'artist', 'duration']);
  });

  it('表示する列は、基準の並びで次に来る表示中の列の前に入れる', () => {
    // アルバムは、アーティストの次（評価の前）
    expect(toggleColumn(columns, 'album')).toEqual([
      'title',
      'artist',
      'album',
      'rating',
      'duration'
    ]);
    // サイズは、基準の並びの最後のため末尾
    expect(toggleColumn(columns, 'fileSize')).toEqual([...columns, 'fileSize']);
  });

  it('列を入れ替えた後でも、次に来る列の前に入れる', () => {
    const reordered: TrackColumnId[] = ['duration', 'title', 'artist'];
    // 評価の次に来る表示中の列は「時間」
    expect(toggleColumn(reordered, 'rating')).toEqual(['rating', 'duration', 'title', 'artist']);
  });

  it('最後の1列は、非表示にしない', () => {
    expect(toggleColumn(['title'], 'title')).toEqual(['title']);
  });
});

describe('moveColumn', () => {
  const columns: TrackColumnId[] = ['title', 'artist', 'album', 'duration'];

  it('右へ動かすと、相手の列の後ろに入る', () => {
    expect(moveColumn(columns, 'title', 'album')).toEqual(['artist', 'album', 'title', 'duration']);
  });

  it('左へ動かすと、相手の列の前に入る', () => {
    expect(moveColumn(columns, 'duration', 'artist')).toEqual([
      'title',
      'duration',
      'artist',
      'album'
    ]);
  });

  it('同じ列・表示していない列では、何も変えない', () => {
    expect(moveColumn(columns, 'title', 'title')).toEqual(columns);
    expect(moveColumn(columns, 'year', 'title')).toEqual(columns);
    expect(moveColumn(columns, 'title', 'year')).toEqual(columns);
  });
});

describe('trackCellText', () => {
  it('列ごとの文字列を返す', () => {
    expect(trackCellText(track, 'title')).toBe('タイトル');
    expect(trackCellText(track, 'album')).toBe('アルバム');
    expect(trackCellText(track, 'year')).toBe('2021');
    expect(trackCellText(track, 'trackNumber')).toBe('3');
    expect(trackCellText(track, 'playCount')).toBe('12');
    expect(trackCellText(track, 'skipCount')).toBe('0');
    expect(trackCellText(track, 'duration')).toBe('3:05');
    expect(trackCellText(track, 'format')).toBe('FLAC');
    expect(trackCellText(track, 'bitrate')).toBe('1411 kbps');
    expect(trackCellText(track, 'sampleRate')).toBe('44.1 kHz');
    expect(trackCellText(track, 'fileSize')).toBe('3.0 MB');
    expect(trackCellText(track, 'createdAt')).toContain('2026');
  });

  it('値のない項目は、空の文字列にする（タイトルは、ファイル名）', () => {
    expect(trackCellText(track, 'albumArtist')).toBe('');
    expect(trackCellText(track, 'discNumber')).toBe('');
    expect(trackCellText(track, 'lastPlayedAt')).toBe('');
    expect(trackCellText({ ...track, title: null }, 'title')).toBe('a.flac');
    expect(trackCellText({ ...track, bitrate: null, sampleRate: null }, 'bitrate')).toBe('');
    // ハート・星は、部品で出すため文字はない
    expect(trackCellText(track, 'favorite')).toBe('');
    expect(trackCellText(track, 'rating')).toBe('');
  });
});
