//! 出力のコールバックで音声を取り出す処理
//!
//! エンジンのスレッドがリングバッファへ書いた音声（ステレオ・出力のサンプルレート）を、
//! 出力デバイスのコールバックが取り出す。コールバックは、OSの音声のスレッドで決まった時間内に
//! 終える必要があるため、ロックの取得・メモリの確保・ファイルの読み取りをしない。
//! エンジンとのやり取りは、リングバッファとアトミックな値（`RenderShared`）だけで行う。
//!
//! - 音量と一時停止は、ここで反映する（バッファにたまっている分を待たずに、すぐ効く）。
//!   値が変わるときは短い時間をかけて滑らかに変え、ノイズ（プチッという音）を防ぐ
//! - シーク・曲の切り替えでは、エンジンが「この位置より前は捨てる」（`discard_until`）を書き、
//!   ここでバッファにたまっている古い音声を読み捨てる
//! - 取り出したフレーム数（`frames_read`）から、エンジンが再生位置と曲の切り替わりを求める

use super::convert::OUTPUT_CHANNELS;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// 音量・一時停止の切り替えを滑らかにする時間（秒）
const GAIN_RAMP_SECONDS: f32 = 0.015;

/// エンジンのスレッドと出力のコールバックで共有する値
pub struct RenderShared {
    /// リングバッファから取り出したフレーム数の合計（読み捨てた分を含む）
    frames_read: AtomicU64,
    /// この位置（フレーム数の合計）より前のフレームは、出力せずに読み捨てる
    discard_until: AtomicU64,
    /// 音量（0.0〜1.0のf32のビット列）
    volume: AtomicU32,
    /// 一時停止中か
    paused: AtomicBool,
    /// 再生中に、バッファの音声が足りなかったコールバックの回数（音切れ）
    underruns: AtomicU64,
}

impl RenderShared {
    pub fn new(volume: f32, paused: bool) -> Arc<Self> {
        Arc::new(Self {
            frames_read: AtomicU64::new(0),
            discard_until: AtomicU64::new(0),
            volume: AtomicU32::new(volume.to_bits()),
            paused: AtomicBool::new(paused),
            underruns: AtomicU64::new(0),
        })
    }

    /// 音切れの回数
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub fn frames_read(&self) -> u64 {
        self.frames_read.load(Ordering::Acquire)
    }

    /// この位置より前の、まだ出力していないフレームを捨てさせる
    pub fn discard_until(&self, frame: u64) {
        self.discard_until.store(frame, Ordering::Release);
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }
}

/// 出力のコールバックの中で、リングバッファから音声を取り出す
pub struct Renderer {
    consumer: rtrb::Consumer<f32>,
    shared: Arc<RenderShared>,
    /// 取り出したフレーム数の合計（`RenderShared::frames_read`の手元の値）
    frames_read: u64,
    /// 今かけている音量（目標の値へ少しずつ近づける）
    gain: f32,
    /// 1フレームあたりの音量の変化の上限
    gain_step: f32,
}

impl Renderer {
    pub fn new(consumer: rtrb::Consumer<f32>, shared: Arc<RenderShared>, sample_rate: u32) -> Self {
        Self {
            consumer,
            shared,
            frames_read: 0,
            gain: 0.0,
            gain_step: 1.0 / (GAIN_RAMP_SECONDS * sample_rate as f32),
        }
    }

    /// 取り出したフレーム数の合計
    #[cfg(test)]
    pub fn frames_read(&self) -> u64 {
        self.frames_read
    }

    /// 出力のバッファ（`channels`チャンネルのインターリーブ）を埋める
    ///
    /// 音声が足りない分・一時停止中は無音にする。
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        self.discard_stale_frames();

        let paused = self.shared.paused.load(Ordering::Relaxed);
        let target = if paused {
            0.0
        } else {
            f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
        };

        let frames = out.len().checked_div(channels).unwrap_or(0);
        // 一時停止して音量が0まで下がりきったら、バッファから取り出さない（再開したら続きから鳴る）
        let wanted = if paused && self.gain <= 0.0 {
            0
        } else {
            frames
        };
        let available = self.consumer.slots() / OUTPUT_CHANNELS;
        let take = wanted.min(available);

        let mut written = 0;
        if take > 0
            && let Ok(chunk) = self.consumer.read_chunk(take * OUTPUT_CHANNELS)
        {
            let (first, second) = chunk.as_slices();
            // リングの折り返しは、偶数個（フレームの境界）で分かれる（容量が偶数のため）
            let frames = first
                .as_chunks::<OUTPUT_CHANNELS>()
                .0
                .iter()
                .chain(second.as_chunks::<OUTPUT_CHANNELS>().0);
            for frame in frames {
                self.gain = if self.gain < target {
                    (self.gain + self.gain_step).min(target)
                } else {
                    (self.gain - self.gain_step).max(target)
                };
                write_frame(
                    &mut out[written * channels..(written + 1) * channels],
                    frame[0] * self.gain,
                    frame[1] * self.gain,
                );
                written += 1;
                // 一時停止で音量が0まで下がりきったら、そこで取り出すのをやめる
                if paused && self.gain <= 0.0 {
                    break;
                }
            }
            chunk.commit(written * OUTPUT_CHANNELS);
            self.frames_read += written as u64;
        }
        // 足りない分は無音
        out[written * channels..].fill(0.0);
        if !paused && written < frames {
            // 音声が途切れた後は、音量を0から上げ直す（急に鳴り始めるノイズを防ぐ）
            self.gain = 0.0;
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }

        self.shared
            .frames_read
            .store(self.frames_read, Ordering::Release);
    }

    /// エンジンが捨てるよう指定した位置まで、バッファの音声を読み捨てる
    fn discard_stale_frames(&mut self) {
        let discard_until = self.shared.discard_until.load(Ordering::Acquire);
        if self.frames_read >= discard_until {
            return;
        }
        let available = (self.consumer.slots() / OUTPUT_CHANNELS) as u64;
        let frames = (discard_until - self.frames_read).min(available);
        if frames > 0
            && let Ok(chunk) = self.consumer.read_chunk(frames as usize * OUTPUT_CHANNELS)
        {
            chunk.commit_all();
            self.frames_read += frames;
        }
        // 続きではない音声へ切り替わるため、音量を0から上げ直す
        self.gain = 0.0;
    }
}

/// 1フレーム分（左・右）を、出力のチャンネル数に合わせて書く
fn write_frame(out: &mut [f32], left: f32, right: f32) {
    match out {
        [] => {}
        [mono] => *mono = (left + right) * 0.5,
        [out_left, out_right, rest @ ..] => {
            *out_left = left;
            *out_right = right;
            rest.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 1000;

    fn setup(frames: usize) -> (rtrb::Producer<f32>, Renderer, Arc<RenderShared>) {
        let (producer, consumer) = rtrb::RingBuffer::new(frames * OUTPUT_CHANNELS);
        let shared = RenderShared::new(1.0, false);
        let renderer = Renderer::new(consumer, shared.clone(), RATE);
        (producer, renderer, shared)
    }

    /// 左が`value`、右が`-value`のフレームを書く
    fn push(producer: &mut rtrb::Producer<f32>, values: impl IntoIterator<Item = f32>) {
        for value in values {
            producer.push(value).unwrap();
            producer.push(-value).unwrap();
        }
    }

    #[test]
    fn test_renders_frames_and_counts_them() {
        let (mut producer, mut renderer, shared) = setup(64);
        push(&mut producer, [1.0; 40]);

        let mut out = [9.0f32; 60];
        renderer.render(&mut out, 2);

        assert_eq!(shared.frames_read(), 30);
        // 音量は0から少しずつ上がり、15ms（15フレーム）で設定の値になる
        assert!(out[0] > 0.0 && out[0] < 0.1);
        assert!(out[0] < out[2] && out[2] < out[4]);
        assert_eq!((out[58], out[59]), (1.0, -1.0));
    }

    #[test]
    fn test_underrun_outputs_silence_without_advancing() {
        let (mut producer, mut renderer, shared) = setup(64);
        push(&mut producer, [1.0; 5]);

        let mut out = [9.0f32; 20];
        renderer.render(&mut out, 2);

        assert_eq!(shared.frames_read(), 5);
        assert!(out[10..].iter().all(|&sample| sample == 0.0));

        // 音声がなければ、すべて無音で位置も進まない
        let mut out = [9.0f32; 20];
        renderer.render(&mut out, 2);
        assert_eq!(shared.frames_read(), 5);
        assert!(out.iter().all(|&sample| sample == 0.0));
        assert_eq!(shared.underruns(), 2);
    }

    #[test]
    fn test_pause_fades_out_then_stops_consuming() {
        let (mut producer, mut renderer, shared) = setup(256);
        push(&mut producer, [1.0; 200]);
        let mut out = [0.0f32; 80];
        renderer.render(&mut out, 2); // 40フレーム。音量は上がりきっている
        assert_eq!(out[78], 1.0);

        shared.set_paused(true);
        renderer.render(&mut out, 2);
        // 少しずつ下がって無音になる（下がりきるまでの15フレームだけを取り出す）
        assert!(out[0] < 1.0 && out[0] > 0.8);
        assert_eq!(out[78], 0.0);
        let after_fade = shared.frames_read();
        assert!((55..=56).contains(&after_fade), "{after_fade}");

        // 下がりきった後は取り出さない
        renderer.render(&mut out, 2);
        assert_eq!(shared.frames_read(), after_fade);
        assert!(out.iter().all(|&sample| sample == 0.0));

        // 再開すると、続きから少しずつ上がる
        shared.set_paused(false);
        renderer.render(&mut out, 2);
        assert!(shared.frames_read() > after_fade);
        assert!(out[0] > 0.0 && out[0] < 0.1);
    }

    #[test]
    fn test_volume_changes_gradually() {
        let (mut producer, mut renderer, shared) = setup(256);
        push(&mut producer, [1.0; 200]);
        let mut out = [0.0f32; 80];
        renderer.render(&mut out, 2);

        shared.set_volume(0.5);
        renderer.render(&mut out, 2);
        assert!(out[0] < 1.0 && out[0] > 0.9);
        assert_eq!(out[78], 0.5);
    }

    #[test]
    fn test_discards_frames_before_the_given_position() {
        let (mut producer, mut renderer, shared) = setup(256);
        push(&mut producer, [0.25; 50]);
        let mut out = [0.0f32; 20];
        renderer.render(&mut out, 2); // 10フレーム

        // シーク: ここまでに書いた50フレームを捨て、新しい音声を書く
        shared.discard_until(50);
        push(&mut producer, [0.75; 30]);
        let mut out = [0.0f32; 40];
        renderer.render(&mut out, 2);

        assert_eq!(shared.frames_read(), 70);
        // 古い音声（0.25）は出ず、新しい音声が0から上がって出る
        assert!(out.iter().step_by(2).all(|&sample| sample <= 0.75));
        assert!(out[0] > 0.0 && out[0] < 0.25 * 0.75);
        assert_eq!(out[38], 0.75);
    }

    #[test]
    fn test_writes_to_mono_and_multichannel_outputs() {
        let (mut producer, mut renderer, _shared) = setup(256);
        push(&mut producer, [1.0; 100]);
        let mut warmup = [0.0f32; 80];
        renderer.render(&mut warmup, 2);

        // モノラル: 左右の平均（1と-1で0）
        let mut mono = [9.0f32; 4];
        renderer.render(&mut mono, 1);
        assert!(mono.iter().all(|&sample| sample == 0.0));

        // 4チャンネル: 前の左右だけに出す
        let mut quad = [9.0f32; 8];
        renderer.render(&mut quad, 4);
        assert_eq!(quad, [1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0]);
    }
}
