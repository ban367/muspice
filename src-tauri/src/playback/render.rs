//! 出力のコールバックで音声を取り出す処理
//!
//! エンジンのスレッドがリングバッファへ書いた音声（ステレオ・出力のサンプルレート）を、
//! 出力デバイスのコールバックが取り出す。コールバックは、OSの音声のスレッドで決まった時間内に
//! 終える必要があるため、ロックの取得・ファイルの読み取りをせず、メモリは作る時に確保した分だけを
//! 使う（作業領域は1秒分。それより長いバッファを渡された時だけ、確保し直す）。
//! エンジンとのやり取りは、リングバッファとアトミックな値（`RenderShared`）だけで行う。
//!
//! - 音量と一時停止は、ここで反映する（バッファにたまっている分を待たずに、すぐ効く）。
//!   値が変わるときは短い時間をかけて滑らかに変え、ノイズ（プチッという音）を防ぐ
//! - 再生速度も、ここで反映する（`stretch`）。リングバッファの音声は元の速さのままで、ここで
//!   取り出す量が速さの分だけ増減する。再生位置は取り出したフレーム数から求めるため、速さを
//!   変えても、曲の中の位置（元の曲の時間）のまま進む
//! - シーク・曲の切り替えでは、エンジンが「この位置より前は捨てる」（`discard_until`）を書き、
//!   ここでバッファにたまっている古い音声を読み捨てる
//! - 取り出したフレーム数（`frames_read`）から、エンジンが再生位置と曲の切り替わりを求める
//! - 取り出した音声に、再生速度 → イコライザ → 音量・一時停止 → リミッターの順でかけて出す（`effects`）。
//!   リミッターを最後に置くのは、音量を下げていて最大を超えない時に、余計に抑えないため

use super::convert::OUTPUT_CHANNELS;
use super::effects::{EffectParams, Equalizer, Limiter};
use super::stretch::{FrameSource, MAX_SPEED, MIN_SPEED, TimeStretcher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// 音量・一時停止の切り替えを滑らかにする時間（秒）
const GAIN_RAMP_SECONDS: f32 = 0.015;

/// エンジンのスレッドと出力のコールバックで共有する値
pub struct RenderShared {
    /// 鳴らし終えたフレーム数の合計（読み捨てた分を含む。リングバッファから取り出したフレーム数
    /// から、再生速度の変更の中に残っている分を引いたもの）
    frames_read: AtomicU64,
    /// この位置（フレーム数の合計）より前のフレームは、出力せずに読み捨てる
    discard_until: AtomicU64,
    /// 音量（0.0〜1.0のf32のビット列）
    volume: AtomicU32,
    /// 再生速度（倍。f32のビット列）
    speed: AtomicU32,
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
            speed: AtomicU32::new(1.0f32.to_bits()),
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

    /// 再生速度（倍）を設定する（範囲の外の値は収め、数値でない値は1倍にする）
    pub fn set_speed(&self, speed: f32) {
        let speed = if speed.is_finite() {
            speed.clamp(MIN_SPEED, MAX_SPEED)
        } else {
            1.0
        };
        self.speed.store(speed.to_bits(), Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }
}

/// リングバッファを、再生速度の変更の入力にする（取り出したフレーム数を数える）
struct RingSource<'a> {
    consumer: &'a mut rtrb::Consumer<f32>,
    frames_read: &'a mut u64,
}

impl FrameSource for RingSource<'_> {
    fn available(&self) -> usize {
        self.consumer.slots() / OUTPUT_CHANNELS
    }

    fn read(&mut self, out: &mut [f32]) {
        if let Ok(chunk) = self.consumer.read_chunk(out.len()) {
            let (first, second) = chunk.as_slices();
            out[..first.len()].copy_from_slice(first);
            out[first.len()..].copy_from_slice(second);
            chunk.commit_all();
            *self.frames_read += (out.len() / OUTPUT_CHANNELS) as u64;
        } else {
            out.fill(0.0);
        }
    }

    fn skip(&mut self, frames: usize) {
        if let Ok(chunk) = self.consumer.read_chunk(frames * OUTPUT_CHANNELS) {
            chunk.commit_all();
            *self.frames_read += frames as u64;
        }
    }
}

/// 出力のコールバックの中で、リングバッファから音声を取り出す
pub struct Renderer {
    consumer: rtrb::Consumer<f32>,
    shared: Arc<RenderShared>,
    /// リングバッファから取り出したフレーム数の合計
    frames_read: u64,
    /// 鳴らし終えたフレーム数の合計（`RenderShared::frames_read`へ書く値。戻らないようにする）
    frames_played: u64,
    /// 最後に見た、読み捨てる位置の指定（`RenderShared::discard_until`）
    discard_seen: u64,
    /// 再生速度の変更
    stretcher: TimeStretcher,
    /// 今かけている音量（目標の値へ少しずつ近づける）
    gain: f32,
    /// 1フレームあたりの音量の変化の上限
    gain_step: f32,
    sample_rate: u32,
    /// 取り出した音声（ステレオ）を加工するための作業用のバッファ
    stereo: Vec<f32>,
    /// イコライザとリミッター（`with_effects`でつなぐ）
    effects: Option<Effects>,
}

/// 出力の直前にかける加工
struct Effects {
    params: Arc<EffectParams>,
    equalizer: Equalizer,
    limiter: Limiter,
}

impl Renderer {
    pub fn new(consumer: rtrb::Consumer<f32>, shared: Arc<RenderShared>, sample_rate: u32) -> Self {
        Self {
            consumer,
            shared,
            frames_read: 0,
            frames_played: 0,
            discard_seen: 0,
            stretcher: TimeStretcher::new(sample_rate),
            gain: 0.0,
            gain_step: 1.0 / (GAIN_RAMP_SECONDS * sample_rate as f32),
            sample_rate,
            // 作業領域は、作る時に確保しておく（コールバックの中で確保しない）
            stereo: Vec::with_capacity(sample_rate as usize * OUTPUT_CHANNELS),
            effects: None,
        }
    }

    /// イコライザとリミッターをつなぐ（設定は`params`から読む）
    pub fn with_effects(mut self, params: Arc<EffectParams>) -> Self {
        self.effects = Some(Effects {
            params,
            equalizer: Equalizer::new(self.sample_rate),
            limiter: Limiter::new(self.sample_rate),
        });
        self
    }

    /// イコライザとリミッターを外す（テストで、取り出した音声をそのままの位置で確かめるため。
    /// リミッターは、音声を先読みの時間だけ遅らせる）
    #[cfg(test)]
    pub fn without_effects(mut self) -> Self {
        self.effects = None;
        self
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
        // 一時停止中は、音量が0まで下がりきる分だけを取り出す（再開したら続きから鳴る）
        let wanted = if paused {
            // 丸めの誤差で残ったごく小さい音量は、下がりきったものとして扱う
            let remaining = (self.gain / self.gain_step - 1e-3).ceil().max(0.0) as usize;
            if remaining == 0 {
                self.gain = 0.0;
            }
            remaining.min(frames)
        } else {
            frames
        };
        // 取り出した音声をステレオのまま作業用のバッファへ移す（足りない分は無音。作業領域は、作る時に
        // 確保した1秒分の中で使う）
        self.stereo.resize(frames * OUTPUT_CHANNELS, 0.0);
        let speed = f32::from_bits(self.shared.speed.load(Ordering::Relaxed));
        let mut take = 0;
        // 再生速度を変えている間（1倍に戻した直後の、残りを出し切るまでを含む）は、時間伸縮を通す
        if speed != 1.0 || !self.stretcher.is_idle() {
            let mut source = RingSource {
                consumer: &mut self.consumer,
                frames_read: &mut self.frames_read,
            };
            take = self.stretcher.process(
                speed,
                &mut source,
                &mut self.stereo[..wanted * OUTPUT_CHANNELS],
            );
        }
        // 1倍の間は、そのまま取り出す（音声を一切変えない）
        if take < wanted && speed == 1.0 && self.stretcher.is_idle() {
            let available = self.consumer.slots() / OUTPUT_CHANNELS;
            let count = (wanted - take).min(available);
            let audio = &mut self.stereo[take * OUTPUT_CHANNELS..(take + count) * OUTPUT_CHANNELS];
            if count > 0
                && let Ok(chunk) = self.consumer.read_chunk(count * OUTPUT_CHANNELS)
            {
                let (first, second) = chunk.as_slices();
                audio[..first.len()].copy_from_slice(first);
                audio[first.len()..].copy_from_slice(second);
                chunk.commit_all();
                self.frames_read += count as u64;
                take += count;
            }
        }
        self.stereo[take * OUTPUT_CHANNELS..].fill(0.0);

        if let Some(effects) = &mut self.effects {
            effects.equalizer.process(&mut self.stereo, &effects.params);
        }

        // 音量・一時停止（目標の値へ、少しずつ近づける）
        for frame in self.stereo[..take * OUTPUT_CHANNELS]
            .as_chunks_mut::<OUTPUT_CHANNELS>()
            .0
        {
            self.gain = if self.gain < target {
                (self.gain + self.gain_step).min(target)
            } else {
                (self.gain - self.gain_step).max(target)
            };
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
        // 音声の後ろ（足りなかった分）は、イコライザの響きが残っていても出さない
        self.stereo[take * OUTPUT_CHANNELS..].fill(0.0);

        if let Some(effects) = &mut self.effects {
            effects.limiter.process(&mut self.stereo);
        }

        for (frame, out) in self
            .stereo
            .as_chunks::<OUTPUT_CHANNELS>()
            .0
            .iter()
            .zip(out.chunks_exact_mut(channels.max(1)))
        {
            write_frame(out, frame[0], frame[1]);
        }

        if !paused && take < frames {
            // 音声が途切れた後は、音量を0から上げ直す（急に鳴り始めるノイズを防ぐ）
            self.gain = 0.0;
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }

        // 再生速度の変更の中に残っている分は、まだ鳴っていない
        self.frames_played = self
            .frames_read
            .saturating_sub(self.stretcher.pending_source_frames())
            .max(self.frames_played);
        self.shared
            .frames_read
            .store(self.frames_played, Ordering::Release);
    }

    /// エンジンが捨てるよう指定した位置まで、バッファの音声を読み捨てる
    fn discard_stale_frames(&mut self) {
        let discard_until = self.shared.discard_until.load(Ordering::Acquire);
        // 新しく指定された: 再生速度の変更の中に残っている音声も、捨てる対象
        if discard_until != self.discard_seen {
            self.discard_seen = discard_until;
            if !self.stretcher.is_idle() {
                self.stretcher.reset();
                self.reset_after_discard();
            }
        }
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
        self.reset_after_discard();
    }

    /// 続きではない音声へ切り替わるため、音量を0から上げ直し、加工の中に残っている前の音声を消す
    fn reset_after_discard(&mut self) {
        self.gain = 0.0;
        if let Some(effects) = &mut self.effects {
            effects.equalizer.reset();
            effects.limiter.reset();
        }
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

    #[test]
    fn test_render_does_not_grow_the_work_buffer() {
        let (mut producer, mut renderer, shared) = setup(4_000);
        let capacity = renderer.stereo.capacity();
        assert!(capacity >= RATE as usize * OUTPUT_CHANNELS);

        push(&mut producer, (0..2_000).map(|i| i as f32 / 2_000.0));
        // 出力のバッファの長さが変わっても（ステレオ・6チャンネル）、1秒分までは確保し直さない
        for (frames, channels) in [(64, 2), (512, 2), (1_000, 2), (300, 6)] {
            renderer.render(&mut vec![0.0; frames * channels], channels);
        }
        shared.set_speed(1.5);
        renderer.render(&mut [0.0; 1_000 * 2], 2);
        assert_eq!(renderer.stereo.capacity(), capacity);
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

    /// 再生速度を変えると、同じ長さの出力に使う音声の量が、速さの分だけ変わる
    #[test]
    fn test_speed_changes_how_many_frames_are_consumed() {
        let (mut producer, mut renderer, shared) = setup(8_000);
        push(
            &mut producer,
            (0..8_000).map(|frame| (frame as f32 * 0.3).sin() * 0.5),
        );

        // 2倍: 1,000フレームの出力に、およそ2,000フレームを使う
        shared.set_speed(2.0);
        let mut out = [0.0f32; 2_000];
        renderer.render(&mut out, 2);
        let fast = shared.frames_read();
        assert!(fast.abs_diff(2_000) < 150, "{fast}");
        // 音は出ていて、左右は逆のまま
        assert!(out.iter().any(|sample| sample.abs() > 0.3));
        assert!(
            out.as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == -frame[1])
        );

        // 0.5倍: およそ500フレームを使う
        shared.set_speed(0.5);
        renderer.render(&mut out, 2);
        let slow = shared.frames_read() - fast;
        assert!(slow.abs_diff(500) < 150, "{slow}");

        // 範囲の外の値は、範囲の中に収める
        shared.set_speed(100.0);
        let before = shared.frames_read();
        renderer.render(&mut out, 2);
        assert!((shared.frames_read() - before).abs_diff(2_000) < 150);
    }

    /// 1倍に戻すと、残りを出し切った後は、音声をそのまま取り出す
    #[test]
    fn test_normal_speed_passes_frames_through_after_a_speed_change() {
        let (mut producer, mut renderer, shared) = setup(8_000);
        // どの位置か分かるよう、フレームの番号を値にする
        push(&mut producer, (0..8_000).map(|frame| frame as f32));

        shared.set_speed(1.5);
        let mut out = [0.0f32; 1_000];
        renderer.render(&mut out, 2);

        shared.set_speed(1.0);
        renderer.render(&mut out, 2);
        // 鳴らし終えた位置の次のフレームから、そのまま並ぶ
        let position = shared.frames_read();
        renderer.render(&mut out, 2);
        for (index, frame) in out.as_chunks::<2>().0.iter().enumerate() {
            let expected = (position + index as u64) as f32;
            assert_eq!(*frame, [expected, -expected], "frame {index}");
        }
        assert_eq!(shared.frames_read(), position + 500);
    }

    /// 捨てるよう指定されたら、再生速度の変更の中に残っている古い音声も出さない
    #[test]
    fn test_discard_drops_audio_held_by_the_speed_change() {
        let (mut producer, mut renderer, shared) = setup(8_000);
        push(&mut producer, [1.0; 300]);
        shared.set_speed(0.5);
        let mut out = [0.0f32; 200];
        renderer.render(&mut out, 2);
        // 取り出した音声の一部は、まだ鳴っていない
        assert!(shared.frames_read() < renderer.frames_read());

        // 古い音声（1.0）を捨てさせ、新しい音声（0.25）を書く
        shared.discard_until(300);
        push(&mut producer, [0.25; 2_000]);
        let mut out = [0.0f32; 400];
        renderer.render(&mut out, 2);
        assert!(shared.frames_read() >= 300);
        assert!(out.iter().all(|sample| sample.abs() <= 0.25 + 1e-6));
        assert!((out[398] - 0.25).abs() < 1e-6);
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

    #[test]
    fn test_effects_equalize_and_limit_the_output() {
        let (mut producer, renderer, _shared) = setup(4_096);
        let params = Arc::new(EffectParams::default());
        let mut renderer = renderer.with_effects(params.clone());
        // 最大の2倍の音声
        push(&mut producer, [2.0; 2_000]);

        let mut out = [0.0f32; 2_000];
        renderer.render(&mut out, 2);

        // リミッターが最大に収める（音量が上がりきった後の値は、上限の近く）
        assert!(out.iter().all(|sample| sample.abs() <= 1.0));
        assert!(out[1_998] > 0.9);

        // イコライザの設定は、次に取り出す時から効く（1kHzのバンドは、直流にはほぼ効かない）
        let mut gains = [0.0; 10];
        gains[5] = -12.0;
        params.set_equalizer(true, &gains);
        renderer.render(&mut out, 2);
        assert!(out[1_998] > 0.9);
    }

    #[test]
    fn test_effects_delay_quiet_audio_without_changing_it() {
        let (mut producer, renderer, shared) = setup(256);
        let mut renderer = renderer.with_effects(Arc::new(EffectParams::default()));
        push(&mut producer, [0.5; 100]);

        let mut out = [0.0f32; 200];
        renderer.render(&mut out, 2);

        // 位置（取り出したフレーム数）は遅れを含まない
        assert_eq!(shared.frames_read(), 100);
        // 音声は、リミッターの先読み（5ms = 5フレーム）だけ遅れて、同じ値で出る
        assert_eq!(&out[..10], &[0.0; 10]);
        assert_eq!((out[60], out[61]), (0.5, -0.5));
        assert_eq!((out[198], out[199]), (0.5, -0.5));
    }
}
