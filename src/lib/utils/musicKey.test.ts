import { describe, expect, it } from 'vitest';
import { formatMusicKey } from './musicKey.js';

describe('formatMusicKey', () => {
  it('長調は主音の名前、短調はmを付ける', () => {
    expect(formatMusicKey(0)).toBe('C');
    expect(formatMusicKey(6)).toBe('F♯');
    expect(formatMusicKey(21)).toBe('Am');
    expect(formatMusicKey(15)).toBe('E♭m');
  });

  it('値のない曲・範囲の外の値は空', () => {
    expect(formatMusicKey(null)).toBe('');
    expect(formatMusicKey(24)).toBe('');
    expect(formatMusicKey(-1)).toBe('');
  });
});
