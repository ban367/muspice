//! デコードした音声を、出力の形式へ変換する
//!
//! エンジンの中では、音声をステレオ・出力デバイスのサンプルレートのf32（インターリーブ）で扱う。
//! ここで、曲ごとに違うチャンネル数とサンプルレートを、その形式にそろえる。
//!
//! - チャンネル: モノラルは左右に同じ音を入れ、3チャンネル以上はステレオへまとめる
//! - サンプルレート: 出力と違う場合は`rubato`のFFTのリサンプラー（比が一定の変換向け）で変換する
//!
//! 続けて再生する曲の形式が同じなら、変換器を使い回す。リサンプラーの中に残っている前の曲の
//! 終わりが次の曲の頭とつながるため、サンプルレートを変換していても曲間に切れ目が入らない。

use crate::error::{AppError, AppResult};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

/// エンジンの中で扱うチャンネル数（ステレオ）
pub const OUTPUT_CHANNELS: usize = 2;

/// リサンプラーに1度に渡すフレーム数の目安（実際の数は、サンプルレートの比に合わせて`rubato`が決める）
const RESAMPLER_CHUNK_FRAMES: usize = 1024;

/// ステレオへまとめるときに、中央・後ろのチャンネルを左右へ足す割合（-3dB）
const DOWNMIX_LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// 1つの形式（サンプルレート・チャンネル数）の音声を、出力の形式へ変換する
pub struct Converter {
    source_rate: u32,
    source_channels: usize,
    output_rate: u32,
    resampling: Option<Resampling>,
}

/// サンプルレートの変換の状態
struct Resampling {
    resampler: Fft<f32>,
    source_rate: u32,
    output_rate: u32,
    /// ステレオにそろえた、リサンプラーへ渡す前のサンプル
    input: Vec<f32>,
    /// リサンプラーの出力を受けるバッファ
    output: Vec<f32>,
    /// これまでに受け取ったフレーム数・出力したフレーム数（出し切るときの長さを決める）
    frames_in: u64,
    frames_out: u64,
    /// リサンプラーの立ち上がりの遅延のうち、まだ捨てていないフレーム数
    delay_left: usize,
}

impl Converter {
    pub fn new(source_rate: u32, source_channels: usize, output_rate: u32) -> AppResult<Self> {
        if source_rate == 0 || source_channels == 0 || output_rate == 0 {
            return Err(AppError::Playback(
                "音声の形式が正しくありません".to_string(),
            ));
        }
        let resampling = if source_rate == output_rate {
            None
        } else {
            Some(Resampling::new(source_rate, output_rate)?)
        };
        Ok(Self {
            source_rate,
            source_channels,
            output_rate,
            resampling,
        })
    }

    /// この変換器で、指定した形式の音声を続けて変換できるか
    pub fn matches(&self, source_rate: u32, source_channels: usize, output_rate: u32) -> bool {
        self.source_rate == source_rate
            && self.source_channels == source_channels
            && self.output_rate == output_rate
    }

    /// サンプル（元のチャンネル数のインターリーブ）を変換し、`out`の後ろへ足す
    pub fn process(&mut self, samples: &[f32], out: &mut Vec<f32>) -> AppResult<()> {
        match &mut self.resampling {
            None => {
                append_stereo(samples, self.source_channels, out);
                Ok(())
            }
            Some(resampling) => {
                append_stereo(samples, self.source_channels, &mut resampling.input);
                resampling.frames_in += (samples.len() / self.source_channels) as u64;
                resampling.process_full_chunks(out)
            }
        }
    }

    /// 変換器の中に残っている音声を出し切り、`out`の後ろへ足す（続きのない曲の終わりで呼ぶ）
    ///
    /// 呼んだ後は、新しい音声を頭から変換できる状態に戻る。
    pub fn flush(&mut self, out: &mut Vec<f32>) -> AppResult<()> {
        match &mut self.resampling {
            None => Ok(()),
            Some(resampling) => resampling.flush(out),
        }
    }

    /// 変換器の中に残っている音声を捨てる（シークなどで、続きではない音声を渡す前に呼ぶ）
    pub fn reset(&mut self) {
        if let Some(resampling) = &mut self.resampling {
            resampling.reset();
        }
    }
}

impl Resampling {
    fn new(source_rate: u32, output_rate: u32) -> AppResult<Self> {
        let resampler = Fft::<f32>::new(
            source_rate as usize,
            output_rate as usize,
            RESAMPLER_CHUNK_FRAMES,
            OUTPUT_CHANNELS,
            FixedSync::Input,
        )
        .map_err(|e| AppError::Playback(format!("サンプルレートの変換を準備できません: {}", e)))?;
        let delay_left = resampler.output_delay();
        let output = vec![0.0; resampler.output_frames_max() * OUTPUT_CHANNELS];
        Ok(Self {
            resampler,
            source_rate,
            output_rate,
            input: Vec::new(),
            output,
            frames_in: 0,
            frames_out: 0,
            delay_left,
        })
    }

    /// たまっている入力のうち、リサンプラーに渡せる分（1度に渡す数の倍数）を変換する
    fn process_full_chunks(&mut self, out: &mut Vec<f32>) -> AppResult<()> {
        let chunk_samples = self.resampler.input_frames_next() * OUTPUT_CHANNELS;
        let mut consumed = 0;
        while self.input.len() - consumed >= chunk_samples {
            self.run(consumed, None, None, out)?;
            consumed += chunk_samples;
        }
        self.input.drain(..consumed);
        Ok(())
    }

    fn flush(&mut self, out: &mut Vec<f32>) -> AppResult<()> {
        self.process_full_chunks(out)?;
        // 入力の長さに対応する出力の長さ。ここまで出したら終わり
        let expected = (u128::from(self.frames_in) * u128::from(self.output_rate))
            .div_ceil(u128::from(self.source_rate)) as u64;

        // 1度に渡す数に満たない残りは、無音を足して変換する
        let remaining = self.input.len() / OUTPUT_CHANNELS;
        if remaining > 0 {
            self.run(0, Some(remaining), Some(expected), out)?;
        }
        // リサンプラーの中に残っている分（遅延の分）を、無音を流して押し出す
        while self.frames_out < expected {
            let before = self.frames_out;
            self.run(0, Some(0), Some(expected), out)?;
            if self.frames_out == before && self.delay_left == 0 {
                break;
            }
        }
        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.resampler.reset();
        self.input.clear();
        self.frames_in = 0;
        self.frames_out = 0;
        self.delay_left = self.resampler.output_delay();
    }

    /// リサンプラーを1回動かし、出力を`out`の後ろへ足す
    ///
    /// - `offset`: `input`の中の、読み始める位置（サンプル）
    /// - `partial`: 入力が1度に渡す数に満たない場合の、有効なフレーム数（残りは無音として扱われる）
    /// - `limit`: 出力の合計がこのフレーム数を超えないように切る
    fn run(
        &mut self,
        offset: usize,
        partial: Option<usize>,
        limit: Option<u64>,
        out: &mut Vec<f32>,
    ) -> AppResult<()> {
        let to_error = |e: &dyn std::fmt::Display| {
            AppError::Playback(format!("サンプルレートの変換に失敗しました: {}", e))
        };

        let input_frames = partial.unwrap_or_else(|| self.resampler.input_frames_next());
        let input = InterleavedSlice::new(
            &self.input[offset..offset + input_frames * OUTPUT_CHANNELS],
            OUTPUT_CHANNELS,
            input_frames,
        )
        .map_err(|e| to_error(&e))?;
        let output_frames = self.output.len() / OUTPUT_CHANNELS;
        let mut output =
            InterleavedSlice::new_mut(&mut self.output, OUTPUT_CHANNELS, output_frames)
                .map_err(|e| to_error(&e))?;
        let indexing = partial.map(|frames| Indexing::new().partial_len(frames));
        let (_, produced) = self
            .resampler
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(|e| to_error(&e))?;
        if partial.is_some() {
            self.input.clear();
        }

        // 立ち上がりの遅延の分は捨てる（捨てないと、その分だけ音が遅れ、曲の終わりが切れる）
        let skipped = self.delay_left.min(produced);
        self.delay_left -= skipped;
        let mut frames = produced - skipped;
        if let Some(limit) = limit {
            frames = frames.min(limit.saturating_sub(self.frames_out) as usize);
        }
        self.frames_out += frames as u64;
        out.extend_from_slice(
            &self.output[skipped * OUTPUT_CHANNELS..(skipped + frames) * OUTPUT_CHANNELS],
        );
        Ok(())
    }
}

/// サンプル（`channels`チャンネルのインターリーブ）をステレオにして、`out`の後ろへ足す
fn append_stereo(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        1 => out.extend(samples.iter().flat_map(|&sample| [sample, sample])),
        2 => out.extend_from_slice(samples),
        // 5.1ch以上（左・右・中央・低音・後ろ左・後ろ右…）: 中央と後ろを左右へ足す。
        // 低音（LFE）は、ステレオへまとめるときの一般的な扱いにならって足さない。
        // 足した結果が大きくなりすぎないよう、割合の合計で割る
        channels if channels >= 6 => {
            let scale = 1.0 / (1.0 + 2.0 * DOWNMIX_LEVEL);
            out.extend(samples.chunks_exact(channels).flat_map(|frame| {
                let center = frame[2] * DOWNMIX_LEVEL;
                [
                    (frame[0] + center + frame[4] * DOWNMIX_LEVEL) * scale,
                    (frame[1] + center + frame[5] * DOWNMIX_LEVEL) * scale,
                ]
            }));
        }
        // 3〜5チャンネル: 並びが形式によって違うため、左右だけを使う
        channels => out.extend(
            samples
                .chunks_exact(channels)
                .flat_map(|frame| [frame[0], frame[1]]),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 指定した周波数の正弦波（ステレオ。左右は同じ）
    fn sine(rate: u32, frequency: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value =
                    (2.0 * std::f64::consts::PI * frequency * i as f64 / f64::from(rate)).sin();
                [value as f32, value as f32]
            })
            .collect()
    }

    #[test]
    fn test_same_rate_passes_samples_through() {
        let mut converter = Converter::new(44_100, 2, 44_100).unwrap();
        let input = sine(44_100, 440.0, 1000);
        let mut out = Vec::new();
        converter.process(&input, &mut out).unwrap();
        converter.flush(&mut out).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn test_mono_is_copied_to_both_channels() {
        let mut converter = Converter::new(48_000, 1, 48_000).unwrap();
        let mut out = Vec::new();
        converter.process(&[0.25, -0.5], &mut out).unwrap();
        assert_eq!(out, vec![0.25, 0.25, -0.5, -0.5]);
    }

    #[test]
    fn test_surround_is_mixed_down_without_clipping() {
        let mut converter = Converter::new(48_000, 6, 48_000).unwrap();
        let mut out = Vec::new();
        // すべてのチャンネルが最大の音量でも、1を超えない
        converter.process(&[1.0; 6], &mut out).unwrap();
        assert_eq!(out.len(), 2);
        assert!((out[0] - 1.0).abs() < 1e-6 && (out[1] - 1.0).abs() < 1e-6);

        // 左だけの音は、左だけに出る
        out.clear();
        converter
            .process(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0], &mut out)
            .unwrap();
        assert!(out[0] > 0.0 && out[1] == 0.0);
    }

    #[test]
    fn test_resampled_length_matches_the_rate_ratio() {
        for (source_rate, output_rate) in [(44_100, 48_000), (96_000, 44_100), (48_000, 96_000)] {
            let frames = source_rate as usize; // 1秒
            let mut converter = Converter::new(source_rate, 2, output_rate).unwrap();
            let input = sine(source_rate, 440.0, frames);
            let mut out = Vec::new();
            // 中途半端な大きさに分けて渡しても、結果の長さは変わらない
            for chunk in input.chunks(1234 * 2) {
                converter.process(chunk, &mut out).unwrap();
            }
            converter.flush(&mut out).unwrap();
            assert_eq!(
                out.len() / 2,
                output_rate as usize,
                "{source_rate} → {output_rate}"
            );
        }
    }

    #[test]
    fn test_resampled_signal_is_not_delayed() {
        // 440Hzの正弦波を変換した結果が、出力のレートで作った正弦波と（頭から）一致する
        let mut converter = Converter::new(44_100, 2, 48_000).unwrap();
        let mut out = Vec::new();
        converter
            .process(&sine(44_100, 440.0, 44_100), &mut out)
            .unwrap();
        converter.flush(&mut out).unwrap();

        let expected = sine(48_000, 440.0, 48_000);
        // 頭と終わりは、信号が急に始まる・終わる影響が出るため、その内側を比べる
        for frame in (2_000..46_000).step_by(97) {
            let (actual, wanted) = (out[frame * 2], expected[frame * 2]);
            assert!(
                (actual - wanted).abs() < 0.01,
                "frame {frame}: {actual} vs {wanted}"
            );
        }
    }

    #[test]
    fn test_flush_and_reset_allow_reuse() {
        let mut converter = Converter::new(44_100, 2, 48_000).unwrap();
        let input = sine(44_100, 440.0, 4_410);
        let mut first = Vec::new();
        converter.process(&input, &mut first).unwrap();
        converter.flush(&mut first).unwrap();

        // 出し切った後は、頭から変換し直せる（結果も同じ）
        let mut second = Vec::new();
        converter.process(&input, &mut second).unwrap();
        converter.flush(&mut second).unwrap();
        assert_eq!(first.len() / 2, 4_800);
        assert_eq!(first, second);

        // 途中で捨てても同じ
        let mut third = Vec::new();
        converter.process(&input[..2_000], &mut third).unwrap();
        converter.reset();
        third.clear();
        converter.process(&input, &mut third).unwrap();
        converter.flush(&mut third).unwrap();
        assert_eq!(first, third);
    }

    #[test]
    fn test_continuous_tracks_join_without_a_gap() {
        // 1つの正弦波を2つの曲に分けて続けて変換しても、分けずに変換した結果と同じになる
        let input = sine(44_100, 440.0, 20_000);
        let mut whole = Vec::new();
        let mut converter = Converter::new(44_100, 2, 48_000).unwrap();
        converter.process(&input, &mut whole).unwrap();
        converter.flush(&mut whole).unwrap();

        let mut joined = Vec::new();
        let (first, second) = input.split_at(7_777 * 2);
        converter.process(first, &mut joined).unwrap();
        converter.process(second, &mut joined).unwrap();
        converter.flush(&mut joined).unwrap();
        assert_eq!(whole, joined);
    }

    #[test]
    fn test_invalid_format_is_rejected() {
        assert!(Converter::new(0, 2, 48_000).is_err());
        assert!(Converter::new(44_100, 0, 48_000).is_err());
        assert!(Converter::new(44_100, 2, 0).is_err());
    }
}
