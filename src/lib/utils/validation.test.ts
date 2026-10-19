import { describe, expect, it } from 'vitest';
import { toSafeString, validatePlaylistName } from './validation';

describe('validatePlaylistName', () => {
  it('通常の名前は有効', () => {
    expect(validatePlaylistName('お気に入り 2026')).toEqual({ valid: true });
  });

  it('空・空白のみは無効', () => {
    expect(validatePlaylistName('').valid).toBe(false);
    expect(validatePlaylistName('   ').valid).toBe(false);
  });

  it('100文字を超えると無効', () => {
    expect(validatePlaylistName('あ'.repeat(100)).valid).toBe(true);
    expect(validatePlaylistName('あ'.repeat(101)).valid).toBe(false);
    // 絵文字も1文字に数える
    expect(validatePlaylistName('🎵'.repeat(100)).valid).toBe(true);
  });

  it('ファイル名に使えない文字を含むと無効', () => {
    for (const ch of ['<', '>', ':', '"', '/', '\\', '|', '?', '*']) {
      expect(validatePlaylistName(`a${ch}b`).valid).toBe(false);
    }
  });
});

describe('toSafeString', () => {
  it('null・undefinedは空文字にし、長さを切り詰める', () => {
    expect(toSafeString(null)).toBe('');
    expect(toSafeString(undefined)).toBe('');
    expect(toSafeString('abcdef', 3)).toBe('abc');
    expect(toSafeString(42)).toBe('42');
  });
});
