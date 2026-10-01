import { describe, expect, it } from 'vitest';
import {
  combineValidationResults,
  toSafeInteger,
  toSafeString,
  validateFieldLength,
  validatePlaylistName,
  validateTrackNumber,
  validateYear
} from './validation';

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
  });

  it('ファイル名に使えない文字を含むと無効', () => {
    for (const ch of ['<', '>', ':', '"', '/', '\\', '|', '?', '*']) {
      expect(validatePlaylistName(`a${ch}b`).valid).toBe(false);
    }
  });
});

describe('validateYear / validateTrackNumber', () => {
  it('未入力は有効', () => {
    expect(validateYear(null).valid).toBe(true);
    expect(validateTrackNumber(undefined).valid).toBe(true);
  });

  it('範囲の境界を判定する', () => {
    expect(validateYear(1000).valid).toBe(true);
    expect(validateYear(9999).valid).toBe(true);
    expect(validateYear(999).valid).toBe(false);
    expect(validateTrackNumber(1).valid).toBe(true);
    expect(validateTrackNumber(999).valid).toBe(true);
    expect(validateTrackNumber(0).valid).toBe(false);
  });

  it('NaNは無効', () => {
    expect(validateYear(NaN).valid).toBe(false);
    expect(validateTrackNumber(NaN).valid).toBe(false);
  });
});

describe('validateFieldLength', () => {
  it('上限を超えると現在の文字数を含むエラーを返す', () => {
    const result = validateFieldLength('abcd', 'タイトル', 3);
    expect(result.valid).toBe(false);
    expect(result.error).toContain('タイトルは3文字以内');
    expect(result.error).toContain('現在: 4文字');
  });
});

describe('combineValidationResults', () => {
  it('無効な結果のエラーだけを集める', () => {
    expect(
      combineValidationResults([
        { valid: true },
        { valid: false, error: 'A' },
        { valid: false, error: 'B' }
      ])
    ).toEqual({ valid: false, errors: ['A', 'B'] });
  });
});

describe('toSafeString / toSafeInteger', () => {
  it('null・undefinedは空文字にし、長さを切り詰める', () => {
    expect(toSafeString(null)).toBe('');
    expect(toSafeString(undefined)).toBe('');
    expect(toSafeString('abcdef', 3)).toBe('abc');
    expect(toSafeString(42)).toBe('42');
  });

  it('整数に変換できない値は既定値を返す', () => {
    expect(toSafeInteger('12')).toBe(12);
    expect(toSafeInteger('12.9')).toBe(12);
    expect(toSafeInteger('abc', 7)).toBe(7);
    expect(toSafeInteger(undefined)).toBe(0);
  });
});
