//! 再生速度の変更（音程を保ったまま、速さだけを変える）
//!
//! 出力のコールバックの中で、リングバッファから取り出した音声（ステレオ）の長さを伸び縮みさせる
//! （時間伸縮）。方法はWSOLA（波形の似た位置で重ねてつなぐ）: 元の音声から短い区間（数十ミリ秒）を、
//! 速さに応じた間隔で切り出し、前の区間の終わりと波形が最もよく重なる位置を探して、重ねながら
//! つなぐ。音の高さは区間の中身のままで、区間を取る間隔だけが変わるため、音程は変わらない
//! （ADR-049）。
//!
//! - コールバックで使うため、メモリの確保は作る時だけ行う（`process`の中では確保しない）
//! - 1倍に戻すと、持っている音声をそのまま出し切って終わる（`is_idle`）。その後は、呼び出し側が
//!   時間伸縮を通さずに取り出す（1倍の間は、音声を一切変えない）
//! - 入力が足りない時（曲の終わり・バッファの補充待ち）も、持っている音声をそのまま出し切る

/// 選べる速さの範囲（倍）
pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 2.0;

/// 区間を重ねる長さ（ミリ秒）
const OVERLAP_MS: f64 = 8.0;

/// 区間の長さ（ミリ秒。速さが`MIN_SPEED`の時と`MAX_SPEED`の時。その間は直線で補う）
///
/// 遅くする時は長い区間の方が響きの繰り返しが目立たず、速くする時は短い区間の方が音の抜けが
/// 目立たない。
const SEQUENCE_MS: (f64, f64) = (90.0, 40.0);

/// 重なる位置を探す範囲（ミリ秒。区間の長さと同じく、速さで補う）
const SEEK_MS: (f64, f64) = (20.0, 15.0);

/// 重なる位置を探す時に比べるサンプルの、おおよそのレート（Hz）
///
/// 出力のレートのまま比べると、コールバックの中の計算が増える。波形の似具合は、間引いた
/// サンプルでも十分に測れる。
const SEARCH_RATE: u32 = 16_000;

/// 時間伸縮の入力（出力のコールバックでは、リングバッファ）
pub trait FrameSource {
    /// 今すぐ取り出せるフレーム数
    fn available(&self) -> usize;
    /// `out`（ステレオのインターリーブ）を埋める（`available`以下のフレーム数で呼ぶ）
    fn read(&mut self, out: &mut [f32]);
    /// `frames`フレームを読み捨てる（`available`以下で呼ぶ）
    fn skip(&mut self, frames: usize);
}

/// 速さに応じた、区間の長さと探す範囲（フレーム数）
struct Params {
    sequence: usize,
    seek: usize,
}

impl Params {
    fn new(speed: f32, sample_rate: u32, overlap: usize) -> Self {
        let t = f64::from((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).clamp(0.0, 1.0);
        let lerp = |(slow, fast): (f64, f64)| slow + (fast - slow) * t;
        Self {
            // 区間は、頭と終わりの重なりの間に、そのまま写す部分を持つ
            sequence: frames(lerp(SEQUENCE_MS), sample_rate).max(overlap * 2 + 1),
            seek: frames(lerp(SEEK_MS), sample_rate).max(1),
        }
    }

    /// 1つの区間を作るのに要る入力のフレーム数
    fn needed(&self) -> usize {
        self.seek + self.sequence
    }
}

fn frames(milliseconds: f64, sample_rate: u32) -> usize {
    (milliseconds * f64::from(sample_rate) / 1000.0).round() as usize
}

/// ステレオの音声の、時間伸縮
pub struct TimeStretcher {
    sample_rate: u32,
    /// 区間を重ねる長さ（フレーム数）
    overlap: usize,
    /// 重なる位置を探す時の、サンプルの間引き
    stride: usize,
    /// まだ使っていない入力
    input: Vec<f32>,
    /// 入力から読み捨てる残りのフレーム数（区間を取る間隔が、持っている入力より長かった分）
    skip_debt: usize,
    /// 区間を取る間隔の、端数の持ち越し
    skip_fraction: f64,
    /// 前の区間の終わり（次の区間の頭と重ねる。`has_tail`の時だけ有効）
    tail: Vec<f32>,
    has_tail: bool,
    /// `tail`と同じ音声が始まる、`input`の中の位置（フレーム。入力に残っていなければNone）
    continuation: Option<usize>,
    /// 作った出力
    output: Vec<f32>,
    /// `output`のうち、渡し終えたサンプル数
    output_offset: usize,
    /// `output`の1フレームが、元の音声の何フレームにあたるか
    output_speed: f32,
}

impl TimeStretcher {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let overlap = frames(OVERLAP_MS, sample_rate).max(1);
        // 持つ入力は、最も遅い速さの時が最大
        let slowest = Params::new(MIN_SPEED, sample_rate, overlap);
        let fastest = Params::new(MAX_SPEED, sample_rate, overlap);
        let capacity = slowest.needed().max(fastest.needed()) + overlap;
        Self {
            sample_rate,
            overlap,
            stride: (sample_rate / SEARCH_RATE).max(1) as usize,
            input: Vec::with_capacity(capacity * 2),
            skip_debt: 0,
            skip_fraction: 0.0,
            tail: vec![0.0; overlap * 2],
            has_tail: false,
            continuation: None,
            output: Vec::with_capacity(capacity * 2),
            output_offset: 0,
            output_speed: 1.0,
        }
    }

    /// 持っている音声がないか（入力・前の区間の終わり・渡していない出力のどれもない）
    pub fn is_idle(&self) -> bool {
        self.input.is_empty() && !self.has_tail && self.output_offset >= self.output.len()
    }

    /// 入力から取り出したが、まだ出力していない音声の長さ（元の音声のフレーム数）
    ///
    /// 取り出したフレーム数からこれを引くと、鳴っている位置になる。
    pub fn pending_source_frames(&self) -> u64 {
        let undelivered = (self.output.len() - self.output_offset) / 2;
        let output = (undelivered as f64 * f64::from(self.output_speed)).round() as u64;
        (self.input.len() / 2) as u64 + output
    }

    /// 持っている音声を捨てる（シーク・曲の切り替え）
    pub fn reset(&mut self) {
        self.input.clear();
        self.skip_debt = 0;
        self.skip_fraction = 0.0;
        self.has_tail = false;
        self.continuation = None;
        self.output.clear();
        self.output_offset = 0;
        self.output_speed = 1.0;
    }

    /// `out`（ステレオのインターリーブ）を、速さを変えた音声で埋める
    ///
    /// 要る分の入力を`source`から取り出す。入力が足りなければ、埋められた分だけで終わる。
    ///
    /// @returns 埋めたフレーム数
    pub fn process(&mut self, speed: f32, source: &mut impl FrameSource, out: &mut [f32]) -> usize {
        let speed = if speed.is_finite() {
            speed.clamp(MIN_SPEED, MAX_SPEED)
        } else {
            1.0
        };
        let wanted = out.len() / 2;
        let mut written = 0;
        loop {
            // 作ってある出力を渡す
            let ready = (self.output.len() - self.output_offset) / 2;
            let count = ready.min(wanted - written);
            out[written * 2..(written + count) * 2]
                .copy_from_slice(&self.output[self.output_offset..self.output_offset + count * 2]);
            self.output_offset += count * 2;
            written += count;
            if written == wanted || !self.produce(speed, source) {
                return written;
            }
        }
    }

    fn buffered(&self) -> usize {
        self.input.len() / 2
    }

    /// 次の出力を作る
    ///
    /// @returns 出力を作れたか（falseは、入力がなく作れない・時間伸縮を終えた）
    fn produce(&mut self, speed: f32, source: &mut impl FrameSource) -> bool {
        self.output.clear();
        self.output_offset = 0;

        // 1倍に戻った: 前の区間の続きが入力に残っていれば、そこからそのまま出して終える。
        // 残っていなければ（速くしていた直後）、もう1区間だけ位置を探してつなぐ
        if speed == 1.0 && (!self.has_tail || self.continuation.is_some()) {
            return self.drain();
        }

        let params = Params::new(speed, self.sample_rate, self.overlap);
        self.fill(source, params.needed());
        if self.buffered() < params.needed() {
            // 入力が足りない（曲の終わり・バッファの補充待ち）: 持っている音声をそのまま出す
            return self.drain();
        }
        self.sequence(speed, &params);
        true
    }

    /// 入力を、`needed`フレームになるまで`source`から取り出す
    fn fill(&mut self, source: &mut impl FrameSource, needed: usize) {
        if self.skip_debt > 0 {
            let count = self.skip_debt.min(source.available());
            source.skip(count);
            self.skip_debt -= count;
            if self.skip_debt > 0 {
                return;
            }
        }
        let buffered = self.buffered();
        if buffered >= needed {
            return;
        }
        let count = (needed - buffered).min(source.available());
        let start = self.input.len();
        // 容量の中に収まるため、確保は起きない
        self.input.resize(start + count * 2, 0.0);
        source.read(&mut self.input[start..]);
    }

    /// 入力から1つの区間を切り出し、前の区間と重ねて出力にする
    fn sequence(&mut self, speed: f32, params: &Params) {
        let overlap = self.overlap;
        let offset = if self.has_tail {
            self.best_offset(params.seek)
        } else {
            0
        };

        // 頭: 前の区間の終わりと重ねる（最初の区間は、そのまま）
        let head = &self.input[offset * 2..(offset + overlap) * 2];
        if self.has_tail {
            for (index, (tail, head)) in self
                .tail
                .as_chunks::<2>()
                .0
                .iter()
                .zip(head.as_chunks::<2>().0)
                .enumerate()
            {
                let weight = (index as f32 + 0.5) / overlap as f32;
                self.output
                    .push(tail[0] * (1.0 - weight) + head[0] * weight);
                self.output
                    .push(tail[1] * (1.0 - weight) + head[1] * weight);
            }
        } else {
            self.output.extend_from_slice(head);
        }
        // 中: そのまま写す
        let tail_start = offset + params.sequence - overlap;
        self.output
            .extend_from_slice(&self.input[(offset + overlap) * 2..tail_start * 2]);
        // 終わり: 次の区間と重ねるために取っておく
        self.tail
            .copy_from_slice(&self.input[tail_start * 2..(tail_start + overlap) * 2]);
        self.has_tail = true;
        self.output_speed = speed;

        // 入力を、速さに応じた間隔だけ進める
        let advance = f64::from(speed) * (params.sequence - overlap) as f64 + self.skip_fraction;
        let skip = advance.floor() as usize;
        self.skip_fraction = advance - skip as f64;
        self.continuation = tail_start.checked_sub(skip);
        let buffered = self.buffered();
        if skip >= buffered {
            self.input.clear();
            self.skip_debt = skip - buffered;
        } else {
            self.input.drain(..skip * 2);
        }
    }

    /// 前の区間の終わり（`tail`）と、波形が最もよく重なる入力の位置を、`seek`フレームの中から探す
    fn best_offset(&self, seek: usize) -> usize {
        let stride = self.stride;
        // 正規化した相互相関（入力の側の大きさで割る。`tail`の大きさは、どの位置でも同じ）
        let score = |offset: usize| {
            let mut correlation = 0.0f32;
            let mut energy = 0.0f32;
            for index in (0..self.overlap).step_by(stride) {
                let tail = &self.tail[index * 2..index * 2 + 2];
                let input = &self.input[(offset + index) * 2..(offset + index) * 2 + 2];
                correlation += tail[0] * input[0] + tail[1] * input[1];
                energy += input[0] * input[0] + input[1] * input[1];
            }
            correlation / (energy + 1e-9).sqrt()
        };

        // 粗く探してから、最もよかった位置のまわりを1フレームずつ探す
        let mut best = (0, score(0));
        for offset in (stride..seek).step_by(stride) {
            let value = score(offset);
            if value > best.1 {
                best = (offset, value);
            }
        }
        let coarse = best.0;
        for offset in coarse.saturating_sub(stride - 1)..(coarse + stride).min(seek) {
            let value = score(offset);
            if value > best.1 {
                best = (offset, value);
            }
        }
        best.0
    }

    /// 持っている音声を、速さを変えずにそのまま出力にして、時間伸縮を終える
    ///
    /// @returns 出力があるか
    fn drain(&mut self) -> bool {
        let overlap = self.overlap;
        let buffered = self.buffered();
        match (self.has_tail, self.continuation) {
            // 前の区間の続きが入力に残っている: そこから先を、そのまま出す
            (true, Some(start)) if start + overlap <= buffered => {
                self.output.extend_from_slice(&self.input[start * 2..]);
            }
            // 残っていない: 前の区間の終わりを、入力の頭と重ねてつなぐ（入力がなければ、消していく）
            (true, _) => {
                for (index, tail) in self.tail.as_chunks::<2>().0.iter().enumerate() {
                    let weight = (index as f32 + 0.5) / overlap as f32;
                    let head = self
                        .input
                        .get(index * 2..index * 2 + 2)
                        .map_or([0.0, 0.0], |frame| [frame[0], frame[1]]);
                    self.output
                        .push(tail[0] * (1.0 - weight) + head[0] * weight);
                    self.output
                        .push(tail[1] * (1.0 - weight) + head[1] * weight);
                }
                if buffered > overlap {
                    self.output.extend_from_slice(&self.input[overlap * 2..]);
                }
            }
            (false, _) => self.output.extend_from_slice(&self.input),
        }

        self.input.clear();
        self.skip_debt = 0;
        self.skip_fraction = 0.0;
        self.has_tail = false;
        self.continuation = None;
        self.output_speed = 1.0;
        !self.output.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// 決まった音声を入力にする（`limit`フレームまでしか取り出せないようにできる）
    pub(crate) struct VecSource {
        samples: Vec<f32>,
        position: usize,
        /// 取り出せるフレーム数の上限（位置。曲の途中でバッファが空になった状態を作る）
        limit: usize,
    }

    impl VecSource {
        pub(crate) fn new(samples: Vec<f32>) -> Self {
            let limit = samples.len() / 2;
            Self {
                samples,
                position: 0,
                limit,
            }
        }

        /// 取り出したフレーム数
        pub(crate) fn consumed(&self) -> usize {
            self.position
        }
    }

    impl FrameSource for VecSource {
        fn available(&self) -> usize {
            self.limit - self.position
        }

        fn read(&mut self, out: &mut [f32]) {
            let start = self.position * 2;
            out.copy_from_slice(&self.samples[start..start + out.len()]);
            self.position += out.len() / 2;
        }

        fn skip(&mut self, frames: usize) {
            self.position += frames;
        }
    }

    /// 左が正弦波、右がその逆の音声
    fn sine(frequency: f64, seconds: f64) -> Vec<f32> {
        let count = (seconds * f64::from(RATE)) as usize;
        (0..count)
            .flat_map(|frame| {
                let phase = frame as f64 * frequency / f64::from(RATE) * std::f64::consts::TAU;
                let value = (phase.sin() * 0.5) as f32;
                [value, -value]
            })
            .collect()
    }

    /// 入力がなくなるまで、`block`フレームずつ取り出す
    fn run(stretcher: &mut TimeStretcher, speed: f32, source: &mut VecSource) -> Vec<f32> {
        let mut result = Vec::new();
        let mut block = vec![0.0f32; 512 * 2];
        loop {
            let written = stretcher.process(speed, source, &mut block);
            result.extend_from_slice(&block[..written * 2]);
            if written < 512 {
                return result;
            }
        }
    }

    /// 左チャンネルの、ゼロを下から上へ横切る回数
    fn rising_crossings(samples: &[f32]) -> usize {
        let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
        left.windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    #[test]
    fn test_changes_the_length_but_not_the_pitch() {
        let input = sine(440.0, 3.0);
        for speed in [0.5, 0.75, 1.25, 1.5, 2.0] {
            let mut stretcher = TimeStretcher::new(RATE);
            let mut source = VecSource::new(input.clone());
            let output = run(&mut stretcher, speed, &mut source);
            let frames = output.len() / 2;

            // 長さは、速さの分だけ変わる（最後の1区間ほどの差は出る）
            let expected = (3.0 * f64::from(RATE) / f64::from(speed)) as usize;
            let tolerance = RATE as usize / 5;
            assert!(
                frames.abs_diff(expected) < tolerance,
                "speed {speed}: {frames} vs {expected}"
            );
            // 音の高さ（1秒あたりの波の数）は変わらない
            let seconds = frames as f64 / f64::from(RATE);
            let frequency = rising_crossings(&output) as f64 / seconds;
            assert!(
                (frequency - 440.0).abs() < 440.0 * 0.01,
                "speed {speed}: {frequency}Hz"
            );
            // つなぎ目で、波形が飛ばない（正弦波の、隣り合うサンプルの差の上限を大きく超えない）
            let max_step = 0.5 * std::f32::consts::TAU * 440.0 / RATE as f32;
            let worst = output
                .as_chunks::<2>()
                .0
                .windows(2)
                .map(|pair| (pair[1][0] - pair[0][0]).abs())
                .fold(0.0f32, f32::max);
            assert!(
                worst < max_step * 1.5,
                "speed {speed}: {worst} vs {max_step}"
            );
            // 音の大きさは、ほぼ変わらない
            let rms = (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
            assert!(
                (rms - 0.5 / 2f32.sqrt()).abs() < 0.02,
                "speed {speed}: {rms}"
            );
            // 左右は、同じ位置でつながれる
            assert!(
                output
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|frame| frame[0] == -frame[1]),
                "speed {speed}"
            );
            // 入力をすべて使い、持っている音声は残らない
            assert_eq!(source.consumed(), input.len() / 2, "speed {speed}");
            assert!(stretcher.is_idle(), "speed {speed}");
            assert_eq!(stretcher.pending_source_frames(), 0, "speed {speed}");
        }
    }

    /// 音楽に近い音（和音にノイズを足したもの）でも、長さが速さの分だけ変わり、音が途切れない
    #[test]
    fn test_stretches_a_mixed_signal_without_dropouts() {
        let count = RATE as usize * 3;
        let mut noise = 0x1234_5678u32;
        let input: Vec<f32> = (0..count)
            .flat_map(|frame| {
                let time = frame as f64 / f64::from(RATE);
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let hiss = f64::from(noise >> 8) / f64::from(1u32 << 24) - 0.5;
                let tone = (time * 220.0 * std::f64::consts::TAU).sin()
                    + (time * 277.2 * std::f64::consts::TAU).sin()
                    + (time * 329.6 * std::f64::consts::TAU).sin();
                let left = (tone * 0.15 + hiss * 0.05) as f32;
                let right = (tone * 0.1 - hiss * 0.05) as f32;
                [left, right]
            })
            .collect();
        let input_rms = (input.iter().map(|s| s * s).sum::<f32>() / input.len() as f32).sqrt();

        for speed in [0.5, 1.5, 2.0] {
            let mut stretcher = TimeStretcher::new(RATE);
            let mut source = VecSource::new(input.clone());
            let output = run(&mut stretcher, speed, &mut source);

            let expected = (count as f64 / f64::from(speed)) as usize;
            assert!(
                (output.len() / 2).abs_diff(expected) < RATE as usize / 5,
                "speed {speed}"
            );
            // 0.1秒ごとの音の大きさが、入力の大きさから大きく外れない（途切れ・重なりすぎがない）
            for (index, window) in output.chunks(RATE as usize / 10 * 2).enumerate() {
                if window.len() < RATE as usize / 10 * 2 {
                    continue;
                }
                let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
                assert!(
                    (rms / input_rms - 1.0).abs() < 0.25,
                    "speed {speed}, window {index}: {rms} vs {input_rms}"
                );
            }
        }
    }

    /// 1倍に戻すと、続きの音声をそのまま出し切って終わる（その後は、入力の続きから取り出せる）
    #[test]
    fn test_returns_to_the_input_at_normal_speed() {
        // どの位置か分かるよう、フレームの番号を値にする
        let input: Vec<f32> = (0..RATE * 2)
            .flat_map(|frame| [frame as f32, -(frame as f32)])
            .collect();
        for speed in [0.5, 2.0] {
            let mut stretcher = TimeStretcher::new(RATE);
            let mut source = VecSource::new(input.clone());
            let mut block = vec![0.0f32; 4_800 * 2];
            assert_eq!(stretcher.process(speed, &mut source, &mut block), 4_800);
            assert!(!stretcher.is_idle());

            // 1倍にして、持っている音声がなくなるまで取り出す
            let mut rest = Vec::new();
            while !stretcher.is_idle() {
                let written = stretcher.process(1.0, &mut source, &mut block[..256]);
                rest.extend_from_slice(&block[..written * 2]);
                assert!(written > 0, "speed {speed}");
            }
            // 最後は、取り出した入力の終わりまでが、そのままの値で並ぶ
            let consumed = source.consumed();
            let last = rest.as_chunks::<2>().0.last().unwrap();
            assert_eq!(last[0], (consumed - 1) as f32, "speed {speed}");
            let tail = &rest[rest.len() - 200..];
            assert!(
                tail.as_chunks::<2>()
                    .0
                    .windows(2)
                    .all(|pair| pair[1][0] - pair[0][0] == 1.0),
                "speed {speed}"
            );
            assert_eq!(stretcher.pending_source_frames(), 0, "speed {speed}");
        }
    }

    /// 入力が途中で足りなくなったら、持っている音声を出し切る。入力が届けば、続きから作る
    #[test]
    fn test_drains_when_the_input_runs_short() {
        let input = sine(440.0, 2.0);
        let total = input.len() / 2;
        let mut stretcher = TimeStretcher::new(RATE);
        let mut source = VecSource::new(input);
        source.limit = RATE as usize / 2;

        let mut block = vec![0.0f32; RATE as usize * 2];
        let first = stretcher.process(2.0, &mut source, &mut block);
        // 0.5秒分の入力を、すべて使って出し切る（2倍のため、出力はおよそ半分）
        assert_eq!(source.consumed(), RATE as usize / 2);
        assert!(stretcher.is_idle());
        assert!(first.abs_diff(RATE as usize / 4) < RATE as usize / 16);
        assert_eq!(stretcher.process(2.0, &mut source, &mut block), 0);

        // 入力が届いたら、続きから
        source.limit = total;
        let second = stretcher.process(2.0, &mut source, &mut block);
        assert!(second > RATE as usize / 2);
    }

    #[test]
    fn test_reset_drops_buffered_audio() {
        let mut stretcher = TimeStretcher::new(RATE);
        let mut source = VecSource::new(sine(440.0, 1.0));
        let mut block = vec![0.0f32; 512 * 2];
        stretcher.process(1.5, &mut source, &mut block);
        assert!(!stretcher.is_idle());
        assert!(stretcher.pending_source_frames() > 0);

        stretcher.reset();
        assert!(stretcher.is_idle());
        assert_eq!(stretcher.pending_source_frames(), 0);
    }

    /// 取り出したフレーム数から、出していない分を引いた位置は、戻らずに進む
    #[test]
    fn test_pending_frames_keep_the_position_monotonic() {
        for speed in [0.5, 0.8, 1.7, 2.0] {
            let mut stretcher = TimeStretcher::new(RATE);
            let mut source = VecSource::new(sine(330.0, 2.0));
            let mut block = vec![0.0f32; 128 * 2];
            let mut previous = 0i64;
            let mut blocks = 0;
            while stretcher.process(speed, &mut source, &mut block) == 128 {
                // 最初の区間では、出していない分の方が多い（呼び出し側と同じく、0より前にはしない）
                let position =
                    (source.consumed() as i64 - stretcher.pending_source_frames() as i64).max(0);
                // 丸めの分（1フレーム）を除いて、戻らない
                assert!(
                    position >= previous - 1,
                    "speed {speed}: {position} < {previous}"
                );
                // 1回に進むのは、出した分（128フレーム × 速さ）と、持っている入力の長さまで
                // （入力の終わりで出し切る時は、遅くするために繰り返す分を飛ばす）
                assert!(
                    position - previous <= (128.0 * speed) as i64 + RATE as i64 / 8,
                    "speed {speed}: {previous} -> {position}"
                );
                previous = position;
                blocks += 1;
            }
            assert!(blocks > 100, "speed {speed}");
        }
    }

    /// コールバックの中でメモリを確保しない（作った時の容量のまま使う）
    #[test]
    fn test_process_does_not_grow_the_buffers() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let mut stretcher = TimeStretcher::new(rate);
            let capacities = (stretcher.input.capacity(), stretcher.output.capacity());
            let samples: Vec<f32> = (0..rate * 2)
                .flat_map(|frame| {
                    let value = (frame as f32 * 0.05).sin() * 0.3;
                    [value, value]
                })
                .collect();
            let mut source = VecSource::new(samples);
            let mut block = vec![0.0f32; 1024 * 2];
            // 速さを変えながら（区間の長さが変わる）、終わりまで
            for speed in [0.5, 2.0, 0.5, 1.0, 1.3, 0.7].into_iter().cycle() {
                if source.available() == 0 && stretcher.is_idle() {
                    break;
                }
                stretcher.process(speed, &mut source, &mut block);
            }
            assert_eq!(
                (stretcher.input.capacity(), stretcher.output.capacity()),
                capacities,
                "{rate}Hz"
            );
        }
    }

    /// 範囲の外の速さ・数値でない速さは、範囲の中に収める
    #[test]
    fn test_clamps_the_speed() {
        let input = sine(440.0, 2.0);
        let length = |speed: f32| {
            let mut stretcher = TimeStretcher::new(RATE);
            let mut source = VecSource::new(input.clone());
            run(&mut stretcher, speed, &mut source).len() / 2
        };
        assert_eq!(length(9.0), length(MAX_SPEED));
        assert_eq!(length(0.01), length(MIN_SPEED));
        // 数値でない値は1倍として扱う（1倍の間は、入力を取り出さない。呼び出し側がそのまま取り出す）
        assert_eq!(length(f32::NAN), 0);
        assert_eq!(length(1.0), 0);
    }
}
