import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ConvertEntry, ConvertStatus } from '#lib/types/models.js';
import {
  DEFAULT_CONVERT_OPTIONS,
  isLossyFormat,
  loadConvertOptions,
  saveConvertOptions,
  summarizeConvertPlan
} from './convert.js';

const entry = (
  trackId: string,
  status: ConvertStatus,
  sourceLossless: boolean | null
): ConvertEntry => ({
  trackId,
  sourceName: `${trackId}.mp3`,
  outputPath: status === 'missing' ? null : `out/${trackId}.m4a`,
  status,
  sourceLossless
});

const ALL = ['aac', 'alac', 'flac', 'wav', 'aiff'] as const;

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubStorage(initial: Record<string, string> = {}) {
  const items = { ...initial };
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => items[key] ?? null,
    setItem: (key: string, value: string) => {
      items[key] = value;
    }
  });
  return items;
}

describe('isLossyFormat', () => {
  it('AACだけが、非可逆の形式', () => {
    expect(ALL.filter(isLossyFormat)).toEqual(['aac']);
  });
});

describe('summarizeConvertPlan', () => {
  const entries = [
    entry('lossy1', 'convert', false),
    entry('lossy2', 'convert', false),
    entry('lossless', 'convert', true),
    entry('unknown', 'convert', null),
    entry('missing', 'missing', false),
    entry('exists', 'targetExists', true),
    entry('same', 'sameTarget', false)
  ];

  it('変換する曲・変換しない曲に分ける（順は変えない）', () => {
    const summary = summarizeConvertPlan(entries, 'flac', 'export');

    expect(summary.conversions.map((e) => e.trackId)).toEqual([
      'lossy1',
      'lossy2',
      'lossless',
      'unknown'
    ]);
    expect(summary.blocked.map((e) => e.trackId)).toEqual(['missing', 'exists', 'same']);
  });

  it('音質が戻らない変換を、変換する曲だけから数える', () => {
    // 可逆の形式へ: 非可逆の曲は、音質が戻らない
    expect(summarizeConvertPlan(entries, 'flac', 'export').warnings).toEqual({
      lossyToLossless: 2,
      lossyToLossy: 0,
      losslessReplaced: 0
    });
    // 非可逆の形式へ書き出す: 非可逆の曲は、音質が下がる
    expect(summarizeConvertPlan(entries, 'aac', 'export').warnings).toEqual({
      lossyToLossless: 0,
      lossyToLossy: 2,
      losslessReplaced: 0
    });
    // 非可逆の形式に置き換える: 可逆の曲は、元の音質に戻せない
    expect(summarizeConvertPlan(entries, 'aac', 'replace').warnings).toEqual({
      lossyToLossless: 0,
      lossyToLossy: 2,
      losslessReplaced: 1
    });
    // 可逆の形式に置き換える場合は、可逆の曲の注意はない
    expect(summarizeConvertPlan(entries, 'alac', 'replace').warnings.losslessReplaced).toBe(0);
  });

  it('空の計画は、すべて空にする', () => {
    expect(summarizeConvertPlan([], 'aac', 'export')).toEqual({
      conversions: [],
      blocked: [],
      warnings: { lossyToLossless: 0, lossyToLossy: 0, losslessReplaced: 0 }
    });
  });
});

describe('前回の指定', () => {
  it('保存した指定を読む（出力先は、いつも書き出しから始める）', () => {
    stubStorage();
    expect(loadConvertOptions(ALL)).toEqual(DEFAULT_CONVERT_OPTIONS);

    saveConvertOptions({
      format: 'flac',
      aacBitrateKbps: 320,
      mode: 'replace',
      template: '%artist% - %title%',
      addToLibrary: true
    });

    expect(loadConvertOptions(ALL)).toEqual({
      format: 'flac',
      aacBitrateKbps: 320,
      mode: 'export',
      template: '%artist% - %title%',
      addToLibrary: true
    });
  });

  it('覚えていた形式が使えなければ、使える形式にする', () => {
    stubStorage({ 'muspice:convertOptions': JSON.stringify({ format: 'flac' }) });

    expect(loadConvertOptions(['aac', 'alac']).format).toBe('aac');
    expect(loadConvertOptions(['alac', 'wav']).format).toBe('alac');
    expect(loadConvertOptions([]).format).toBe(DEFAULT_CONVERT_OPTIONS.format);
  });

  it('壊れた値・保存先を使えない場合は、既定の指定で動く', () => {
    stubStorage({ 'muspice:convertOptions': '{broken' });
    expect(loadConvertOptions(ALL)).toEqual(DEFAULT_CONVERT_OPTIONS);

    stubStorage({
      'muspice:convertOptions': JSON.stringify({ aacBitrateKbps: '256', template: '  ' })
    });
    expect(loadConvertOptions(ALL)).toEqual(DEFAULT_CONVERT_OPTIONS);

    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('使えません');
      },
      setItem: () => {
        throw new Error('使えません');
      }
    });
    expect(() => saveConvertOptions(DEFAULT_CONVERT_OPTIONS)).not.toThrow();
    expect(loadConvertOptions(ALL)).toEqual(DEFAULT_CONVERT_OPTIONS);
  });
});
