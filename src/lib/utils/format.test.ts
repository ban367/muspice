import { describe, expect, it } from 'vitest';
import { formatDateTime, formatDuration, formatFileSize, formatTotalDuration } from './format';

describe('formatDuration', () => {
  it('秒数を m:ss 形式にする', () => {
    expect(formatDuration(5)).toBe('0:05');
    expect(formatDuration(245.7)).toBe('4:05');
    expect(formatDuration(3600)).toBe('60:00');
  });

  it('不明な長さ（null・0）は --:-- にする', () => {
    expect(formatDuration(null)).toBe('--:--');
    expect(formatDuration(0)).toBe('--:--');
  });
});

describe('formatTotalDuration', () => {
  it('1時間未満は分だけを表示する', () => {
    expect(formatTotalDuration(0)).toBe('0分');
    expect(formatTotalDuration(59 * 60 + 59)).toBe('59分');
  });

  it('1時間以上は時間と分を表示する', () => {
    expect(formatTotalDuration(3600)).toBe('1時間0分');
    expect(formatTotalDuration(2 * 3600 + 30 * 60)).toBe('2時間30分');
  });
});

describe('formatFileSize', () => {
  it('単位を切り替えて表示する', () => {
    expect(formatFileSize(null)).toBe('--');
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(1023)).toBe('1023 B');
    expect(formatFileSize(1536)).toBe('1.5 KB');
    expect(formatFileSize(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatFileSize(3 * 1024 * 1024 * 1024)).toBe('3.00 GB');
  });
});

describe('formatDateTime', () => {
  it('日時を年月日と時刻にする', () => {
    // タイムゾーンに依存しないよう、ローカル時刻で作った日時を使う
    const date = new Date(2026, 9, 3, 14, 5);
    expect(formatDateTime(date.toISOString())).toBe('2026/10/03 14:05');
  });

  it('空・不正な値は -- にする', () => {
    expect(formatDateTime(null)).toBe('--');
    expect(formatDateTime('not a date')).toBe('--');
  });
});
