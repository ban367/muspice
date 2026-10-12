//! ラウドネスの測定（ITU-R BS.1770 / EBU R128）と、ReplayGainの値の計算
//!
//! ReplayGainのタグがない曲の音量をそろえるため、曲をデコードしてラウドネスを求める（ADR-053）。
//!
//! - 測り方は、ITU-R BS.1770の「番組のラウドネス」（聴感に合わせたフィルター（K特性）をかけ、
//!   400msのブロックごとの大きさを、静かな部分を除いて（ゲート）平均する）
//! - 測るのは、再生エンジンが出す音（ステレオにそろえた後）。モノラルは左右に同じ音を出すため、
//!   2チャンネル分として数え、3チャンネル以上は、再生と同じまとめ方でステレオにしてから測る
//! - ゲインは、ReplayGain 2.0の基準（-18 LUFS）との差。ピークは、サンプルの最大の振幅
//!   （サンプルの間の山（トゥルーピーク）は求めない）
//!
//! アルバムのラウドネスは、アルバムの全曲のブロックをまとめて、同じ方法で平均する。

use crate::error::{AppError, AppResult};
use crate::playback::{OUTPUT_CHANNELS, TrackDecoder, append_stereo};
use std::f64::consts::PI;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// ReplayGain 2.0の基準のラウドネス（LUFS）
pub const REFERENCE_LUFS: f64 = -18.0;

/// これより静かなブロックは、平均に入れない（LUFS。絶対ゲート）
const ABSOLUTE_GATE_LUFS: f64 = -70.0;

/// 絶対ゲートを通ったブロックの平均より、これ以上静かなブロックは、平均に入れない（相対ゲート。-10 LU）
const RELATIVE_GATE_RATIO: f64 = 0.1;

/// ブロックをずらす幅（秒）。ブロックの長さ（400ms）の4分の1（75%ずつ重ねる）
const HOP_SECONDS: f64 = 0.1;

/// 1つのブロックに入る、ずらす幅の数
const HOPS_PER_BLOCK: usize = 4;

/// 大きさ（チャンネルの二乗平均の和）をLUFSにする時に足す値（BS.1770。K特性の1kHzでの増幅を打ち消す）
const LUFS_OFFSET: f64 = -0.691;

/// 2次のフィルター（転置した直接形II）
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    state: [f64; 2],
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 2]) -> Self {
        Self {
            b,
            a,
            state: [0.0; 2],
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        let output = self.b[0] * input + self.state[0];
        self.state[0] = self.b[1] * input - self.a[0] * output + self.state[1];
        self.state[1] = self.b[2] * input - self.a[1] * output;
        output
    }
}

/// K特性のフィルター（高い音を持ち上げる棚形のフィルターと、低い音を落とすフィルターの2段）
///
/// BS.1770の係数は48kHz用のため、ほかのサンプルレートでは、同じ特性になるように求め直す
/// （元になるアナログのフィルターの値は、libebur128と同じ）。
fn k_weighting(sample_rate: u32) -> [Biquad; 2] {
    let rate = f64::from(sample_rate);

    let k = (PI * 1681.974450955533 / rate).tan();
    let q = 0.7071752369554196;
    let high = 10f64.powf(3.999843853973347 / 20.0);
    let band = high.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad::new(
        [
            (high + band * k / q + k * k) / a0,
            2.0 * (k * k - high) / a0,
            (high - band * k / q + k * k) / a0,
        ],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );

    let k = (PI * 38.13547087602444 / rate).tan();
    let q = 0.5003270373238773;
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad::new(
        [1.0, -2.0, 1.0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );

    [shelf, high_pass]
}

/// 大きさ（チャンネルの二乗平均の和）をLUFSにする
fn to_lufs(energy: f64) -> f64 {
    LUFS_OFFSET + 10.0 * energy.log10()
}

/// 1曲を測った結果
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Measurement {
    /// 400msのブロックごとの大きさ（チャンネルの二乗平均の和。100msずつずらしたもの）
    pub blocks: Vec<f64>,
    /// サンプルの最大の振幅（1.0がフルスケール）
    pub peak: f64,
}

impl Measurement {
    /// 曲のラウドネス（LUFS。短すぎる・無音の曲はNone）
    pub fn loudness(&self) -> Option<f64> {
        integrated_loudness([self])
    }
}

/// ステレオの音声のラウドネスを測る
pub struct LoudnessMeter {
    filters: [[Biquad; 2]; OUTPUT_CHANNELS],
    /// ずらす幅（100ms）のフレーム数
    hop_frames: usize,
    /// 今のずらす幅の中で、数えたフレーム数と、フィルターを通した音の二乗の和（2チャンネルの合計）
    hop_filled: usize,
    hop_sum: f64,
    /// ずらす幅ごとの、二乗の和
    hops: Vec<f64>,
    peak: f32,
}

impl LoudnessMeter {
    pub fn new(sample_rate: u32) -> Self {
        let filters = k_weighting(sample_rate);
        Self {
            filters: [filters; OUTPUT_CHANNELS],
            hop_frames: ((f64::from(sample_rate) * HOP_SECONDS).round() as usize).max(1),
            hop_filled: 0,
            hop_sum: 0.0,
            hops: Vec::new(),
            peak: 0.0,
        }
    }

    /// 音声（ステレオのインターリーブ）を足す
    pub fn process(&mut self, samples: &[f32]) {
        for frame in samples.as_chunks::<OUTPUT_CHANNELS>().0 {
            for (sample, filters) in frame.iter().zip(&mut self.filters) {
                self.peak = self.peak.max(sample.abs());
                let shelved = filters[0].process(f64::from(*sample));
                let filtered = filters[1].process(shelved);
                self.hop_sum += filtered * filtered;
            }
            self.hop_filled += 1;
            if self.hop_filled == self.hop_frames {
                self.hops.push(self.hop_sum);
                self.hop_filled = 0;
                self.hop_sum = 0.0;
            }
        }
    }

    /// 測った結果を返す（ブロックに満たない終わりの部分は、使わない）
    pub fn finish(self) -> Measurement {
        let block_frames = (self.hop_frames * HOPS_PER_BLOCK) as f64;
        Measurement {
            blocks: self
                .hops
                .windows(HOPS_PER_BLOCK)
                .map(|hops| hops.iter().sum::<f64>() / block_frames)
                .collect(),
            peak: f64::from(self.peak),
        }
    }
}

/// 曲（1曲か、アルバムの全曲）のラウドネス（LUFS）を求める
///
/// 絶対ゲート（-70 LUFS）を通ったブロックの平均を求め、その平均より10 LU以上静かなブロックを
/// 除いて、もう一度平均する。絶対ゲートを通るブロックがなければ（短すぎる・無音）、None。
pub fn integrated_loudness<'a>(
    measurements: impl IntoIterator<Item = &'a Measurement>,
) -> Option<f64> {
    let audible: Vec<f64> = measurements
        .into_iter()
        .flat_map(|measurement| &measurement.blocks)
        .copied()
        .filter(|energy| to_lufs(*energy) > ABSOLUTE_GATE_LUFS)
        .collect();
    if audible.is_empty() {
        return None;
    }
    let threshold = audible.iter().sum::<f64>() / audible.len() as f64 * RELATIVE_GATE_RATIO;
    let (sum, count) = audible
        .iter()
        .filter(|energy| **energy > threshold)
        .fold((0.0, 0usize), |(sum, count), energy| {
            (sum + energy, count + 1)
        });
    (count > 0).then(|| to_lufs(sum / count as f64))
}

/// ラウドネスから、ReplayGainのゲイン（dB）を求める
///
/// 測れなかった曲（短すぎる・無音）は、補正しない（0 dB）。
pub fn replay_gain_db(loudness: Option<f64>) -> f64 {
    loudness.map_or(0.0, |loudness| REFERENCE_LUFS - loudness)
}

/// 音楽ファイルをデコードして、ラウドネスを測る
///
/// @param cancelled - trueになったら、途中でやめる（`Ok(None)`を返す）
pub fn measure_file(path: &Path, cancelled: &AtomicBool) -> AppResult<Option<Measurement>> {
    let mut decoder = TrackDecoder::open(path)?;
    let channels = decoder.channels();
    if channels == 0 || decoder.sample_rate() == 0 {
        return Err(AppError::Playback(
            "音声の形式が正しくありません".to_string(),
        ));
    }
    let mut meter = LoudnessMeter::new(decoder.sample_rate());
    let mut stereo: Vec<f32> = Vec::new();

    while let Some(chunk) = decoder.next_chunk()? {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        stereo.clear();
        append_stereo(chunk, channels, &mut stereo);
        meter.process(&stereo);
    }
    Ok(Some(meter.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// 正弦波（ステレオ。左右は同じ）を足す
    fn feed_sine(meter: &mut LoudnessMeter, rate: u32, frequency: f64, dbfs: f64, seconds: f64) {
        let amplitude = 10f64.powf(dbfs / 20.0);
        let frames = (f64::from(rate) * seconds).round() as usize;
        let samples: Vec<f32> = (0..frames)
            .flat_map(|frame| {
                let value = (amplitude
                    * (2.0 * PI * frequency * frame as f64 / f64::from(rate)).sin())
                    as f32;
                [value, value]
            })
            .collect();
        meter.process(&samples);
    }

    /// 大きさを変えながら続く1kHzの正弦波（EBU Tech 3341の確認用の信号）のラウドネス
    fn loudness_of(rate: u32, parts: &[(f64, f64)]) -> Option<f64> {
        let mut meter = LoudnessMeter::new(rate);
        for (dbfs, seconds) in parts {
            feed_sine(&mut meter, rate, 1000.0, *dbfs, *seconds);
        }
        meter.finish().loudness()
    }

    fn assert_close(actual: Option<f64>, expected: f64, tolerance: f64) {
        let actual = actual.expect("ラウドネスを測れるはず");
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} は {expected} ± {tolerance} のはず"
        );
    }

    #[test]
    fn test_k_weighting_matches_the_standard_coefficients_at_48khz() {
        let [shelf, high_pass] = k_weighting(48_000);
        let close = |actual: f64, expected: f64| (actual - expected).abs() < 1e-9;

        // ITU-R BS.1770の表の値
        assert!(close(shelf.b[0], 1.53512485958697));
        assert!(close(shelf.b[1], -2.69169618940638));
        assert!(close(shelf.b[2], 1.19839281085285));
        assert!(close(shelf.a[0], -1.69065929318241));
        assert!(close(shelf.a[1], 0.73248077421585));
        assert!(close(high_pass.a[0], -1.99004745483398));
        assert!(close(high_pass.a[1], 0.99007225036621));
    }

    #[test]
    fn test_stereo_sine_measures_its_level() {
        // EBU Tech 3341の確認1・2: -23 dBFS・-33 dBFSの1kHzの正弦波（ステレオ）は、-23・-33 LUFS
        assert_close(loudness_of(RATE, &[(-23.0, 20.0)]), -23.0, 0.1);
        assert_close(loudness_of(RATE, &[(-33.0, 20.0)]), -33.0, 0.1);
    }

    #[test]
    fn test_other_sample_rates_measure_the_same() {
        for rate in [8_000, 22_050, 44_100, 96_000, 192_000] {
            assert_close(loudness_of(rate, &[(-23.0, 10.0)]), -23.0, 0.1);
        }
    }

    #[test]
    fn test_quiet_parts_are_gated_out() {
        // 確認3: 静かな部分（相対ゲートより下）は、平均に入れない
        assert_close(
            loudness_of(RATE, &[(-36.0, 10.0), (-23.0, 60.0), (-36.0, 10.0)]),
            -23.0,
            0.1,
        );
        // 確認4: 絶対ゲートより下の部分も入れない
        assert_close(
            loudness_of(
                RATE,
                &[
                    (-72.0, 10.0),
                    (-36.0, 10.0),
                    (-23.0, 60.0),
                    (-36.0, 10.0),
                    (-72.0, 10.0),
                ],
            ),
            -23.0,
            0.1,
        );
        // 確認5: 相対ゲートを通る部分は、平均に入る
        assert_close(
            loudness_of(RATE, &[(-26.0, 20.0), (-20.0, 20.1), (-26.0, 20.0)]),
            -23.0,
            0.1,
        );
    }

    #[test]
    fn test_one_channel_is_3_lu_quieter_than_both() {
        // BS.1770: 0 dBFS・997Hzの正弦波を1チャンネルだけに入れると、-3.01 LKFS
        let mut meter = LoudnessMeter::new(RATE);
        let samples: Vec<f32> = (0..RATE * 10)
            .flat_map(|frame| {
                let value = (2.0 * PI * 997.0 * f64::from(frame) / f64::from(RATE)).sin() as f32;
                [value, 0.0]
            })
            .collect();
        meter.process(&samples);
        let measurement = meter.finish();

        assert_close(measurement.loudness(), -3.01, 0.05);
        assert!((measurement.peak - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_low_frequencies_count_less() {
        // K特性は、低い音を小さく数える（同じ振幅でも、ラウドネスは低い）
        let mut low = LoudnessMeter::new(RATE);
        feed_sine(&mut low, RATE, 40.0, -23.0, 10.0);
        let low = low.finish().loudness().unwrap();

        assert!(low < -27.0, "{low}");
    }

    #[test]
    fn test_silence_and_short_audio_have_no_loudness() {
        let mut silence = LoudnessMeter::new(RATE);
        silence.process(&vec![0.0; RATE as usize * 2 * 5]);
        let silence = silence.finish();
        assert_eq!(silence.loudness(), None);
        assert_eq!(silence.peak, 0.0);
        assert_eq!(replay_gain_db(silence.loudness()), 0.0);

        // 400msに満たない音声は、ブロックにならない
        let mut short = LoudnessMeter::new(RATE);
        feed_sine(&mut short, RATE, 1000.0, -10.0, 0.39);
        let short = short.finish();
        assert!(short.blocks.is_empty());
        assert_eq!(short.loudness(), None);
        assert!(short.peak > 0.3);
    }

    #[test]
    fn test_chunk_boundaries_do_not_change_the_result() {
        let mut whole = LoudnessMeter::new(44_100);
        feed_sine(&mut whole, 44_100, 1000.0, -20.0, 3.0);
        let whole = whole.finish();

        // 同じ音声を、細かく分けて渡す
        let amplitude = 10f64.powf(-20.0 / 20.0);
        let samples: Vec<f32> = (0..44_100 * 3)
            .flat_map(|frame| {
                let value =
                    (amplitude * (2.0 * PI * 1000.0 * f64::from(frame) / 44_100.0).sin()) as f32;
                [value, value]
            })
            .collect();
        let mut pieces = LoudnessMeter::new(44_100);
        for chunk in samples.chunks(2 * 777) {
            pieces.process(chunk);
        }

        assert_eq!(pieces.finish(), whole);
    }

    #[test]
    fn test_album_loudness_gates_over_all_tracks() {
        let measure = |dbfs: f64, seconds: f64| {
            let mut meter = LoudnessMeter::new(RATE);
            feed_sine(&mut meter, RATE, 1000.0, dbfs, seconds);
            meter.finish()
        };
        let loud = measure(-20.0, 20.1);
        let quiet = measure(-26.0, 40.0);
        let very_quiet = measure(-50.0, 30.0);

        // 曲ごとのラウドネス
        assert_close(loud.loudness(), -20.0, 0.1);
        assert_close(quiet.loudness(), -26.0, 0.1);
        // アルバム: 全曲のブロックをまとめて平均する（とても静かな曲は、相対ゲートで除かれる）
        assert_close(
            integrated_loudness([&loud, &quiet, &very_quiet]),
            -23.0,
            0.1,
        );
        assert_eq!(integrated_loudness([]), None);
    }

    #[test]
    fn test_replay_gain_is_the_distance_to_the_reference() {
        assert_eq!(replay_gain_db(Some(-12.0)), -6.0);
        assert_eq!(replay_gain_db(Some(-23.0)), 5.0);
        assert_eq!(replay_gain_db(None), 0.0);
    }

    /// 実際のファイルのラウドネスを出す（手動の確認用。ffmpegの`ebur128`フィルターの値と比べる）
    ///
    /// ```text
    /// LOUDNESS_TEST_FILES=a.flac:b.mp3 cargo test print_file_loudness -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "実際のファイルを使う手動の確認"]
    fn print_file_loudness() {
        let files = std::env::var("LOUDNESS_TEST_FILES").expect("LOUDNESS_TEST_FILESを指定する");
        let never = AtomicBool::new(false);
        let mut all = Vec::new();
        for file in files.split(':') {
            let measurement = measure_file(Path::new(file), &never).unwrap().unwrap();
            println!(
                "{file}: I={:?} LUFS, gain={:+.2} dB, peak={:.6}",
                measurement.loudness(),
                replay_gain_db(measurement.loudness()),
                measurement.peak
            );
            all.push(measurement);
        }
        println!("album: I={:?} LUFS", integrated_loudness(&all));
    }

    #[test]
    fn test_measure_file_reads_the_audio_as_the_engine_plays_it() {
        use crate::playback::test_audio::{temp_dir, write_wav_samples};

        let dir = temp_dir("loudness");
        let rate = 44_100u32;
        let amplitude = 10f64.powf(-23.0 / 20.0) * f64::from(i16::MAX);
        let sine: Vec<i16> = (0..rate * 5)
            .map(|frame| {
                (amplitude * (2.0 * PI * 1000.0 * f64::from(frame) / f64::from(rate)).sin()) as i16
            })
            .collect();
        // モノラルは、左右に同じ音を出すものとして測る（ステレオの同じ音と同じラウドネス）
        let mono = dir.join("mono.wav");
        write_wav_samples(&mono, rate, 1, &sine);
        let stereo = dir.join("stereo.wav");
        let both: Vec<i16> = sine.iter().flat_map(|sample| [*sample, *sample]).collect();
        write_wav_samples(&stereo, rate, 2, &both);

        let never = AtomicBool::new(false);
        let mono = measure_file(&mono, &never).unwrap().unwrap();
        let stereo = measure_file(&stereo, &never).unwrap().unwrap();
        assert_close(mono.loudness(), -23.0, 0.1);
        assert_close(stereo.loudness(), -23.0, 0.1);
        assert!((mono.peak - 10f64.powf(-23.0 / 20.0)).abs() < 1e-3);

        // 中断
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            measure_file(&dir.join("stereo.wav"), &cancelled).unwrap(),
            None
        );
        // 開けないファイル
        assert!(measure_file(&dir.join("missing.wav"), &never).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
