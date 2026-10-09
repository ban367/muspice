//! 音量の正規化（ReplayGain）の補正量の計算
//!
//! audio要素での再生で使っているフロントエンドの`normalizationGain`
//! （`src/lib/utils/normalization.ts`）と同じ計算をする。

use crate::models::ReplayGain;
use crate::settings::VolumeNormalization;

/// トラックの音量を補正する倍率を求める
///
/// - トラック単位ではトラックのゲイン、アルバム単位ではアルバムのゲインを使い、
///   ない場合はもう一方を使う（ゲインとピークは同じ単位の組で使う）
/// - ピークがある場合は、補正後にピークがフルスケール（1.0）を超えない倍率に抑える（音割れを防ぐ）
/// - ゲインのタグがない曲・正規化がオフの場合は補正しない（1）
pub fn normalization_gain(replay_gain: &ReplayGain, mode: VolumeNormalization) -> f32 {
    let track = (replay_gain.track_gain, replay_gain.track_peak);
    let album = (replay_gain.album_gain, replay_gain.album_peak);
    let (preferred, fallback) = match mode {
        VolumeNormalization::Off => return 1.0,
        VolumeNormalization::Track => (track, album),
        VolumeNormalization::Album => (album, track),
    };
    let (gain_db, peak) = match (preferred, fallback) {
        ((Some(gain), peak), _) | ((None, _), (Some(gain), peak)) => (gain, peak),
        _ => return 1.0,
    };

    let gain = 10f64.powf(gain_db / 20.0);
    let limited = match peak {
        Some(peak) if peak > 0.0 => gain.min(1.0 / peak),
        _ => gain,
    };
    if limited.is_finite() {
        limited as f32
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_db(db: f64) -> f32 {
        10f64.powf(db / 20.0) as f32
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-5
    }

    fn tagged() -> ReplayGain {
        ReplayGain {
            track_gain: Some(-6.0),
            track_peak: Some(0.5),
            album_gain: Some(-9.0),
            album_peak: Some(0.5),
        }
    }

    #[test]
    fn test_off_does_not_change_the_volume() {
        assert_eq!(normalization_gain(&tagged(), VolumeNormalization::Off), 1.0);
    }

    #[test]
    fn test_uses_the_gain_of_the_selected_unit() {
        assert!(close(
            normalization_gain(&tagged(), VolumeNormalization::Track),
            from_db(-6.0)
        ));
        assert!(close(
            normalization_gain(&tagged(), VolumeNormalization::Album),
            from_db(-9.0)
        ));
    }

    #[test]
    fn test_falls_back_to_the_other_unit() {
        let album_only = ReplayGain {
            album_gain: Some(-3.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&album_only, VolumeNormalization::Track),
            from_db(-3.0)
        ));
        let track_only = ReplayGain {
            track_gain: Some(-4.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&track_only, VolumeNormalization::Album),
            from_db(-4.0)
        ));
    }

    #[test]
    fn test_tracks_without_gain_are_not_changed() {
        let peak_only = ReplayGain {
            track_peak: Some(0.9),
            ..ReplayGain::default()
        };
        assert_eq!(
            normalization_gain(&peak_only, VolumeNormalization::Track),
            1.0
        );
    }

    #[test]
    fn test_boost_is_limited_by_the_peak() {
        // +6 dB（約2倍）上げるとピーク0.8は1.6になるため、1/0.8倍に抑える
        let loud = ReplayGain {
            track_gain: Some(6.0),
            track_peak: Some(0.8),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&loud, VolumeNormalization::Track),
            1.0 / 0.8
        ));
        // ピークがない場合はゲインのまま
        let no_peak = ReplayGain {
            track_gain: Some(6.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&no_peak, VolumeNormalization::Track),
            from_db(6.0)
        ));
    }

    #[test]
    fn test_peak_of_the_same_unit_is_used() {
        // アルバムのゲインを使うときに、トラックのピークで抑えない
        let replay_gain = ReplayGain {
            track_peak: Some(0.9),
            album_gain: Some(3.0),
            album_peak: Some(0.5),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&replay_gain, VolumeNormalization::Album),
            from_db(3.0)
        ));
    }
}
