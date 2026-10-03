import { describe, expect, it } from 'vitest';
import type { ReplayGain } from '#lib/types/models.js';
import { normalizationGain } from './normalization';

function replayGain(overrides: Partial<ReplayGain> = {}): ReplayGain {
  return { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null, ...overrides };
}

/** dBを倍率にする */
const fromDb = (db: number) => 10 ** (db / 20);

describe('normalizationGain', () => {
  const tagged = replayGain({ trackGain: -6, trackPeak: 0.5, albumGain: -9, albumPeak: 0.5 });

  it('オフでは補正しない', () => {
    expect(normalizationGain(tagged, 'off')).toBe(1);
  });

  it('トラック単位・アルバム単位でそれぞれのゲインを使う', () => {
    expect(normalizationGain(tagged, 'track')).toBeCloseTo(fromDb(-6));
    expect(normalizationGain(tagged, 'album')).toBeCloseTo(fromDb(-9));
  });

  it('使う単位のゲインがない場合は、もう一方を使う', () => {
    expect(normalizationGain(replayGain({ albumGain: -3 }), 'track')).toBeCloseTo(fromDb(-3));
    expect(normalizationGain(replayGain({ trackGain: -4 }), 'album')).toBeCloseTo(fromDb(-4));
  });

  it('ゲインのタグがない曲は補正しない', () => {
    expect(normalizationGain(replayGain({ trackPeak: 0.9 }), 'track')).toBe(1);
  });

  it('補正後にピークが1.0を超えないよう、上げる量を抑える', () => {
    // +6 dB（約2倍）上げるとピーク0.8は1.6になるため、1/0.8倍に抑える
    expect(normalizationGain(replayGain({ trackGain: 6, trackPeak: 0.8 }), 'track')).toBeCloseTo(
      1 / 0.8
    );
    // ピークがない場合はゲインのまま
    expect(normalizationGain(replayGain({ trackGain: 6 }), 'track')).toBeCloseTo(fromDb(6));
  });

  it('ピークはゲインと同じ単位のものを使う', () => {
    // アルバムのゲインを使うときに、トラックのピークで抑えない
    const gain = normalizationGain(
      replayGain({ trackPeak: 0.9, albumGain: 3, albumPeak: 0.5 }),
      'album'
    );
    expect(gain).toBeCloseTo(fromDb(3));
  });
});
