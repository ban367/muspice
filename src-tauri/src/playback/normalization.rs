//! 音量の正規化（ReplayGain）の補正量の計算
//!
//! タグから読んだゲインとピーク（`ReplayGain`）と、設定（トラック単位 / アルバム単位 / 自動）から、
//! 曲にかける倍率を決める（考え方は`docs/design/decisions.md`のADR-012・ADR-100）。

use crate::models::ReplayGain;
use crate::settings::{Settings, VolumeNormalization};

/// 音量の正規化の設定（再生エンジンが使う形）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalization {
    /// 補正に使うゲインの単位（`None`は正規化しない）
    pub mode: Option<VolumeNormalization>,
    /// プリアンプ（dB）。補正する曲（ゲインのある曲）の音量を、まとめて上げ下げする
    pub preamp_db: f64,
    /// ゲインのない曲にかける音量の変化（dB）
    pub untagged_preamp_db: f64,
    /// ピークがフルスケールを超えない倍率に抑える（クリップ防止）
    pub prevent_clipping: bool,
}

impl Normalization {
    /// 正規化しない
    #[cfg(test)]
    pub const OFF: Self = Self {
        mode: None,
        preamp_db: 0.0,
        untagged_preamp_db: 0.0,
        prevent_clipping: true,
    };
}

impl From<VolumeNormalization> for Normalization {
    /// その単位で正規化する、プリアンプなし・クリップ防止ありの設定
    fn from(mode: VolumeNormalization) -> Self {
        Self {
            mode: Some(mode),
            preamp_db: 0.0,
            untagged_preamp_db: 0.0,
            prevent_clipping: true,
        }
    }
}

impl From<&Settings> for Normalization {
    fn from(settings: &Settings) -> Self {
        Self {
            mode: settings
                .volume_normalization_enabled
                .then_some(settings.volume_normalization),
            preamp_db: f64::from(settings.normalization_preamp_db),
            untagged_preamp_db: f64::from(settings.normalization_untagged_preamp_db),
            prevent_clipping: settings.normalization_prevent_clipping,
        }
    }
}

/// トラックの音量を補正する倍率を求める
///
/// - トラック単位ではトラックのゲイン、アルバム単位ではアルバムのゲインを使い、
///   ない場合はもう一方を使う（ゲインとピークは同じ単位の組で使う）。自動では、シャッフルモードの間
///   （`shuffle`）はトラック単位、それ以外はアルバム単位にする
/// - ゲインに、プリアンプ（dB）を足す
/// - クリップ防止がオンで、ピークがある場合は、補正後にピークがフルスケール（1.0）を超えない
///   倍率に抑える（音割れを防ぐ。オフでも、出力のリミッターは効く）
/// - ゲインのタグがない曲は、ゲインのない曲の設定（dB）だけをかける（0なら補正しない）。クリップ防止が
///   オンで、ピークがある場合は、持ち上げてもピークがフルスケールを超えない倍率に抑える（下げはしない）
/// - 正規化がオフの場合は補正しない（1。プリアンプもかけない）
///
/// サウンドのパネルの表示（フロントエンドの`#lib/utils/replayGain`の`appliedReplayGain`）も、同じ計算をする。
pub fn normalization_gain(
    replay_gain: &ReplayGain,
    normalization: Normalization,
    shuffle: bool,
) -> f32 {
    let track = (replay_gain.track_gain, replay_gain.track_peak);
    let album = (replay_gain.album_gain, replay_gain.album_peak);
    let Some(mode) = normalization.mode else {
        return 1.0;
    };
    let (preferred, fallback) = match mode {
        VolumeNormalization::Track => (track, album),
        VolumeNormalization::Album => (album, track),
        VolumeNormalization::Auto if shuffle => (track, album),
        VolumeNormalization::Auto => (album, track),
    };
    let (gain_db, peak) = match (preferred, fallback) {
        ((Some(gain), peak), _) | ((None, _), (Some(gain), peak)) => (gain, peak),
        ((None, preferred_peak), (None, fallback_peak)) => {
            return finite_or_one(untagged_gain(
                normalization,
                preferred_peak.or(fallback_peak),
            ));
        }
    };

    let gain = from_db(gain_db + normalization.preamp_db);
    let limited = match peak {
        Some(peak) if normalization.prevent_clipping && peak > 0.0 => gain.min(1.0 / peak),
        _ => gain,
    };
    finite_or_one(limited)
}

/// ゲインのない曲の倍率（ゲインのない曲の設定（dB）をかける）
///
/// クリップ防止がオンで、ピークがある場合は、持ち上げてもピークがフルスケールを超えない倍率に抑える。
/// 抑えるのは持ち上げた分だけで、ピークがもともと最大を超えている曲も、下げはしない（設定が0なら補正しない）。
fn untagged_gain(normalization: Normalization, peak: Option<f64>) -> f64 {
    let gain = from_db(normalization.untagged_preamp_db);
    match peak {
        Some(peak) if normalization.prevent_clipping && peak > 0.0 => {
            gain.min((1.0 / peak).max(1.0))
        }
        _ => gain,
    }
}

/// dBを倍率にする
fn from_db(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// 倍率が有限の値でなければ、補正しない（1）
fn finite_or_one(factor: f64) -> f32 {
    if factor.is_finite() {
        factor as f32
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

    fn gain(replay_gain: &ReplayGain, normalization: Normalization) -> f32 {
        normalization_gain(replay_gain, normalization, false)
    }

    #[test]
    fn test_preamp_shifts_the_gain_of_tagged_tracks_only() {
        let with_preamp = |mode, preamp_db| Normalization {
            preamp_db,
            ..Normalization::from(mode)
        };
        let no_peak = ReplayGain {
            track_gain: Some(-6.0),
            ..ReplayGain::default()
        };

        // ゲインにプリアンプを足す（-6 + 4 = -2 dB）
        assert!(close(
            gain(&no_peak, with_preamp(VolumeNormalization::Track, 4.0)),
            from_db(-2.0)
        ));
        assert!(close(
            gain(&no_peak, with_preamp(VolumeNormalization::Track, -4.0)),
            from_db(-10.0)
        ));
        // 上げても、ピークがフルスケールを超えない倍率に抑える（ピーク0.5 → 2倍まで）
        let quiet = ReplayGain {
            track_gain: Some(0.0),
            track_peak: Some(0.5),
            ..ReplayGain::default()
        };
        assert!(close(
            gain(&quiet, with_preamp(VolumeNormalization::Track, 12.0)),
            2.0
        ));
        assert!(close(
            gain(&quiet, with_preamp(VolumeNormalization::Track, 3.0)),
            from_db(3.0)
        ));
        // ゲインのない曲・正規化がオフの場合は、プリアンプもかけない
        assert_eq!(
            gain(
                &ReplayGain::default(),
                with_preamp(VolumeNormalization::Track, 6.0)
            ),
            1.0
        );
        let off = Normalization {
            preamp_db: 6.0,
            ..Normalization::OFF
        };
        assert_eq!(gain(&no_peak, off), 1.0);
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
        assert_eq!(gain(&tagged(), Normalization::OFF), 1.0);
        // シャッフル・ゲインのない曲の設定も効かない
        let off = Normalization {
            untagged_preamp_db: -6.0,
            ..Normalization::OFF
        };
        assert_eq!(normalization_gain(&tagged(), off, true), 1.0);
        assert_eq!(normalization_gain(&ReplayGain::default(), off, true), 1.0);
    }

    #[test]
    fn test_uses_the_gain_of_the_selected_unit() {
        assert!(close(
            gain(&tagged(), VolumeNormalization::Track.into()),
            from_db(-6.0)
        ));
        assert!(close(
            gain(&tagged(), VolumeNormalization::Album.into()),
            from_db(-9.0)
        ));
    }

    #[test]
    fn test_auto_uses_track_gain_only_while_shuffled() {
        let auto = Normalization::from(VolumeNormalization::Auto);
        assert!(close(
            normalization_gain(&tagged(), auto, false),
            from_db(-9.0)
        ));
        assert!(close(
            normalization_gain(&tagged(), auto, true),
            from_db(-6.0)
        ));
        // トラック単位・アルバム単位は、シャッフルによらない
        assert!(close(
            normalization_gain(&tagged(), VolumeNormalization::Album.into(), true),
            from_db(-9.0)
        ));
        // 使う単位のゲインがなければ、もう一方を使う
        let track_only = ReplayGain {
            track_gain: Some(-4.0),
            ..ReplayGain::default()
        };
        assert!(close(
            normalization_gain(&track_only, auto, false),
            from_db(-4.0)
        ));
    }

    #[test]
    fn test_falls_back_to_the_other_unit() {
        let album_only = ReplayGain {
            album_gain: Some(-3.0),
            ..ReplayGain::default()
        };
        assert!(close(
            gain(&album_only, VolumeNormalization::Track.into()),
            from_db(-3.0)
        ));
        let track_only = ReplayGain {
            track_gain: Some(-4.0),
            ..ReplayGain::default()
        };
        assert!(close(
            gain(&track_only, VolumeNormalization::Album.into()),
            from_db(-4.0)
        ));
    }

    #[test]
    fn test_tracks_without_gain_are_not_changed() {
        let peak_only = ReplayGain {
            track_peak: Some(0.9),
            ..ReplayGain::default()
        };
        assert_eq!(gain(&peak_only, VolumeNormalization::Track.into()), 1.0);
    }

    #[test]
    fn test_untagged_preamp_applies_to_tracks_without_gain_only() {
        let normalization = Normalization {
            preamp_db: 3.0,
            untagged_preamp_db: -6.0,
            ..Normalization::from(VolumeNormalization::Album)
        };
        // ゲインのない曲には、ゲインのない曲の設定だけをかける（プリアンプは足さない）
        let peak_only = ReplayGain {
            track_peak: Some(0.9),
            ..ReplayGain::default()
        };
        assert!(close(gain(&peak_only, normalization), from_db(-6.0)));
        // ゲインのある曲には、かけない（-9 + 3 = -6 dB）
        assert!(close(gain(&tagged(), normalization), from_db(-6.0)));
        let louder = Normalization {
            untagged_preamp_db: 12.0,
            ..normalization
        };
        assert!(close(gain(&tagged(), louder), from_db(-6.0)));
    }

    #[test]
    fn test_untagged_boost_is_limited_by_the_peak() {
        let boost = |untagged_preamp_db, prevent_clipping| Normalization {
            untagged_preamp_db,
            prevent_clipping,
            ..Normalization::from(VolumeNormalization::Track)
        };
        // ピーク0.5の曲を+12dB（約4倍）上げると最大を超えるため、2倍に抑える
        let half = ReplayGain {
            album_peak: Some(0.5),
            ..ReplayGain::default()
        };
        assert!(close(gain(&half, boost(12.0, true)), 2.0));
        // 超えない量なら、そのまま上げる
        assert!(close(gain(&half, boost(3.0, true)), from_db(3.0)));
        // クリップ防止がオフなら、抑えない
        assert!(close(gain(&half, boost(12.0, false)), from_db(12.0)));
        // 抑えるのは持ち上げた分だけ: 最大を超えている曲も、下げない（0dBなら補正しない）
        let loud = ReplayGain {
            track_peak: Some(1.2),
            ..ReplayGain::default()
        };
        assert_eq!(gain(&loud, boost(0.0, true)), 1.0);
        assert_eq!(gain(&loud, boost(6.0, true)), 1.0);
        assert!(close(gain(&loud, boost(-6.0, true)), from_db(-6.0)));
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
            gain(&loud, VolumeNormalization::Track.into()),
            1.0 / 0.8
        ));
        // ピークがない場合はゲインのまま
        let no_peak = ReplayGain {
            track_gain: Some(6.0),
            ..ReplayGain::default()
        };
        assert!(close(
            gain(&no_peak, VolumeNormalization::Track.into()),
            from_db(6.0)
        ));
    }

    #[test]
    fn test_clipping_is_allowed_when_prevention_is_off() {
        let loud = ReplayGain {
            track_gain: Some(6.0),
            track_peak: Some(0.8),
            ..ReplayGain::default()
        };
        let allow = Normalization {
            prevent_clipping: false,
            ..Normalization::from(VolumeNormalization::Track)
        };
        // ピークで抑えず、ゲインのまま上げる（出力のリミッターが抑える）
        assert!(close(gain(&loud, allow), from_db(6.0)));
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
            gain(&replay_gain, VolumeNormalization::Album.into()),
            from_db(3.0)
        ));
    }
}
