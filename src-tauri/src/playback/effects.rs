//! 出力の直前にかける音の加工: イコライザ・クロスフィード・リミッター
//!
//! どれも出力のコールバック（`render`）の中で動く。リングバッファより後ろにあるため、
//! イコライザのスライダーを動かした結果が、たまっている音声（約0.5秒）を待たずに聞こえる。
//! 設定は`EffectParams`（アトミックな値）でエンジンのスレッドから渡し、ロックを取らない。
//!
//! - イコライザ: 10バンドのピーキングフィルター。周波数・Q・係数の式は、Web Audioの
//!   `BiquadFilterNode`（`peaking`）と同じにしてある（以前のaudio要素での再生と、同じ設定で同じ特性になる）
//! - クロスフィード: ヘッドホン向けに、左右の音を少し混ぜる。bs2b（Bauer stereophonic-to-binaural）の方式で、
//!   反対側のチャンネルの低い音を少し遅らせて足し、同じ側の高い音をその分だけ持ち上げる（1次のフィルター）。
//!   左右に同じ音（モノラル）の低い音は、音量が変わらない
//! - リミッター: 出力が最大（0dBFS）を超えそうな時だけ、一時的に音量を下げる。イコライザでの
//!   持ち上げのほか、音量の正規化・クロスフェードの重なり・非可逆の形式のデコード結果・
//!   サンプルレートの変換で最大を超える場合にも効く。超えない音には何もしない

use super::convert::OUTPUT_CHANNELS;
use crate::settings::CrossfeedLevel;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

/// イコライザのバンドの周波数（Hz。フロントエンドの`EQ_FREQUENCIES`と同じ）
pub const EQ_FREQUENCIES: [f64; EQ_BANDS] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// イコライザのバンドの数
pub const EQ_BANDS: usize = 10;

/// バンドのゲインの範囲（dB。フロントエンドの`MIN_GAIN`・`MAX_GAIN`と同じ）
pub const EQ_MAX_GAIN_DB: f32 = 12.0;

/// フィルターのQ（Web Audioの経路で使っている値と同じ）
const EQ_Q: f64 = 1.4;

/// ゲインを変える速さ（dB/秒）。一度に変えると、フィルターの切り替わりでノイズが出る
const EQ_SLEW_DB_PER_SECOND: f32 = 120.0;

/// リミッターが抑える上限（最大の少し下。-0.1dBFS）
const LIMITER_CEILING: f32 = 0.988_553_1;

/// リミッターが先読みする時間（秒）。この時間だけ音声を遅らせ、最大を超える前から音量を下げ始める
const LIMITER_LOOKAHEAD_SECONDS: f64 = 0.005;

/// リミッターが下げた音量を元に戻す速さ（時定数。秒）
const LIMITER_RELEASE_SECONDS: f64 = 0.2;

/// リミッターの音量がここまで戻ったら、戻りきったものとして1にする（差は0.001dB未満）
const LIMITER_RESTORED_GAIN: f64 = 0.9999;

/// クロスフィードのオン・オフを切り替える時に、元の音と混ぜた音を入れ替える時間（秒）
const CROSSFEED_FADE_SECONDS: f32 = 0.02;

/// エンジンのスレッドから出力のコールバックへ渡す、加工の設定
#[derive(Debug)]
pub struct EffectParams {
    equalizer_enabled: AtomicBool,
    /// バンドごとのゲイン（dBのf32のビット列）
    equalizer_gains: [AtomicU32; EQ_BANDS],
    /// クロスフィードの強さ（0はオフ。`crossfeed_code`）
    crossfeed: AtomicU8,
    /// 設定を変えるたびに増やす番号（コールバックは、番号が変わった時だけ設定を読み直す）
    version: AtomicU64,
}

impl Default for EffectParams {
    fn default() -> Self {
        Self {
            equalizer_enabled: AtomicBool::new(false),
            equalizer_gains: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            crossfeed: AtomicU8::new(0),
            version: AtomicU64::new(0),
        }
    }
}

impl EffectParams {
    /// イコライザの設定を変える（ゲインは範囲に収める）
    pub fn set_equalizer(&self, enabled: bool, gains: &[f32; EQ_BANDS]) {
        self.equalizer_enabled.store(enabled, Ordering::Relaxed);
        for (slot, gain) in self.equalizer_gains.iter().zip(gains) {
            let gain = if gain.is_finite() {
                gain.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB)
            } else {
                0.0
            };
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    /// クロスフィードの強さを変える（Noneは、クロスフィードしない）
    pub fn set_crossfeed(&self, level: Option<CrossfeedLevel>) {
        let code = match level {
            None => 0,
            Some(CrossfeedLevel::Weak) => 1,
            Some(CrossfeedLevel::Medium) => 2,
            Some(CrossfeedLevel::Strong) => 3,
        };
        if self.crossfeed.swap(code, Ordering::Relaxed) != code {
            self.version.fetch_add(1, Ordering::Release);
        }
    }

    /// クロスフィードの強さ（オフの間はNone）
    fn crossfeed_level(&self) -> Option<CrossfeedLevel> {
        match self.crossfeed.load(Ordering::Relaxed) {
            1 => Some(CrossfeedLevel::Weak),
            2 => Some(CrossfeedLevel::Medium),
            3 => Some(CrossfeedLevel::Strong),
            _ => None,
        }
    }

    /// イコライザが実際にかけるゲイン（無効の間は、すべて0dB）
    fn equalizer_targets(&self) -> [f32; EQ_BANDS] {
        if !self.equalizer_enabled.load(Ordering::Relaxed) {
            return [0.0; EQ_BANDS];
        }
        std::array::from_fn(|band| {
            f32::from_bits(self.equalizer_gains[band].load(Ordering::Relaxed))
        })
    }
}

/// 2次のフィルター（転置直接形II）の係数
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    /// 何も変えないフィルター
    const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// ピーキングフィルター（Web Audioの`BiquadFilterNode`の`peaking`と同じ式）
    fn peaking(frequency: f64, gain_db: f64, sample_rate: f64) -> Self {
        // ナイキスト周波数に届かないバンド（サンプルレートの低い出力での16kHzなど）は、何もしない
        if gain_db == 0.0 || frequency >= sample_rate * 0.49 {
            return Self::IDENTITY;
        }
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * frequency / sample_rate;
        let alpha = w0.sin() / (2.0 * EQ_Q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos_w0 / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

/// 10バンドのイコライザ
pub struct Equalizer {
    sample_rate: f64,
    /// 読み込んだ設定の番号
    version: u64,
    /// 目標のゲイン（dB）と、今かけているゲイン（目標へ少しずつ近づける）
    targets: [f32; EQ_BANDS],
    gains: [f32; EQ_BANDS],
    filters: [Biquad; EQ_BANDS],
    /// フィルターの内部状態（バンド × チャンネル × 2）
    state: [[[f64; 2]; OUTPUT_CHANNELS]; EQ_BANDS],
}

impl Equalizer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: f64::from(sample_rate),
            // 最初のコールバックで、必ず設定を読む
            version: u64::MAX,
            targets: [0.0; EQ_BANDS],
            gains: [0.0; EQ_BANDS],
            filters: [Biquad::IDENTITY; EQ_BANDS],
            state: [[[0.0; 2]; OUTPUT_CHANNELS]; EQ_BANDS],
        }
    }

    /// フィルターの内部状態を消す（続きではない音声へ切り替わる時）
    pub fn reset(&mut self) {
        self.state = [[[0.0; 2]; OUTPUT_CHANNELS]; EQ_BANDS];
    }

    /// サンプル（ステレオのインターリーブ）にイコライザをかける
    pub fn process(&mut self, samples: &mut [f32], params: &EffectParams) {
        let version = params.version.load(Ordering::Acquire);
        if version != self.version {
            self.version = version;
            self.targets = params.equalizer_targets();
        }
        self.approach_targets(samples.len() / OUTPUT_CHANNELS);

        // すべて0dBなら、音声に触れない（イコライザが無効の間は、元の音声のまま出す）
        if self.gains.iter().all(|&gain| gain == 0.0) {
            return;
        }
        for (band, filter) in self.filters.iter().enumerate() {
            if self.gains[band] == 0.0 {
                continue;
            }
            for frame in samples.as_chunks_mut::<OUTPUT_CHANNELS>().0 {
                for (channel, sample) in frame.iter_mut().enumerate() {
                    let state = &mut self.state[band][channel];
                    let input = f64::from(*sample);
                    let output = filter.b0 * input + state[0];
                    state[0] = filter.b1 * input - filter.a1 * output + state[1];
                    state[1] = filter.b2 * input - filter.a2 * output;
                    *sample = output as f32;
                }
            }
        }
    }

    /// 今かけているゲインを、`frames`フレーム分の時間だけ目標へ近づけ、フィルターを作り直す
    fn approach_targets(&mut self, frames: usize) {
        let step = EQ_SLEW_DB_PER_SECOND * frames as f32 / self.sample_rate as f32;
        for (band, frequency) in EQ_FREQUENCIES.into_iter().enumerate() {
            let (gain, target) = (self.gains[band], self.targets[band]);
            if gain == target {
                continue;
            }
            let next = if (target - gain).abs() <= step {
                target
            } else {
                gain + step.copysign(target - gain)
            };
            self.gains[band] = next;
            self.filters[band] = Biquad::peaking(frequency, f64::from(next), self.sample_rate);
            if next == 0.0 {
                // 使わなくなったフィルターの状態は、次に使う時のために消しておく
                self.state[band] = [[0.0; 2]; OUTPUT_CHANNELS];
            }
        }
    }
}

/// クロスフィードのフィルターの係数（bs2bの方式）
#[derive(Debug, Clone, Copy)]
struct CrossfeedCoefficients {
    /// 反対側へ足す、低い音のフィルター（1次のローパス）
    a0_lo: f64,
    b1_lo: f64,
    /// 同じ側の、高い音を持ち上げるフィルター（1次のハイシェルフ）
    a0_hi: f64,
    a1_hi: f64,
    b1_hi: f64,
    /// 全体の音量（低い音のモノラルの音量を変えない）
    gain: f64,
}

impl CrossfeedCoefficients {
    /// 強さごとのカットオフ周波数（Hz）と、反対側へ足す量（dB。小さいほど多く混ぜる）
    ///
    /// bs2bでよく使われる組（弱: Jan Meier、中: Chu Moy、強: bs2bの既定）。
    fn parameters(level: CrossfeedLevel) -> (f64, f64) {
        match level {
            CrossfeedLevel::Weak => (650.0, 9.5),
            CrossfeedLevel::Medium => (700.0, 6.0),
            CrossfeedLevel::Strong => (700.0, 4.5),
        }
    }

    fn new(level: CrossfeedLevel, sample_rate: f64) -> Self {
        let (cutoff, feed_db) = Self::parameters(level);
        let low_gain_db = -feed_db * 5.0 / 6.0 - 3.0;
        let high_gain_db = feed_db / 6.0 - 3.0;
        let low_gain = 10f64.powf(low_gain_db / 20.0);
        let high_gain = 1.0 - 10f64.powf(high_gain_db / 20.0);
        let high_cutoff = cutoff * 2f64.powf((low_gain_db - 20.0 * high_gain.log10()) / 12.0);

        let x_lo = (-2.0 * std::f64::consts::PI * cutoff / sample_rate).exp();
        let x_hi = (-2.0 * std::f64::consts::PI * high_cutoff / sample_rate).exp();
        Self {
            a0_lo: low_gain * (1.0 - x_lo),
            b1_lo: x_lo,
            a0_hi: 1.0 - high_gain * (1.0 - x_hi),
            a1_hi: -x_hi,
            b1_hi: x_hi,
            gain: 1.0 / (1.0 - high_gain + low_gain),
        }
    }
}

/// クロスフィード
///
/// オン・オフと強さを切り替える時は、元の音と混ぜた音を短い時間（`CROSSFEED_FADE_SECONDS`）で入れ替える
/// （急に切り替えると、音が跳ねる）。強さを変える時は、いったん元の音へ戻してから、新しい強さの係数で
/// 混ぜ直す（混ぜている途中で係数を差し替えると、フィルターの状態と合わずに音が跳ねる）。
/// オフの間は、音声に触れない。
pub struct Crossfeed {
    sample_rate: f64,
    /// 読み込んだ設定の番号
    version: u64,
    /// 今の係数と、その強さ
    coefficients: Option<CrossfeedCoefficients>,
    level: Option<CrossfeedLevel>,
    /// 元の音へ戻しきったら切り替える強さ（混ぜている間に強さが変わった時）
    pending: Option<CrossfeedLevel>,
    /// 混ぜた音の割合（0で元の音、1で混ぜた音）と、その目標
    mix: f32,
    target_mix: f32,
    mix_step: f32,
    /// フィルターの内部状態（チャンネルごと）: ローパスの出力・ハイシェルフの出力・前の入力
    low: [f64; OUTPUT_CHANNELS],
    high: [f64; OUTPUT_CHANNELS],
    previous: [f64; OUTPUT_CHANNELS],
}

impl Crossfeed {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: f64::from(sample_rate),
            // 最初のコールバックで、必ず設定を読む
            version: u64::MAX,
            coefficients: None,
            level: None,
            pending: None,
            mix: 0.0,
            target_mix: 0.0,
            mix_step: 1.0 / (CROSSFEED_FADE_SECONDS * sample_rate as f32),
            low: [0.0; OUTPUT_CHANNELS],
            high: [0.0; OUTPUT_CHANNELS],
            previous: [0.0; OUTPUT_CHANNELS],
        }
    }

    /// フィルターの内部状態を消す（続きではない音声へ切り替わる時）
    pub fn reset(&mut self) {
        self.clear_state();
        self.mix = self.target_mix;
    }

    /// サンプル（ステレオのインターリーブ）にクロスフィードをかける
    pub fn process(&mut self, samples: &mut [f32], params: &EffectParams) {
        let version = params.version.load(Ordering::Acquire);
        if version != self.version {
            self.version = version;
            self.follow_level(params.crossfeed_level());
        }
        if self.mix == 0.0
            && let Some(level) = self.pending.take()
        {
            self.switch_to(level);
        }
        if self.mix == 0.0 && self.target_mix == 0.0 {
            return;
        }
        let Some(mut c) = self.coefficients else {
            return;
        };

        for frame in samples.as_chunks_mut::<OUTPUT_CHANNELS>().0 {
            let input = [f64::from(frame[0]), f64::from(frame[1])];
            let channels = self
                .low
                .iter_mut()
                .zip(&mut self.high)
                .zip(&self.previous)
                .zip(input);
            for (((low, high), previous), input) in channels {
                *low = c.a0_lo * input + c.b1_lo * *low;
                *high = c.a0_hi * input + c.a1_hi * previous + c.b1_hi * *high;
            }
            self.previous = input;
            let mixed = [
                ((self.high[0] + self.low[1]) * c.gain) as f32,
                ((self.high[1] + self.low[0]) * c.gain) as f32,
            ];

            self.mix = if self.mix < self.target_mix {
                (self.mix + self.mix_step).min(self.target_mix)
            } else {
                (self.mix - self.mix_step).max(self.target_mix)
            };
            for (sample, wet) in frame.iter_mut().zip(mixed) {
                *sample += (wet - *sample) * self.mix;
            }
            // 元の音へ戻しきった: 待っていた強さの係数で、混ぜ直す
            if self.mix == 0.0
                && let Some(level) = self.pending.take()
            {
                c = self.switch_to(level);
            }
        }
        if self.mix == 0.0 && self.target_mix == 0.0 {
            // 元の音へ戻しきった: 次にオンにする時のために、状態を消す
            self.clear_state();
        }
    }

    /// 設定の強さ（Noneはオフ）に合わせて、混ぜる割合の目標と、切り替える強さを決める
    fn follow_level(&mut self, wanted: Option<CrossfeedLevel>) {
        match wanted {
            // 係数は、元の音へ戻しきるまで使う
            None => {
                self.pending = None;
                self.target_mix = 0.0;
            }
            Some(level) if self.level == Some(level) => {
                self.pending = None;
                self.target_mix = 1.0;
            }
            Some(level) if self.mix == 0.0 => {
                self.switch_to(level);
            }
            // 混ぜている間に強さが変わった: いったん元の音へ戻してから切り替える
            Some(level) => {
                self.pending = Some(level);
                self.target_mix = 0.0;
            }
        }
    }

    /// 強さの係数に切り替えて、混ぜ始める（元の音を出している間だけ呼ぶ。フィルターの状態は消す）
    fn switch_to(&mut self, level: CrossfeedLevel) -> CrossfeedCoefficients {
        let coefficients = CrossfeedCoefficients::new(level, self.sample_rate);
        self.coefficients = Some(coefficients);
        self.level = Some(level);
        self.clear_state();
        self.target_mix = 1.0;
        coefficients
    }

    fn clear_state(&mut self) {
        self.low = [0.0; OUTPUT_CHANNELS];
        self.high = [0.0; OUTPUT_CHANNELS];
        self.previous = [0.0; OUTPUT_CHANNELS];
    }
}

/// 先読みするリミッター
///
/// 音声を少し（`LIMITER_LOOKAHEAD_SECONDS`）遅らせ、その間に来る音声の最大の振幅を先に見て、
/// 上限を超える前から滑らかに音量を下げる。超える音声が過ぎたら、ゆっくり元に戻す。
/// 上限を超えない音声は、遅れるだけで値は変わらない。
pub struct Limiter {
    /// 遅らせている音声（インターリーブ。先読みのフレーム数の輪）
    delay: Vec<f32>,
    /// 先読みの範囲の中の「必要な音量」の最小を求めるための待ち行列（位置と値。値は先頭が最小で昇順）
    window: Vec<(u64, f32)>,
    window_head: usize,
    window_len: usize,
    /// 処理したフレーム数
    position: u64,
    lookahead: usize,
    /// 今かけている音量（1で、何もしない）。f32では1の近くで変化が丸められて戻りきらないため、f64で持つ
    gain: f64,
    attack: f64,
    release: f64,
}

impl Limiter {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let lookahead = ((rate * LIMITER_LOOKAHEAD_SECONDS) as usize).max(1);
        Self {
            delay: vec![0.0; lookahead * OUTPUT_CHANNELS],
            window: vec![(0, 1.0); lookahead + 1],
            window_head: 0,
            window_len: 0,
            position: 0,
            lookahead,
            gain: 1.0,
            // 先読みの時間の中で、必要な音量まで下がりきる速さにする
            attack: 1.0 - (-6.0 / lookahead as f64).exp(),
            release: 1.0 - (-1.0 / (rate * LIMITER_RELEASE_SECONDS)).exp(),
        }
    }

    /// 遅らせている音声を捨てる（続きではない音声へ切り替わる時）
    pub fn reset(&mut self) {
        self.delay.fill(0.0);
        self.window_len = 0;
        self.gain = 1.0;
    }

    /// サンプル（ステレオのインターリーブ）を、先読みの時間だけ遅らせて、上限に収める
    pub fn process(&mut self, samples: &mut [f32]) {
        let capacity = self.window.len();
        for frame in samples.as_chunks_mut::<OUTPUT_CHANNELS>().0 {
            // このフレームを上限に収めるのに必要な音量
            let peak = frame
                .iter()
                .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            let needed = if peak > LIMITER_CEILING {
                LIMITER_CEILING / peak
            } else {
                1.0
            };

            // 範囲から出た値を捨てる（このフレームの値を足す前に捨てる。範囲は、このフレームと
            // 先読みの分の`lookahead + 1`フレームで、足した後に捨てると容量を超える）
            while self.window_len > 0
                && self.window[self.window_head].0 + (self.lookahead as u64) < self.position
            {
                self.window_head = (self.window_head + 1) % capacity;
                self.window_len -= 1;
            }
            // 先読みの範囲の最小を保つ: これより大きい値は、この値がある間は最小にならないため捨てる
            while self.window_len > 0 {
                let back = (self.window_head + self.window_len - 1) % capacity;
                if self.window[back].1 < needed {
                    break;
                }
                self.window_len -= 1;
            }
            let back = (self.window_head + self.window_len) % capacity;
            self.window[back] = (self.position, needed);
            self.window_len += 1;
            let target = f64::from(self.window[self.window_head].1);

            // 下げる時は速く、戻す時はゆっくり
            let rate = if target < self.gain {
                self.attack
            } else {
                self.release
            };
            self.gain += (target - self.gain) * rate;
            if target >= 1.0 && self.gain > LIMITER_RESTORED_GAIN {
                // 戻りきったら、ちょうど1にする（超えない音声の値を変えない）
                self.gain = 1.0;
            }

            // 遅らせておいたフレームと入れ替えて出す
            let slot = (self.position % self.lookahead as u64) as usize * OUTPUT_CHANNELS;
            for (channel, sample) in frame.iter_mut().enumerate() {
                let delayed = std::mem::replace(&mut self.delay[slot + channel], *sample);
                // 下げるのが間に合わなかった分（ごくわずか）は、上限で切る
                *sample = (delayed * self.gain as f32).clamp(-1.0, 1.0);
            }
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// 左右で別の音（左右の振幅を指定した正弦波）
    fn panned_sine(frequency: f64, left: f32, right: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value = (2.0 * std::f64::consts::PI * frequency * i as f64 / f64::from(RATE))
                    .sin() as f32;
                [value * left, value * right]
            })
            .collect()
    }

    /// 後ろ半分（フィルター・切り替えが落ち着いた後）の、チャンネルごとの最大の振幅
    fn settled_peaks(samples: &[f32]) -> [f32; 2] {
        let half = &samples[samples.len() / 2..];
        let mut peaks = [0.0f32; 2];
        for frame in half.as_chunks::<OUTPUT_CHANNELS>().0 {
            for (peak, sample) in peaks.iter_mut().zip(frame) {
                *peak = peak.max(sample.abs());
            }
        }
        peaks
    }

    fn make_crossfeed(level: Option<CrossfeedLevel>) -> (Crossfeed, EffectParams) {
        let params = EffectParams::default();
        params.set_crossfeed(level);
        (Crossfeed::new(RATE), params)
    }

    #[test]
    fn test_crossfeed_off_leaves_the_audio_untouched() {
        let (mut crossfeed, params) = make_crossfeed(None);
        let original = panned_sine(200.0, 0.5, 0.0, 4_800);
        let mut samples = original.clone();
        crossfeed.process(&mut samples, &params);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_crossfeed_feeds_low_sounds_to_the_other_side() {
        let (mut crossfeed, params) = make_crossfeed(Some(CrossfeedLevel::Medium));
        // 左だけの低い音: 右にも出る（反対側へ足す量は、左の音より小さい）
        let mut samples = panned_sine(100.0, 0.5, 0.0, 48_000);
        crossfeed.process(&mut samples, &params);
        let [left, right] = settled_peaks(&samples);
        assert!(right > 0.05 && right < left, "{left} {right}");

        // 強いほど、多く混ぜる
        let ratio = |level| {
            let (mut crossfeed, params) = make_crossfeed(Some(level));
            let mut samples = panned_sine(100.0, 0.5, 0.0, 48_000);
            crossfeed.process(&mut samples, &params);
            let [left, right] = settled_peaks(&samples);
            right / left
        };
        assert!(ratio(CrossfeedLevel::Weak) < ratio(CrossfeedLevel::Medium));
        assert!(ratio(CrossfeedLevel::Medium) < ratio(CrossfeedLevel::Strong));
    }

    #[test]
    fn test_crossfeed_keeps_the_volume_of_low_mono_sounds() {
        let (mut crossfeed, params) = make_crossfeed(Some(CrossfeedLevel::Strong));
        let mut samples = panned_sine(50.0, 0.5, 0.5, 48_000);
        crossfeed.process(&mut samples, &params);
        let [left, right] = settled_peaks(&samples);
        assert!(
            (left - 0.5).abs() < 0.02 && (right - 0.5).abs() < 0.02,
            "{left} {right}"
        );
    }

    #[test]
    fn test_crossfeed_strength_change_goes_through_the_dry_sound() {
        let (mut crossfeed, params) = make_crossfeed(Some(CrossfeedLevel::Medium));
        // 100Hzの1秒分は、ちょうど100周期（続けて流しても、音が途切れない）
        let mut samples = panned_sine(100.0, 0.5, 0.0, 48_000);
        crossfeed.process(&mut samples, &params);
        let mut rights: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();

        // 同じ強さのままの設定の変更（イコライザの変更など）では、混ぜた音のまま
        params.set_equalizer(false, &[0.0; EQ_BANDS]);
        let mut block = panned_sine(100.0, 0.5, 0.0, 480);
        crossfeed.process(&mut block, &params);
        assert_eq!(crossfeed.mix, 1.0);
        rights.extend(block.iter().skip(1).step_by(2));

        // 強さを変えると、いったん元の音へ戻してから、新しい強さで混ぜ直す
        params.set_crossfeed(Some(CrossfeedLevel::Strong));
        let mut dipped = false;
        for _ in 0..10 {
            let mut block = panned_sine(100.0, 0.5, 0.0, 480);
            crossfeed.process(&mut block, &params);
            dipped |= crossfeed.mix < 0.5;
            rights.extend(block.iter().skip(1).step_by(2));
        }
        assert!(dipped);
        assert_eq!(crossfeed.level, Some(CrossfeedLevel::Strong));
        assert_eq!(crossfeed.mix, 1.0);
        // 右（混ぜた音）は、切り替えの間も跳ねない
        let jump = rights
            .windows(2)
            .fold(0.0f32, |max, pair| max.max((pair[1] - pair[0]).abs()));
        assert!(jump < 0.01, "{jump}");
    }

    #[test]
    fn test_crossfeed_switches_smoothly_and_stops_touching_the_audio() {
        let (mut crossfeed, params) = make_crossfeed(Some(CrossfeedLevel::Strong));
        let mut samples = panned_sine(100.0, 0.5, 0.0, 9_600);
        crossfeed.process(&mut samples, &params);
        // オンにした直後は、元の音から少しずつ混ぜた音へ変わる（右は0から始まる）
        assert_eq!(samples[1], 0.0);
        assert!(
            samples
                .as_chunks::<OUTPUT_CHANNELS>()
                .0
                .windows(2)
                .all(|pair| (pair[1][1] - pair[0][1]).abs() < 0.02)
        );

        // オフにすると元の音へ戻し、戻しきった後は音声に触れない
        params.set_crossfeed(None);
        let mut fading = panned_sine(100.0, 0.5, 0.0, 9_600);
        crossfeed.process(&mut fading, &params);
        let original = panned_sine(100.0, 0.5, 0.0, 4_800);
        let mut after = original.clone();
        crossfeed.process(&mut after, &params);
        assert_eq!(after, original);
    }

    /// 指定した周波数・振幅の正弦波（ステレオ。左右は同じ）
    fn sine(frequency: f64, amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value = (2.0 * std::f64::consts::PI * frequency * i as f64 / f64::from(RATE))
                    .sin() as f32
                    * amplitude;
                [value, value]
            })
            .collect()
    }

    /// 後ろ半分の最大の振幅（リミッターが落ち着いた後の大きさ）
    fn settled_peak(samples: &[f32]) -> f32 {
        samples[samples.len() / 2..]
            .iter()
            .fold(0.0, |peak, sample| peak.max(sample.abs()))
    }

    /// 実効値（サンプルの位置によらない、正弦波の大きさ）
    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn to_db(ratio: f32) -> f32 {
        20.0 * ratio.log10()
    }

    /// 設定を反映し終えたイコライザに正弦波を通し、通した後の大きさ（dB）を返す
    fn equalized_gain_db(params: &EffectParams, frequency: f64) -> f32 {
        let mut equalizer = Equalizer::new(RATE);
        // ゲインが目標に届くまで流す
        let mut warmup = sine(frequency, 0.1, RATE as usize);
        for block in warmup.chunks_mut(512) {
            equalizer.process(block, params);
        }
        let original = sine(frequency, 0.1, RATE as usize);
        let mut samples = original.clone();
        for block in samples.chunks_mut(512) {
            equalizer.process(block, params);
        }
        to_db(rms(&samples) / rms(&original))
    }

    fn gains(pairs: &[(usize, f32)]) -> [f32; EQ_BANDS] {
        let mut gains = [0.0; EQ_BANDS];
        for &(band, gain) in pairs {
            gains[band] = gain;
        }
        gains
    }

    #[test]
    fn test_equalizer_boosts_and_cuts_at_the_band_frequency() {
        // 1kHzを+6dB
        let params = EffectParams::default();
        params.set_equalizer(true, &gains(&[(5, 6.0)]));
        assert!((equalized_gain_db(&params, 1000.0) - 6.0).abs() < 0.05);
        // 離れた周波数は、ほとんど変わらない
        assert!(equalized_gain_db(&params, 62.0).abs() < 0.05);
        assert!(equalized_gain_db(&params, 16_000.0).abs() < 0.05);

        // 8kHzを-9dB
        params.set_equalizer(true, &gains(&[(8, -9.0)]));
        assert!((equalized_gain_db(&params, 8000.0) + 9.0).abs() < 0.05);
        assert!(equalized_gain_db(&params, 250.0).abs() < 0.05);
    }

    #[test]
    fn test_equalizer_matches_the_web_audio_peaking_response() {
        // 1kHz・+12dB・Q=1.4のフィルターの、各周波数での特性（dB）。期待する値は、Web Audioの仕様の
        // 式（peaking）から、この実装とは別に計算したもの（48kHz）
        let params = EffectParams::default();
        params.set_equalizer(true, &gains(&[(5, 12.0)]));

        for (frequency, expected) in [
            (250.0, 0.545),
            (500.0, 2.547),
            (707.0, 6.039),
            (1000.0, 12.0),
            (1414.0, 6.033),
            (2000.0, 2.527),
            (4000.0, 0.522),
        ] {
            let actual = equalized_gain_db(&params, frequency);
            assert!(
                (actual - expected).abs() < 0.02,
                "{frequency}Hz: {actual} vs {expected}"
            );
        }
    }

    #[test]
    fn test_disabled_equalizer_leaves_samples_untouched() {
        let params = EffectParams::default();
        params.set_equalizer(false, &gains(&[(0, 12.0), (5, -12.0)]));
        let original = sine(1000.0, 0.5, 4_800);
        let mut samples = original.clone();

        Equalizer::new(RATE).process(&mut samples, &params);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_equalizer_changes_gain_gradually() {
        let params = EffectParams::default();
        let mut equalizer = Equalizer::new(RATE);
        params.set_equalizer(true, &gains(&[(5, 12.0)]));

        // 10ms（480フレーム）では、1.2dBまでしか変わらない
        let mut block = sine(1000.0, 0.1, 480);
        equalizer.process(&mut block, &params);
        assert!((equalizer.gains[5] - 1.2).abs() < 1e-3);

        // 0.1秒あれば、12dBに届く
        for _ in 0..10 {
            equalizer.process(&mut block, &params);
        }
        assert_eq!(equalizer.gains[5], 12.0);

        // 無効にすると、0dBへ戻っていき、戻りきったら音声に触れない
        params.set_equalizer(false, &gains(&[(5, 12.0)]));
        for _ in 0..11 {
            equalizer.process(&mut block, &params);
        }
        assert_eq!(equalizer.gains, [0.0; EQ_BANDS]);
    }

    #[test]
    fn test_equalizer_params_are_clamped() {
        let params = EffectParams::default();
        params.set_equalizer(true, &gains(&[(0, 40.0), (1, -40.0), (2, f32::NAN)]));

        let targets = params.equalizer_targets();
        assert_eq!(&targets[..3], &[12.0, -12.0, 0.0]);
    }

    #[test]
    fn test_band_above_nyquist_is_skipped() {
        // 出力が22.05kHzなら、16kHzのバンドはナイキスト周波数（11.025kHz）を超える
        let identity = Biquad::peaking(16_000.0, 12.0, 22_050.0);
        assert_eq!((identity.b0, identity.b1, identity.a1), (1.0, 0.0, 0.0));
    }

    #[test]
    fn test_limiter_passes_quiet_audio_unchanged_after_the_delay() {
        let mut limiter = Limiter::new(RATE);
        let original = sine(440.0, 0.9, 4_800);
        let mut samples = original.clone();

        limiter.process(&mut samples);

        // 先読みの時間（5ms = 240フレーム）だけ遅れて、同じ値が出る
        let delay = 240 * 2;
        assert!(samples[..delay].iter().all(|&sample| sample == 0.0));
        assert_eq!(&samples[delay..], &original[..original.len() - delay]);
    }

    #[test]
    fn test_limiter_keeps_loud_audio_under_the_ceiling() {
        let mut limiter = Limiter::new(RATE);
        // 最大の2倍（+6dB）の正弦波
        let mut samples = sine(440.0, 2.0, 48_000);

        limiter.process(&mut samples);

        let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak <= 1.0, "{peak}");
        // 切るのではなく、音量を下げて収める（落ち着いた後の最大が、上限の近くにある）
        let settled = settled_peak(&samples);
        assert!(
            (0.95..=LIMITER_CEILING + 0.005).contains(&settled),
            "{settled}"
        );
        // 波形が歪んでいない: 上限に張り付いたサンプルがほとんどない
        let flat = samples[24_000..]
            .iter()
            .filter(|sample| sample.abs() >= 0.999)
            .count();
        assert_eq!(flat, 0);
    }

    #[test]
    fn test_limiter_lowers_the_gain_before_a_sudden_peak() {
        let mut limiter = Limiter::new(RATE);
        // 小さい音の途中に、最大の3倍の音が突然入る
        let mut samples = sine(440.0, 0.5, 9_600);
        for sample in &mut samples[4_800 * 2..(4_800 + 480) * 2] {
            *sample *= 6.0;
        }

        limiter.process(&mut samples);

        // 突然の大きな音でも、最大を超えない
        let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak <= 1.0, "{peak}");
        // 大きな音が過ぎると、元の音量へ戻っていく
        assert!(limiter.gain > 0.4 && limiter.gain < 1.0);
        let mut tail = sine(440.0, 0.5, 96_000);
        limiter.process(&mut tail);
        assert_eq!(limiter.gain, 1.0);
    }

    /// 上限を超える音が少しずつ小さくなり、必要な音量が先読みの長さより長く上がり続けても、
    /// 先読みの範囲の中の最小を保つ
    #[test]
    fn test_limiter_keeps_the_window_minimum_while_the_needed_gain_rises() {
        let mut limiter = Limiter::new(RATE);
        let lookahead = limiter.lookahead;
        // 最大の4倍から、フレームごとに小さくなる音
        let frames = lookahead * 3;
        let mut needed = Vec::new();
        for frame in 0..frames {
            let peak = 4.0 - 3.0 * frame as f32 / frames as f32;
            needed.push(LIMITER_CEILING / peak);
            limiter.process(&mut [peak, -peak]);

            // このフレームと、その前の先読みの分（遅らせて今出したフレームまで）の最小
            let from = needed.len().saturating_sub(lookahead + 1);
            let minimum = needed[from..].iter().copied().fold(f32::INFINITY, f32::min);
            assert!(limiter.window_len <= limiter.window.len(), "frame {frame}");
            assert_eq!(
                limiter.window[limiter.window_head].1, minimum,
                "frame {frame}"
            );
        }
    }

    #[test]
    fn test_limiter_reset_drops_the_delayed_audio() {
        let mut limiter = Limiter::new(RATE);
        let mut loud = sine(440.0, 3.0, 4_800);
        limiter.process(&mut loud);
        assert!(limiter.gain < 1.0);

        limiter.reset();
        let mut quiet = sine(440.0, 0.25, 4_800);
        let original = quiet.clone();
        limiter.process(&mut quiet);

        // 前の音声は出ず（無音の後に）、新しい音声がそのままの音量で出る
        assert!(quiet[..480].iter().all(|&sample| sample == 0.0));
        assert_eq!(&quiet[480..], &original[..original.len() - 480]);
    }
}
