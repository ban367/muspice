/**
 * 音量の正規化（ReplayGain）の補正量の計算
 */
import type { ReplayGain, VolumeNormalization } from '#lib/types/models.js';

/**
 * トラックの音量を補正する倍率（Web AudioのGainNodeに設定する値）を求める
 *
 * - トラック単位ではトラックのゲイン、アルバム単位ではアルバムのゲインを使い、
 *   ない場合はもう一方を使う（ゲインとピークは同じ単位の組で使う）
 * - ピークがある場合は、補正後にピークがフルスケール（1.0）を超えない倍率に抑える（音割れを防ぐ）
 * - ゲインのタグがない曲・正規化がオフの場合は補正しない（1）
 */
export function normalizationGain(replayGain: ReplayGain, mode: VolumeNormalization): number {
  if (mode === 'off') return 1;

  const track = { gain: replayGain.trackGain, peak: replayGain.trackPeak };
  const album = { gain: replayGain.albumGain, peak: replayGain.albumPeak };
  const [preferred, fallback] = mode === 'album' ? [album, track] : [track, album];
  const source = preferred.gain !== null ? preferred : fallback;
  if (source.gain === null) return 1;

  const gain = 10 ** (source.gain / 20);
  return source.peak !== null && source.peak > 0 ? Math.min(gain, 1 / source.peak) : gain;
}
