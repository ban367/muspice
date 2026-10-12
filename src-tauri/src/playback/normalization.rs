//! 音量の正規化（ReplayGain）の補正量の計算
//!
//! タグから読んだゲインとピーク（`ReplayGain`）と、設定（トラック単位 / アルバム単位）から、
//! 曲にかける倍率を決める（考え方は`docs/design/decisions.md`のADR-012）。

use crate::models::ReplayGain;
use crate::settings::{Settings, VolumeNormalization};

/// 音量の正規化の設定（再生エンジンが使う形）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalization {
    pub mode: VolumeNormalization,
    /// プリアンプ（dB）。補正する曲（ゲインのある曲）の音量を、まとめて上げ下げする
    pub preamp_db: f64,
}

impl Normalization {
    /// 正規化しない
    #[cfg(test)]
    pub const OFF: Self = Self {
        mode: VolumeNormalization::Off,
        preamp_db: 0.0,
    };
}

impl From<VolumeNormalization> for Normalization {
    /// プリアンプなしの設定
    fn from(mode: VolumeNormalization) -> Self {
        Self {
            mode,
            preamp_db: 0.0,
        }
    }
}

impl From<&Settings> for Normalization {
    fn from(settings: &Settings) -> Self {
        Self {
            mode: settings.volume_normalization,
            preamp_db: f64::from(settings.normalization_preamp_db),
        }
    }
}

/// トラックの音量を補正する倍率を求める
///
/// - トラック単位ではトラックのゲイン、アルバム単位ではアルバムのゲインを使い、
///   ない場合はもう一方を使う（ゲインとピークは同じ単位の組で使う）
/// - ゲインに、プリアンプ（dB）を足す
/// - ピークがある場合は、補正後にピークがフルスケール（1.0）を超えない倍率に抑える（音割れを防ぐ）
/// - ゲインのタグがない曲・正規化がオフの場合は補正しない（1。プリアンプもかけない）
pub fn normalization_gain(replay_gain: &ReplayGain, normalization: Normalization) -> f32 {
    let track = (replay_gain.track_gain, replay_gain.track_peak);
    let album = (replay_gain.album_gain, replay_gain.album_peak);
    let (preferred, fallback) = match normalization.mode {
        VolumeNormalization::Off => return 1.0,
        VolumeNormalization::Track => (track, album),
        VolumeNormalization::Album => (album, track),
    };
    let (gain_db, peak) = match (preferred, fallback) {
        ((Some(gain), peak), _) | ((None, _), (Some(gain), peak)) => (gain, peak),
        _ => return 1.0,
    };

    let gain = 10f64.powf((gain_db + normalization.preamp_db) / 20.0);
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

    #[test]
    fn test_preamp_shifts_the_gain_of_tagged_tracks_only() {
        let with_preamp = |mode, preamp_db| Normalization { mode, preamp_db };
        let no_peak = ReplayGain {
            track_gain: Some(-6.0),
            ..ReplayGain::default()
        };

        // ゲインにプリアンプを足す（-6 + 4 = -2 dB）
        assert!(close(
            normalization_gain(&no_peak, with_preamp(VolumeNormalization::Track, 4.0)),
            from_db(-2.0)
        ));
        assert!(close(
            normalization_gain(&no_peak, with_preamp(VolumeNormalization::Track, -4.0)),
            from_db(-10.0)
        ));
        // 上げても、ピークがフルスケールを超えない倍率に抑える（ピーク0.5 → 2倍まで）
        let quiet = ReplayGain {
            track_gain: Some(0.0),
            track_peak: Some(0.5),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&quiet, with_preamp(VolumeNormalization::Track, 12.0)),
            2.0
        ));
        assert!(close(
            normalization_gain(&quiet, with_preamp(VolumeNormalization::Track, 3.0)),
            from_db(3.0)
        ));
        // ゲインのない曲・正規化がオフの場合は、プリアンプもかけない
        assert_eq!(
            normalization_gain(
                &ReplayGain::default(),
                with_preamp(VolumeNormalization::Track, 6.0)
            ),
            1.0
        );
        assert_eq!(
            normalization_gain(&no_peak, with_preamp(VolumeNormalization::Off, 6.0)),
            1.0
        );
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
        assert_eq!(
            normalization_gain(&tagged(), VolumeNormalization::Off.into()),
            1.0
        );
    }

    #[test]
    fn test_uses_the_gain_of_the_selected_unit() {
        assert!(close(
            normalization_gain(&tagged(), VolumeNormalization::Track.into()),
            from_db(-6.0)
        ));
        assert!(close(
            normalization_gain(&tagged(), VolumeNormalization::Album.into()),
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
            normalization_gain(&album_only, VolumeNormalization::Track.into()),
            from_db(-3.0)
        ));
        let track_only = ReplayGain {
            track_gain: Some(-4.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&track_only, VolumeNormalization::Album.into()),
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
            normalization_gain(&peak_only, VolumeNormalization::Track.into()),
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
            normalization_gain(&loud, VolumeNormalization::Track.into()),
            1.0 / 0.8
        ));
        // ピークがない場合はゲインのまま
        let no_peak = ReplayGain {
            track_gain: Some(6.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&no_peak, VolumeNormalization::Track.into()),
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
            normalization_gain(&replay_gain, VolumeNormalization::Album.into()),
            from_db(3.0)
        ));
    }
}
