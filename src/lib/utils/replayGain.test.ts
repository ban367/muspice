import { describe, expect, it } from 'vitest';
import type { ReplayGain } from '#lib/types/models.js';
import { appliedReplayGain, type ReplayGainSettings } from './replayGain';

const NO_GAIN: ReplayGain = { trackGain: null, trackPeak: null, albumGain: null, albumPeak: null };

const TAGGED: ReplayGain = { trackGain: -6, trackPeak: 0.5, albumGain: -9, albumPeak: 0.5 };

function settings(overrides: Partial<ReplayGainSettings> = {}): ReplayGainSettings {
  return {
    volumeNormalizationEnabled: true,
    volumeNormalization: 'track',
    normalizationPreampDb: 0,
    normalizationUntaggedPreampDb: 0,
    normalizationPreventClipping: true,
    ...overrides
  };
}

describe('appliedReplayGain', () => {
  it('正規化がオフの時は補正しない', () => {
    expect(
      appliedReplayGain(TAGGED, settings({ volumeNormalizationEnabled: false }), false)
    ).toBeNull();
  });

  it('選んだ単位のゲインにプリアンプを足す', () => {
    expect(appliedReplayGain(TAGGED, settings(), false)).toEqual({
      source: 'track',
      gainDb: -6,
      limited: false
    });
    expect(
      appliedReplayGain(
        TAGGED,
        settings({ volumeNormalization: 'album', normalizationPreampDb: 2 }),
        false
      )
    ).toEqual({ source: 'album', gainDb: -7, limited: false });
  });

  it('自動は、シャッフルモードの間だけトラック単位', () => {
    const auto = settings({ volumeNormalization: 'auto' });
    expect(appliedReplayGain(TAGGED, auto, false)?.source).toBe('album');
    expect(appliedReplayGain(TAGGED, auto, true)?.source).toBe('track');
    // トラック単位・アルバム単位は、シャッフルによらない
    expect(
      appliedReplayGain(TAGGED, settings({ volumeNormalization: 'album' }), true)?.source
    ).toBe('album');
  });

  it('使う単位のゲインがなければ、もう一方を使う', () => {
    const albumOnly: ReplayGain = { ...NO_GAIN, albumGain: -3 };
    expect(appliedReplayGain(albumOnly, settings(), false)).toEqual({
      source: 'album',
      gainDb: -3,
      limited: false
    });
  });

  it('ゲインのない曲は、ゲインのない曲の設定だけをかける', () => {
    expect(
      appliedReplayGain(
        { ...NO_GAIN, trackPeak: 0.9 },
        settings({ normalizationPreampDb: 3, normalizationUntaggedPreampDb: -6 }),
        false
      )
    ).toEqual({ source: 'untagged', gainDb: -6, limited: false });
  });

  it('ゲインのない曲も、持ち上げた分はピークがフルスケールを超えない量に抑える', () => {
    const half: ReplayGain = { ...NO_GAIN, albumPeak: 0.5 };
    const limited = appliedReplayGain(half, settings({ normalizationUntaggedPreampDb: 12 }), false);
    expect(limited?.source).toBe('untagged');
    expect(limited?.limited).toBe(true);
    expect(limited?.gainDb).toBeCloseTo(-20 * Math.log10(0.5));
    // クリップ防止がオフなら抑えない
    expect(
      appliedReplayGain(
        half,
        settings({ normalizationUntaggedPreampDb: 12, normalizationPreventClipping: false }),
        false
      )
    ).toEqual({ source: 'untagged', gainDb: 12, limited: false });
    // 最大を超えている曲も、下げはしない
    const loud: ReplayGain = { ...NO_GAIN, trackPeak: 1.2 };
    expect(appliedReplayGain(loud, settings({ normalizationUntaggedPreampDb: 6 }), false)).toEqual({
      source: 'untagged',
      gainDb: 0,
      limited: true
    });
    expect(appliedReplayGain(loud, settings(), false)).toEqual({
      source: 'untagged',
      gainDb: 0,
      limited: false
    });
  });

  it('クリップ防止がオンなら、ピークがフルスケールを超えない量に抑える', () => {
    const loud: ReplayGain = { ...NO_GAIN, trackGain: 6, trackPeak: 0.8 };
    const limited = appliedReplayGain(loud, settings(), false);
    expect(limited?.limited).toBe(true);
    expect(limited?.gainDb).toBeCloseTo(-20 * Math.log10(0.8));
    // 抑える必要がなければ、そのまま
    expect(appliedReplayGain(TAGGED, settings(), false)?.limited).toBe(false);
    // オフなら抑えない
    expect(
      appliedReplayGain(loud, settings({ normalizationPreventClipping: false }), false)
    ).toEqual({ source: 'track', gainDb: 6, limited: false });
  });
});
