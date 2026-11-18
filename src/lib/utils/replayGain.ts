/**
 * リプレイゲイン（音量の正規化）で、曲にかかる補正の量
 *
 * 補正は再生エンジンがかける（`src-tauri/src/playback/normalization.rs`の`normalization_gain`）。
 * ここは、サウンドのパネルに「再生中の曲にかけている補正」を出すために、同じ計算をする。
 * エンジンの計算を変えたら、ここも合わせる。
 */

import type { ReplayGain, Settings } from '#lib/types/models.js';

/** 補正に使ったゲイン（`untagged`は、ゲインのタグがない曲） */
export type ReplayGainSource = 'track' | 'album' | 'untagged';

/** 曲にかかる補正 */
export interface AppliedReplayGain {
  source: ReplayGainSource;
  /** 音量の変化（dB。プリアンプ・クリップ防止を含む） */
  gainDb: number;
  /** クリップ防止で、ゲインより小さく抑えたか */
  limited: boolean;
}

/** 計算に使う設定 */
export type ReplayGainSettings = Pick<
  Settings,
  | 'volumeNormalization'
  | 'normalizationPreampDb'
  | 'normalizationUntaggedPreampDb'
  | 'normalizationPreventClipping'
>;

/**
 * 曲にかかる補正を求める（正規化がオフの時はnull）
 *
 * @param shuffle シャッフルモードか（自動の時に、トラック単位とアルバム単位を選ぶ）
 */
export function appliedReplayGain(
  replayGain: ReplayGain,
  settings: ReplayGainSettings,
  shuffle: boolean
): AppliedReplayGain | null {
  const mode = settings.volumeNormalization;
  if (mode === 'off') return null;
  const useTrack = mode === 'track' || (mode === 'auto' && shuffle);
  const track = {
    source: 'track' as const,
    gain: replayGain.trackGain,
    peak: replayGain.trackPeak
  };
  const album = {
    source: 'album' as const,
    gain: replayGain.albumGain,
    peak: replayGain.albumPeak
  };
  const [preferred, fallback] = useTrack ? [track, album] : [album, track];
  const chosen = preferred.gain !== null ? preferred : fallback.gain !== null ? fallback : null;
  if (chosen === null || chosen.gain === null) {
    return untaggedGain(settings, preferred.peak ?? fallback.peak);
  }

  const gainDb = chosen.gain + settings.normalizationPreampDb;
  if (settings.normalizationPreventClipping && chosen.peak !== null && chosen.peak > 0) {
    // ピークがフルスケール（1.0）を超えない倍率（1/peak）までに抑える
    const limitDb = -20 * Math.log10(chosen.peak);
    if (gainDb > limitDb) return { source: chosen.source, gainDb: limitDb, limited: true };
  }
  return { source: chosen.source, gainDb, limited: false };
}

/**
 * ゲインのない曲の補正（ゲインのない曲の設定をかける）
 *
 * クリップ防止がオンで、ピークがある場合は、持ち上げてもピークがフルスケールを超えない量に抑える。
 * 抑えるのは持ち上げた分だけで、ピークがもともと最大を超えている曲も、下げはしない。
 */
function untaggedGain(settings: ReplayGainSettings, peak: number | null): AppliedReplayGain {
  const gainDb = settings.normalizationUntaggedPreampDb;
  if (settings.normalizationPreventClipping && peak !== null && peak > 0) {
    const limitDb = Math.max(-20 * Math.log10(peak), 0);
    if (gainDb > limitDb) return { source: 'untagged', gainDb: limitDb, limited: true };
  }
  return { source: 'untagged', gainDb, limited: false };
}
