//! 再生エンジン本体
//!
//! エンジンは1つのスレッドで動き、コマンド（再生・一時停止・シークなど）を順に処理しながら、
//! デコードした音声を出力のリングバッファへ書き続ける。
//!
//! ```text
//! コマンド → [エンジンのスレッド] デコード → 変換（ステレオ・出力のレート） → リングバッファ
//!                                                                              ↓
//!                                             [出力のコールバック] 音量・一時停止 → デバイス
//! ```
//!
//! 再生キューはフロントエンドが持つ。エンジンが持つのは「再生中の曲」と「続けて再生する曲」
//! （`set_next`）だけで、再生中の曲のデコードが終わると、続けて再生する曲の音声をそのまま
//! 続けて書く（ギャップレス再生）。続けて再生する曲がなければ、鳴り終わった時点で`Ended`を送る。
//!
//! リングバッファには約0.5秒分の音声がたまっているため、「デコードしている位置」と
//! 「鳴っている位置」はずれる。鳴っている位置は、出力のコールバックが取り出したフレーム数
//! （`RenderShared::frames_read`）と、曲ごとの書き始めの位置（`Segment`）から求める。
//!
//! 音量の正規化（曲ごとの倍率）とクロスフェード（前の曲の終わりと次の曲の頭を重ねる）は、
//! リングバッファへ書く前にここでかける。イコライザとリミッターは、出力のコールバックでかける
//! （`effects`）。

use super::convert::{Converter, OUTPUT_CHANNELS};
use super::decoder::TrackDecoder;
use super::effects::{EQ_BANDS, EffectParams};
use super::normalization::normalization_gain;
use super::output::{CpalBackend, OutputBackend, OutputEvent, OutputHandle};
use super::render::{RenderShared, Renderer};
use crate::error::{AppError, AppResult};
use crate::events::PlaybackEvent;
use crate::models::ReplayGain;
use crate::settings::{MAX_CROSSFADE_SECONDS, Settings, VolumeNormalization};
use serde::Serialize;
use specta::Type;
use std::collections::VecDeque;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

/// リングバッファにためる音声の長さ（秒）
///
/// 長いほどデコードの遅れ（ディスクの待ちなど）に強いが、続けて再生する曲の変更が
/// 間に合わなくなる時間も長くなる。音量・一時停止・シークは、たまっている分を待たずに効く。
const BUFFER_SECONDS: f64 = 0.5;

/// 再生中に、リングバッファの補充と再生位置の確認をする間隔
const TICK: Duration = Duration::from_millis(10);

/// 再生位置を通知する間隔
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

/// 一時停止・再生の終了から、出力を止めるまでの時間（音量を下げきる・最後の音を鳴らしきるのを待つ）
const OUTPUT_STOP_DELAY: Duration = Duration::from_millis(200);

/// 何も再生しなくなってから、出力を閉じるまでの時間（すぐ次の曲を再生する場合に、開き直さない）
const OUTPUT_CLOSE_DELAY: Duration = Duration::from_secs(3);

/// これより短い時間しか残っていなければ、クロスフェードせずに切れ目なく続ける（秒）
const MIN_CROSSFADE_SECONDS: f64 = 0.05;

/// 設定のうち、再生エンジンが使うもの
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackOptions {
    /// 出力デバイスのID（Noneは、OSの既定のデバイス）
    pub output_device_id: Option<String>,
    /// 音量の正規化
    pub normalization: VolumeNormalization,
    /// クロスフェードの秒数（0でクロスフェードしない）
    pub crossfade_seconds: u8,
}

impl From<&Settings> for PlaybackOptions {
    fn from(settings: &Settings) -> Self {
        Self {
            output_device_id: settings.output_device_id.clone(),
            normalization: settings.volume_normalization,
            crossfade_seconds: settings.crossfade_seconds,
        }
    }
}

/// 再生する曲
#[derive(Debug, Clone)]
pub struct PlayRequest {
    /// フロントエンドが曲ごとに振る番号（イベントがどの再生のものかを区別する）
    ///
    /// 新しい要求ほど大きい番号を振る。コマンドは別々のスレッドから届くため、続けて出した
    /// 再生の要求が逆の順で届くことがある。その場合に、古い要求で新しい再生を止めないようにする。
    pub token: u32,
    /// 音声ファイルのパス
    pub path: PathBuf,
    /// 音量の正規化に使うゲインとピーク（タグから読んだもの）
    pub replay_gain: ReplayGain,
}

/// 再生を始めた曲の情報
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackTrackInfo {
    /// 曲の長さ（秒。ファイルから分からない場合はnull）
    pub duration: Option<f64>,
}

/// エンジンのスレッドへ送るコマンド
enum Command {
    Play {
        request: PlayRequest,
        reply: SyncSender<AppResult<PlaybackTrackInfo>>,
    },
    SetNext {
        request: Option<PlayRequest>,
        reply: SyncSender<AppResult<()>>,
    },
    Pause,
    Resume,
    Seek(f64),
    SetVolume(f32),
    Stop,
    SetOutputDevice(Option<String>),
    SetNormalization(VolumeNormalization),
    SetCrossfade(u8),
    /// 出力からの通知（`generation`は、通知した出力を開いたときの番号）
    Output {
        generation: u64,
        event: OutputEvent,
    },
    Shutdown,
}

/// 再生エンジン（エンジンのスレッドへコマンドを送る窓口）
pub struct PlaybackEngine {
    commands: Sender<Command>,
    /// イコライザの設定（出力のコールバックが直接読む）
    effects: Arc<EffectParams>,
}

impl PlaybackEngine {
    /// エンジンのスレッドを始める（出力は、最初に再生するときに開く）
    ///
    /// - `options`: 設定のうち、再生エンジンが使うもの
    /// - `emit`: エンジンからの通知（再生位置・曲の切り替わりなど）を受け取る
    pub fn start(options: PlaybackOptions, emit: impl Fn(PlaybackEvent) + Send + 'static) -> Self {
        Self::start_with_backend(Box::new(CpalBackend), options, Box::new(emit))
    }

    fn start_with_backend(
        backend: Box<dyn OutputBackend>,
        options: PlaybackOptions,
        emit: Box<dyn Fn(PlaybackEvent) + Send>,
    ) -> Self {
        let (commands, receiver) = mpsc::channel();
        let effects = Arc::new(EffectParams::default());
        let engine_commands = commands.clone();
        let engine_effects = effects.clone();
        let spawned = thread::Builder::new()
            .name("playback-engine".to_string())
            .spawn(move || {
                Engine::new(backend, options, engine_effects, emit, engine_commands).run(receiver)
            });
        if let Err(e) = spawned {
            // スレッドがなければコマンドは届かず、各操作が「停止しています」のエラーを返す
            log::error!("再生エンジンのスレッドを開始できません: {}", e);
        }
        Self { commands, effects }
    }

    /// 曲を頭から再生する（再生中の曲は止める）
    pub fn play(&self, request: PlayRequest) -> AppResult<PlaybackTrackInfo> {
        self.request(|reply| Command::Play { request, reply })
    }

    /// 再生中の曲に続けて再生する曲を用意する（Noneで取り消す）
    pub fn set_next(&self, request: Option<PlayRequest>) -> AppResult<()> {
        self.request(|reply| Command::SetNext { request, reply })
    }

    pub fn pause(&self) -> AppResult<()> {
        self.send(Command::Pause)
    }

    pub fn resume(&self) -> AppResult<()> {
        self.send(Command::Resume)
    }

    /// 再生中の曲の、指定した位置（秒）へ移動する
    pub fn seek(&self, seconds: f64) -> AppResult<()> {
        self.send(Command::Seek(seconds))
    }

    /// 音量（0.0〜1.0）を設定する
    pub fn set_volume(&self, volume: f32) -> AppResult<()> {
        self.send(Command::SetVolume(volume))
    }

    /// 再生を止める
    pub fn stop(&self) -> AppResult<()> {
        self.send(Command::Stop)
    }

    /// 出力デバイスを変える（Noneは、OSの既定のデバイス）。再生中なら、同じ位置から続ける
    #[cfg(test)]
    pub fn set_output_device(&self, device_id: Option<String>) -> AppResult<()> {
        self.send(Command::SetOutputDevice(device_id))
    }

    /// 設定の変更を反映する（出力デバイス・音量の正規化・クロスフェード）
    ///
    /// 出力デバイスが変わっていれば、再生中の曲を同じ位置から続ける。音量の正規化は、
    /// 再生中の曲にもすぐに（バッファにたまっている分の後から）効く。
    pub fn apply_options(&self, options: PlaybackOptions) -> AppResult<()> {
        self.send(Command::SetOutputDevice(options.output_device_id))?;
        self.send(Command::SetNormalization(options.normalization))?;
        self.send(Command::SetCrossfade(options.crossfade_seconds))
    }

    /// イコライザの設定を変える（`gains`は、バンドごとのゲイン（dB）。すぐに効く）
    pub fn set_equalizer(&self, enabled: bool, gains: &[f32; EQ_BANDS]) {
        self.effects.set_equalizer(enabled, gains);
    }

    fn send(&self, command: Command) -> AppResult<()> {
        self.commands.send(command).map_err(|_| engine_stopped())
    }

    /// コマンドを送り、エンジンのスレッドでの処理の結果を待つ
    fn request<T>(
        &self,
        command: impl FnOnce(SyncSender<AppResult<T>>) -> Command,
    ) -> AppResult<T> {
        let (reply, result) = mpsc::sync_channel(1);
        self.send(command(reply))?;
        result.recv().map_err(|_| engine_stopped())?
    }
}

impl Drop for PlaybackEngine {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

fn engine_stopped() -> AppError {
    AppError::Playback("再生エンジンが停止しています".to_string())
}

/// 開いているデコーダーと、その曲
struct Deck {
    token: u32,
    path: PathBuf,
    decoder: TrackDecoder,
    replay_gain: ReplayGain,
    /// 音量の正規化の倍率（設定と`replay_gain`から決まる）
    gain: f32,
    /// 最後に書いた音声にかけた倍率（`gain`が変わった直後は、ここから滑らかに変える）
    applied_gain: f32,
}

/// クロスフェードの途中の状態
///
/// `current`（前の曲）の残りと、`incoming`（次の曲）の頭を、それぞれデコードして重ねる。
/// 前の曲が終わったら、`incoming`が`current`になる。
struct Fade {
    incoming: Deck,
    /// 次の曲の変換器（前の曲の変換器は、`Engine::converter`のまま使う）
    converter: Converter,
    /// 変換済みで、まだ重ねていない音声
    outgoing_samples: Vec<f32>,
    incoming_samples: Vec<f32>,
    outgoing_finished: bool,
    incoming_finished: bool,
    /// 重ね終えたフレーム数と、重ねる長さ（フレーム）
    mixed_frames: u64,
    total_frames: u64,
}

/// 重ね始めてから`frame`フレーム目の、前の曲・次の曲にかける音量（等パワー）
///
/// 2つの曲を重ねたときに合計の大きさが途中で下がらないよう、前の曲はcos、次の曲はsinで
/// 変える（2乗の和が常に1になる）。`total_frames`は、重ねる長さ。
fn crossfade_gains(frame: u64, total_frames: u64) -> (f32, f32) {
    let progress = (frame as f32 + 0.5) / total_frames as f32;
    let angle = progress.min(1.0) * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// リングバッファへ書いた音声のうち、1つの曲の範囲
#[derive(Debug, Clone)]
struct Segment {
    token: u32,
    path: PathBuf,
    replay_gain: ReplayGain,
    /// この曲の音声を書き始めた位置（リングバッファへ書いたフレーム数の合計）
    start_frame: u64,
    /// 書き始めた位置の、曲の中での位置（秒。シークした場合は0以外）
    base_seconds: f64,
    duration: Option<f64>,
}

/// 開いている出力
struct Output {
    handle: Box<dyn OutputHandle>,
    producer: rtrb::Producer<f32>,
    shared: Arc<RenderShared>,
    sample_rate: u32,
    /// 出力が動いているか（コールバックが呼ばれているか）
    running: bool,
    /// リングバッファへ書いたフレーム数の合計
    pushed_frames: u64,
    /// 音声を捨てさせた位置（シーク・曲の切り替えのたびに、その時点の`pushed_frames`にする）
    discarded_until: u64,
    /// 捨てさせた音声が読み捨てられた後、リングバッファがいっぱいになるまで書けたか
    /// （再生の開始・シークの直後はfalse）
    filled: bool,
    /// ログに出した音切れの回数
    reported_underruns: u64,
}

impl Output {
    /// リングバッファにたまっている音声を捨てさせる（シーク・曲の切り替え）
    fn discard_buffered(&mut self) {
        self.shared.discard_until(self.pushed_frames);
        self.discarded_until = self.pushed_frames;
        self.filled = false;
    }
}

/// エンジンのスレッドの状態
struct Engine {
    backend: Box<dyn OutputBackend>,
    emit: Box<dyn Fn(PlaybackEvent) + Send>,
    /// 出力からの通知を、自分自身へコマンドとして送る
    commands: Sender<Command>,
    device_id: Option<String>,
    volume: f32,
    paused: bool,
    output: Option<Output>,
    /// 出力を開くたびに増やす番号（閉じた出力からの遅れた通知を無視する）
    output_generation: u64,
    /// デコードしている曲（鳴っている曲か、その続きの曲）
    current: Option<Deck>,
    /// `current`のデコードが終わり、続けて再生する曲もない（鳴り終わるのを待っている）
    decode_finished: bool,
    /// 続けて再生する曲（デコーダーを開いて、頭から読める状態で待つ）
    next: Option<Deck>,
    /// クロスフェードの途中なら、その状態
    fade: Option<Fade>,
    /// `current`の変換器
    converter: Option<Converter>,
    /// 変換した音声のうち、リングバッファへまだ書いていない分（`pending_offset`から後ろ）
    pending: Vec<f32>,
    pending_offset: usize,
    /// リングバッファに音声が残っている曲（先頭が、鳴っている曲）
    segments: VecDeque<Segment>,
    /// 音声の終わりの位置（続けて再生する曲がない場合。ここまで鳴ったら`Ended`）
    end_frame: Option<u64>,
    last_position_at: Instant,
    /// 再生していない状態（一時停止・何も再生していない）になった時刻
    idle_since: Option<Instant>,
    /// これまでに受け取った再生の要求のうち、最も新しいものの番号
    latest_play_token: Option<u32>,
    /// イコライザの設定（開いた出力の`Renderer`へ渡す）
    effects: Arc<EffectParams>,
    normalization: VolumeNormalization,
    /// クロスフェードの秒数（0でクロスフェードしない）
    crossfade_seconds: f64,
}

impl Engine {
    fn new(
        backend: Box<dyn OutputBackend>,
        options: PlaybackOptions,
        effects: Arc<EffectParams>,
        emit: Box<dyn Fn(PlaybackEvent) + Send>,
        commands: Sender<Command>,
    ) -> Self {
        Self {
            backend,
            emit,
            commands,
            device_id: options.output_device_id,
            effects,
            normalization: options.normalization,
            crossfade_seconds: f64::from(options.crossfade_seconds.min(MAX_CROSSFADE_SECONDS)),
            fade: None,
            volume: 1.0,
            paused: false,
            output: None,
            output_generation: 0,
            current: None,
            decode_finished: false,
            next: None,
            converter: None,
            pending: Vec::new(),
            pending_offset: 0,
            segments: VecDeque::new(),
            end_frame: None,
            last_position_at: Instant::now(),
            idle_since: None,
            latest_play_token: None,
        }
    }

    fn run(mut self, commands: Receiver<Command>) {
        loop {
            let command = match self.wait_duration() {
                Some(timeout) => match commands.recv_timeout(timeout) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                },
                None => match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                },
            };
            if let Some(command) = command {
                if !self.handle(command) {
                    return;
                }
                // たまっているコマンドを先に片付ける（シークバーのドラッグなど）
                while let Ok(command) = commands.try_recv() {
                    if !self.handle(command) {
                        return;
                    }
                }
            }
            self.tick();
        }
    }

    /// 次にコマンドを待つ時間（Noneは、コマンドが届くまで待つ）
    fn wait_duration(&self) -> Option<Duration> {
        let Some(idle_since) = self.idle_since else {
            return Some(TICK);
        };
        let output = self.output.as_ref()?;
        let deadline = if output.running {
            OUTPUT_STOP_DELAY
        } else if self.segments.is_empty() {
            OUTPUT_CLOSE_DELAY
        } else {
            return None;
        };
        Some(deadline.saturating_sub(idle_since.elapsed()))
    }

    /// コマンドを処理する（falseを返したら、スレッドを終える）
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Play { request, reply } => {
                let result = self.play(request);
                if let Err(e) = &result {
                    log::error!("再生を開始できません: {}", e);
                }
                let _ = reply.send(result);
            }
            Command::SetNext { request, reply } => {
                let _ = reply.send(self.set_next(request));
            }
            Command::Pause => {
                if !self.segments.is_empty() && !self.paused {
                    self.set_paused(true);
                    self.idle_since = Some(Instant::now());
                    self.report_position(true);
                }
            }
            Command::Resume => {
                if !self.segments.is_empty() && self.paused {
                    self.set_paused(false);
                    self.idle_since = None;
                    self.start_output();
                }
            }
            Command::Seek(seconds) => {
                if !self.segments.is_empty() {
                    match self.reposition(seconds) {
                        Ok(()) => self.report_position(true),
                        Err(e) => self.fail(e),
                    }
                }
            }
            Command::SetVolume(volume) => {
                self.volume = volume.clamp(0.0, 1.0);
                if let Some(output) = &self.output {
                    output.shared.set_volume(self.volume);
                }
            }
            Command::Stop => self.stop_playback(),
            Command::SetOutputDevice(device_id) => {
                if self.device_id != device_id {
                    self.device_id = device_id;
                    self.reopen_output();
                }
            }
            Command::SetNormalization(mode) => {
                self.normalization = mode;
                // 開いている曲の倍率を決め直す（次に書く音声から、滑らかに変わる）
                let decks = [
                    self.current.as_mut(),
                    self.next.as_mut(),
                    self.fade.as_mut().map(|fade| &mut fade.incoming),
                ];
                for deck in decks.into_iter().flatten() {
                    deck.gain = normalization_gain(&deck.replay_gain, mode);
                }
            }
            Command::SetCrossfade(seconds) => {
                self.crossfade_seconds = f64::from(seconds.min(MAX_CROSSFADE_SECONDS));
            }
            Command::Output { generation, event } => {
                if generation == self.output_generation {
                    match event {
                        OutputEvent::DeviceChanged => self.reopen_output(),
                    }
                }
            }
            Command::Shutdown => return false,
        }
        true
    }

    /// 定期的な処理: リングバッファの補充、鳴っている曲の確認、再生位置の通知、使っていない出力の停止
    fn tick(&mut self) {
        self.pump();
        self.log_underruns();
        self.update_audible();
        self.report_position(false);
        self.release_idle_output();
    }

    // ---------- コマンド ----------

    fn play(&mut self, request: PlayRequest) -> AppResult<PlaybackTrackInfo> {
        if self
            .latest_play_token
            .is_some_and(|latest| request.token < latest)
        {
            return Err(superseded());
        }
        self.latest_play_token = Some(request.token);

        // 続けて再生する曲として用意していた曲なら、開いてあるデコーダーをそのまま使う
        let prepared = self.next.take().filter(|next| next.path == request.path);
        self.stop_playback();

        let decoder = match prepared {
            Some(deck) => deck.decoder,
            None => TrackDecoder::open(&request.path)?,
        };
        let deck = Deck::new(request, decoder, self.normalization);
        if self.output.is_none() {
            self.open_output()?;
        }
        let output = self.output.as_ref().ok_or_else(output_closed)?;
        let converter = Converter::new(
            deck.decoder.sample_rate(),
            deck.decoder.channels(),
            output.sample_rate,
        )?;
        let info = PlaybackTrackInfo {
            duration: deck.decoder.duration_seconds(),
        };

        self.segments
            .push_back(deck.segment(output.pushed_frames, 0.0));
        self.current = Some(deck);
        self.converter = Some(converter);
        self.set_paused(false);
        self.idle_since = None;
        // 出力を動かす前に、鳴らす音声を書いておく
        self.pump();
        self.start_output();
        if self.segments.is_empty() {
            // 出力を動かせなかった（`start_output`が通知して再生を止めた）
            return Err(AppError::Playback("音声の出力を開始できません".to_string()));
        }
        Ok(info)
    }

    fn set_next(&mut self, request: Option<PlayRequest>) -> AppResult<()> {
        let Some(request) = request else {
            self.next = None;
            return Ok(());
        };
        if self.segments.is_empty()
            || self
                .latest_play_token
                .is_some_and(|latest| request.token < latest)
        {
            // 何も再生していない（再生の開始に失敗した直後など）か、前の再生についての遅れた要求
            return Ok(());
        }
        // 同じ曲をすでに用意していれば、番号だけを付け替える
        if let Some(next) = self.next.as_mut().filter(|next| next.path == request.path) {
            next.token = request.token;
            return Ok(());
        }
        self.next = None;
        let decoder = TrackDecoder::open(&request.path)?;
        let deck = Deck::new(request, decoder, self.normalization);
        if self.decode_finished {
            // 再生中の曲のデコードは終わっている（鳴り終わるのを待っている）ため、すぐに続ける
            self.start_next(deck);
        } else {
            self.next = Some(deck);
        }
        Ok(())
    }

    /// 再生を止め、再生中の曲・続けて再生する曲を手放す（出力は、しばらく開いたままにする）
    fn stop_playback(&mut self) {
        if let Some(output) = &mut self.output {
            output.discard_buffered();
        }
        self.current = None;
        self.next = None;
        self.fade = None;
        self.converter = None;
        self.decode_finished = false;
        self.pending.clear();
        self.pending_offset = 0;
        self.segments.clear();
        self.end_frame = None;
        self.set_paused(false);
        self.idle_since = Some(Instant::now());
    }

    /// 鳴っている曲を再生できなくなった: 再生を止め、フロントエンドへ知らせる
    fn fail(&mut self, error: AppError) {
        let token = self.segments.front().map(|segment| segment.token);
        log::error!("再生を続けられません: {}", error);
        self.stop_playback();
        if let Some(token) = token {
            (self.emit)(PlaybackEvent::Failed { token, error });
        }
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(output) = &self.output {
            output.shared.set_paused(paused);
        }
    }

    // ---------- デコードとリングバッファへの書き込み ----------

    /// リングバッファが埋まるまで、デコードした音声を書く
    fn pump(&mut self) {
        loop {
            let Some(output) = &mut self.output else {
                return;
            };
            // 変換済みの音声を書く（フレームの途中で切らないよう、偶数個ずつ）
            if self.pending_offset < self.pending.len() {
                let slots = output.producer.slots() / OUTPUT_CHANNELS * OUTPUT_CHANNELS;
                let count = slots.min(self.pending.len() - self.pending_offset);
                let chunk = &self.pending[self.pending_offset..self.pending_offset + count];
                let (written, _) = output.producer.push_partial_slice(chunk);
                output.pushed_frames += (written.len() / OUTPUT_CHANNELS) as u64;
                self.pending_offset += written.len();
                if self.pending_offset < self.pending.len() {
                    // リングバッファがいっぱい（捨てさせた古い音声で埋まっている間は、数えない）
                    if !output.filled && output.shared.frames_read() >= output.discarded_until {
                        output.filled = true;
                        output.reported_underruns = output.shared.underruns();
                    }
                    return;
                }
            }
            self.pending.clear();
            self.pending_offset = 0;

            if self.decode_finished {
                return;
            }
            if self.fade.is_some() {
                if let Err(e) = self.pump_fade() {
                    self.fail(e);
                    return;
                }
                continue;
            }
            // クロスフェードを始める位置に来ていれば始め、手前なら、その位置でかたまりを区切る
            let limit = match self.crossfade_plan() {
                Some((seconds, 0)) => {
                    self.begin_fade(seconds);
                    continue;
                }
                Some((_, frames_until_start)) => Some(frames_until_start),
                None => None,
            };
            let (Some(deck), Some(converter)) = (&mut self.current, &mut self.converter) else {
                return;
            };
            let result = match deck.decoder.next_chunk_limited(limit) {
                Ok(Some(samples)) => {
                    let start = self.pending.len();
                    let result = converter.process(samples, &mut self.pending);
                    apply_gain(
                        &mut self.pending[start..],
                        &mut deck.applied_gain,
                        deck.gain,
                    );
                    result
                }
                Ok(None) => {
                    self.finish_current_decode();
                    Ok(())
                }
                Err(e) => Err(e),
            };
            if let Err(e) = result {
                self.fail(e);
                return;
            }
        }
    }

    // ---------- クロスフェード ----------

    /// 次の曲へクロスフェードする場合に、重ねる長さ（秒）と、始めるまでに書く前の曲のフレーム数
    /// （前の曲のサンプルレートでの数。0なら、今から始める）を返す
    ///
    /// 重ねる長さは、設定の秒数を上限に、どちらの曲も長さの半分まで、かつ前の曲の残りまで。
    /// 同じ曲の繰り返し（1曲リピート）と、長さの分からない曲は、クロスフェードせずに切れ目なく続ける。
    fn crossfade_plan(&self) -> Option<(f64, u64)> {
        if self.crossfade_seconds <= 0.0 {
            return None;
        }
        let (current, next) = (self.current.as_ref()?, self.next.as_ref()?);
        if current.path == next.path {
            return None;
        }
        let duration = current.decoder.duration_seconds()?;
        let remaining = current.decoder.remaining_seconds()?;
        let mut length = self.crossfade_seconds.min(duration / 2.0);
        if let Some(next_duration) = next.decoder.duration_seconds() {
            length = length.min(next_duration / 2.0);
        }
        if remaining <= MIN_CROSSFADE_SECONDS {
            // 続けて再生する曲が届くのが遅かった（残りがほとんどない）: 切れ目なく続ける
            return None;
        }
        let frames_until_start =
            ((remaining - length).max(0.0) * f64::from(current.decoder.sample_rate())).round();
        Some((remaining.min(length), frames_until_start as u64))
    }

    /// 続けて再生する曲を、前の曲の残り（`seconds`秒）に重ねて書き始める
    fn begin_fade(&mut self, seconds: f64) {
        let Some(output_rate) = self.output.as_ref().map(|output| output.sample_rate) else {
            return;
        };
        let Some(incoming) = self.next.take() else {
            return;
        };
        let converter = match Converter::new(
            incoming.decoder.sample_rate(),
            incoming.decoder.channels(),
            output_rate,
        ) {
            Ok(converter) => converter,
            Err(e) => {
                // 続けて再生できない曲: 続きがないものとして再生中の曲を終える
                // （フロントエンドが次の曲を通常の手順で再生し、そこでエラーを通知する）
                log::warn!("続けて再生する曲を準備できません: {}", e);
                return;
            }
        };
        // 重なりの始まりが、次の曲の始まり（ここが鳴った時点で、曲が切り替わったと通知する）
        let start_frame = self.write_position();
        self.segments.push_back(incoming.segment(start_frame, 0.0));
        self.fade = Some(Fade {
            incoming,
            converter,
            outgoing_samples: Vec::new(),
            incoming_samples: Vec::new(),
            outgoing_finished: false,
            incoming_finished: false,
            mixed_frames: 0,
            total_frames: ((seconds * f64::from(output_rate)).round() as u64).max(1),
        });
    }

    /// クロスフェードの続きを書く: 前の曲の残りと次の曲の頭をデコードし、重ねて`pending`へ足す
    fn pump_fade(&mut self) -> AppResult<()> {
        let (Some(fade), Some(outgoing), Some(converter)) =
            (&mut self.fade, &mut self.current, &mut self.converter)
        else {
            return Ok(());
        };

        // 短い方の音声を、1かたまり分デコードする
        if !fade.outgoing_finished && fade.outgoing_samples.len() <= fade.incoming_samples.len() {
            let start = fade.outgoing_samples.len();
            match outgoing.decoder.next_chunk()? {
                Some(samples) => converter.process(samples, &mut fade.outgoing_samples)?,
                None => {
                    converter.flush(&mut fade.outgoing_samples)?;
                    fade.outgoing_finished = true;
                }
            }
            apply_gain(
                &mut fade.outgoing_samples[start..],
                &mut outgoing.applied_gain,
                outgoing.gain,
            );
        } else if !fade.incoming_finished {
            let start = fade.incoming_samples.len();
            match fade.incoming.decoder.next_chunk()? {
                Some(samples) => fade
                    .converter
                    .process(samples, &mut fade.incoming_samples)?,
                None => fade.incoming_finished = true,
            }
            let incoming = &mut fade.incoming;
            apply_gain(
                &mut fade.incoming_samples[start..],
                &mut incoming.applied_gain,
                incoming.gain,
            );
        }
        if fade.incoming_finished && fade.incoming_samples.len() < fade.outgoing_samples.len() {
            // 次の曲が重なりの途中で終わった（ごく短い曲）: 残りは無音として重ねる
            fade.incoming_samples
                .resize(fade.outgoing_samples.len(), 0.0);
        }

        // 両方にある分を重ねる
        let samples = fade.outgoing_samples.len().min(fade.incoming_samples.len());
        let frames = samples / OUTPUT_CHANNELS;
        for frame in 0..frames {
            let (fade_out, fade_in) =
                crossfade_gains(fade.mixed_frames + frame as u64, fade.total_frames);
            for channel in 0..OUTPUT_CHANNELS {
                let index = frame * OUTPUT_CHANNELS + channel;
                self.pending.push(
                    fade.outgoing_samples[index] * fade_out
                        + fade.incoming_samples[index] * fade_in,
                );
            }
        }
        fade.outgoing_samples.drain(..frames * OUTPUT_CHANNELS);
        fade.incoming_samples.drain(..frames * OUTPUT_CHANNELS);
        fade.mixed_frames += frames as u64;

        if fade.outgoing_finished && fade.outgoing_samples.is_empty() {
            // 前の曲が終わった: 次の曲を、通常の再生へ切り替える
            let Some(mut fade) = self.fade.take() else {
                return Ok(());
            };
            // 先にデコードしてあった分。前の曲が予定より早く終わった場合は、残りのフェードインをかける
            let (mixed_frames, total_frames) = (fade.mixed_frames, fade.total_frames);
            for (frame, samples) in fade
                .incoming_samples
                .as_chunks_mut::<OUTPUT_CHANNELS>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (_, fade_in) = crossfade_gains(mixed_frames + frame as u64, total_frames);
                samples.iter_mut().for_each(|sample| *sample *= fade_in);
            }
            self.pending.extend_from_slice(&fade.incoming_samples);
            self.current = Some(fade.incoming);
            self.converter = Some(fade.converter);
        }
        Ok(())
    }

    /// デコードが出力に間に合わず、音が途切れていたらログに残す
    ///
    /// 再生の開始・シークの直後（バッファがたまる前）と、曲の終わり（書く音声がもうない）は、
    /// バッファが空になるのが正常なため数えない。
    fn log_underruns(&mut self) {
        let Some(output) = &mut self.output else {
            return;
        };
        if !output.filled || self.decode_finished {
            return;
        }
        let underruns = output.shared.underruns();
        if underruns > output.reported_underruns {
            log::warn!(
                "音声の出力にデコードが間に合わず、音が途切れました（{}回）",
                underruns - output.reported_underruns
            );
            output.reported_underruns = underruns;
        }
    }

    /// これから書く音声の位置（リングバッファへ書いた数に、まだ書いていない分を足したフレーム数）
    fn write_position(&self) -> u64 {
        let pushed = self
            .output
            .as_ref()
            .map_or(0, |output| output.pushed_frames);
        pushed + ((self.pending.len() - self.pending_offset) / OUTPUT_CHANNELS) as u64
    }

    /// デコードしていた曲が終わった: 続けて再生する曲があれば、切れ目なく続ける
    fn finish_current_decode(&mut self) {
        match self.next.take() {
            Some(next) => self.start_next(next),
            None => {
                // 変換器の中に残っている音声を出し切り、鳴り終わるのを待つ
                if let Some(converter) = &mut self.converter
                    && let Err(e) = converter.flush(&mut self.pending)
                {
                    log::warn!("曲の終わりの音声を変換できません: {}", e);
                }
                self.decode_finished = true;
                self.end_frame = Some(self.write_position());
            }
        }
    }

    /// `deck`の音声を、これまでに書いた音声に続けて書き始める
    fn start_next(&mut self, deck: Deck) {
        let Some(output_rate) = self.output.as_ref().map(|output| output.sample_rate) else {
            return;
        };
        let (rate, channels) = (deck.decoder.sample_rate(), deck.decoder.channels());
        // 形式が同じなら変換器を使い回す（変換器の中の、前の曲の終わりと切れ目なくつながる）
        let reusable = self
            .converter
            .as_ref()
            .is_some_and(|converter| converter.matches(rate, channels, output_rate));
        if !reusable {
            if let Some(converter) = &mut self.converter
                && let Err(e) = converter.flush(&mut self.pending)
            {
                log::warn!("曲の終わりの音声を変換できません: {}", e);
            }
            match Converter::new(rate, channels, output_rate) {
                Ok(converter) => self.converter = Some(converter),
                Err(e) => {
                    // 続けて再生できない曲: 続きがないものとして再生中の曲を終える
                    // （フロントエンドが次の曲を通常の手順で再生し、そこでエラーを通知する）
                    log::warn!("続けて再生する曲を準備できません: {}", e);
                    self.decode_finished = true;
                    self.end_frame = Some(self.write_position());
                    return;
                }
            }
        }
        let start_frame = self.write_position();
        self.segments.push_back(deck.segment(start_frame, 0.0));
        self.current = Some(deck);
        self.decode_finished = false;
        self.end_frame = None;
    }

    /// 鳴っている曲の中の、指定した位置（秒）から書き直す（シーク・出力の開き直し）
    fn reposition(&mut self, seconds: f64) -> AppResult<()> {
        let Some(front) = self.segments.front().cloned() else {
            return Ok(());
        };
        let output = self.output.as_mut().ok_or_else(output_closed)?;
        output.discard_buffered();
        self.pending.clear();
        self.pending_offset = 0;

        if let Some(mut fade) = self.fade.take() {
            if self.segments.len() > 1 {
                // 重なりは、まだ鳴り始めていない。鳴っている（前の）曲の中で移動し、
                // 重ね始めていた曲は、続けて再生する曲として頭から用意し直す
                if self.next.is_none() && fade.incoming.decoder.seek(0.0).is_ok() {
                    self.next = Some(fade.incoming);
                }
            } else {
                // 重なりが鳴っている: 鳴っている曲は次の曲に切り替わっているため、
                // 前の曲を止めて、次の曲の中で移動する
                self.current = Some(fade.incoming);
                self.converter = Some(fade.converter);
            }
        } else if self.segments.len() > 1 {
            // 鳴っている曲のデコードは終わり、続きの曲を書き始めていた。鳴っている曲を開き直し、
            // 書き始めていた曲は、続けて再生する曲として頭から用意し直す
            if let Some(mut upcoming) = self.current.take()
                && self.next.is_none()
                && upcoming.decoder.seek(0.0).is_ok()
            {
                self.next = Some(upcoming);
            }
            let request = PlayRequest {
                token: front.token,
                path: front.path.clone(),
                replay_gain: front.replay_gain.clone(),
            };
            let decoder = TrackDecoder::open(&front.path)?;
            self.current = Some(Deck::new(request, decoder, self.normalization));
        }
        let deck = self.current.as_mut().ok_or_else(output_closed)?;
        let position = deck.decoder.seek(seconds)?;
        let (rate, channels) = (deck.decoder.sample_rate(), deck.decoder.channels());
        match &mut self.converter {
            Some(converter) if converter.matches(rate, channels, output.sample_rate) => {
                converter.reset();
            }
            _ => self.converter = Some(Converter::new(rate, channels, output.sample_rate)?),
        }

        // シークの後は、倍率を途中から滑らかに変える必要がない
        deck.applied_gain = deck.gain;

        self.segments.clear();
        self.segments.push_back(Segment {
            start_frame: output.pushed_frames,
            base_seconds: position,
            ..front
        });
        self.decode_finished = false;
        self.end_frame = None;
        Ok(())
    }

    // ---------- 鳴っている位置 ----------

    /// 取り出されたフレーム数から、鳴っている曲の切り替わりと、再生の終わりを確かめる
    fn update_audible(&mut self) {
        let Some(output) = &self.output else {
            return;
        };
        let frames_read = output.shared.frames_read();

        let mut advanced = false;
        while self.segments.len() > 1 && frames_read >= self.segments[1].start_frame {
            self.segments.pop_front();
            let front = &self.segments[0];
            (self.emit)(PlaybackEvent::Advanced {
                token: front.token,
                duration: front.duration,
            });
            advanced = true;
        }
        if advanced {
            self.report_position(true);
        }

        if let Some(end_frame) = self.end_frame
            && frames_read >= end_frame
            && self.pending_offset >= self.pending.len()
            && let Some(token) = self.segments.front().map(|segment| segment.token)
        {
            self.stop_playback();
            (self.emit)(PlaybackEvent::Ended { token });
        }
    }

    /// 鳴っている曲の番号と、再生位置（秒）
    fn position(&self) -> Option<(u32, f64)> {
        let front = self.segments.front()?;
        let output = self.output.as_ref()?;
        let played = output
            .shared
            .frames_read()
            .saturating_sub(front.start_frame);
        let mut position = front.base_seconds + played as f64 / f64::from(output.sample_rate);
        if let Some(duration) = front.duration {
            position = position.min(duration);
        }
        Some((front.token, position))
    }

    /// 再生位置を通知する（`force`がfalseなら、再生中に一定の間隔で）
    fn report_position(&mut self, force: bool) {
        if !force && (self.paused || self.last_position_at.elapsed() < POSITION_INTERVAL) {
            return;
        }
        if let Some((token, position)) = self.position() {
            (self.emit)(PlaybackEvent::Position { token, position });
            self.last_position_at = Instant::now();
        }
    }

    // ---------- 出力 ----------

    fn open_output(&mut self) -> AppResult<()> {
        self.output_generation += 1;
        let generation = self.output_generation;
        let commands = self.commands.clone();
        let (volume, paused) = (self.volume, self.paused);
        let effects = self.effects.clone();

        let mut link = None;
        let handle = self.backend.open(
            self.device_id.as_deref(),
            &mut |sample_rate| {
                let frames = ((f64::from(sample_rate) * BUFFER_SECONDS) as usize).max(1024);
                let (producer, consumer) = rtrb::RingBuffer::new(frames * OUTPUT_CHANNELS);
                let shared = RenderShared::new(volume, paused);
                link = Some((producer, shared.clone(), sample_rate));
                Renderer::new(consumer, shared, sample_rate).with_effects(effects.clone())
            },
            Box::new(move |event| {
                let _ = commands.send(Command::Output { generation, event });
            }),
        )?;
        let (producer, shared, sample_rate) = link.ok_or_else(output_closed)?;
        self.output = Some(Output {
            handle,
            producer,
            shared,
            sample_rate,
            running: false,
            pushed_frames: 0,
            discarded_until: 0,
            filled: false,
            reported_underruns: 0,
        });
        Ok(())
    }

    /// 出力を動かす。動かせなければ、出力を開き直してやり直す（デバイスが外れていた場合など）
    fn start_output(&mut self) {
        if let Err(e) = self.try_start_output() {
            log::warn!("出力を開き直します: {}", e);
            self.reopen_output();
        }
    }

    fn try_start_output(&mut self) -> AppResult<()> {
        let output = self.output.as_mut().ok_or_else(output_closed)?;
        if !output.running {
            output.handle.start()?;
            output.running = true;
        }
        Ok(())
    }

    /// 出力を開き直し、鳴っていた曲を同じ位置から続ける（出力先・デバイスの設定が変わった場合）
    fn reopen_output(&mut self) {
        let position = self.position().map_or(0.0, |(_, position)| position);
        self.output = None;
        if self.segments.is_empty() {
            // 再生していなければ、次に再生するときに開く
            return;
        }
        let reopened = self
            .open_output()
            .and_then(|()| self.reposition(position))
            .and_then(|()| {
                if self.paused {
                    Ok(())
                } else {
                    self.pump();
                    self.try_start_output()
                }
            });
        if let Err(e) = reopened {
            self.fail(e);
        }
    }

    /// 再生していない間は出力を止め、何も再生しない状態が続いたら閉じる
    ///
    /// 出力を動かしたままにすると、無音を出しているだけでもOSがスリープしなくなる。
    fn release_idle_output(&mut self) {
        let Some(idle_since) = self.idle_since else {
            return;
        };
        let idle = idle_since.elapsed();
        if let Some(output) = &mut self.output
            && output.running
            && idle >= OUTPUT_STOP_DELAY
        {
            if let Err(e) = output.handle.stop() {
                log::warn!("{}", e);
            }
            output.running = false;
        }
        if self.segments.is_empty() && idle >= OUTPUT_CLOSE_DELAY {
            self.output = None;
        }
    }
}

/// より新しい再生の要求を先に受け取っていた場合のエラー（フロントエンドは、古い要求の結果を捨てる）
fn superseded() -> AppError {
    AppError::Playback("新しい再生の要求があったため、取り消しました".to_string())
}

impl Deck {
    /// デコーダーを開いた曲から、デッキを作る（`normalization`は、音量の正規化の設定）
    fn new(
        request: PlayRequest,
        decoder: TrackDecoder,
        normalization: VolumeNormalization,
    ) -> Self {
        let gain = normalization_gain(&request.replay_gain, normalization);
        Self {
            token: request.token,
            path: request.path,
            decoder,
            replay_gain: request.replay_gain,
            gain,
            applied_gain: gain,
        }
    }

    /// この曲の音声を`start_frame`から書き始める時の、書いた範囲の記録
    fn segment(&self, start_frame: u64, base_seconds: f64) -> Segment {
        Segment {
            token: self.token,
            path: self.path.clone(),
            replay_gain: self.replay_gain.clone(),
            start_frame,
            base_seconds,
            duration: self.decoder.duration_seconds(),
        }
    }
}

/// サンプル（ステレオのインターリーブ）に、音量の正規化の倍率をかける
///
/// 倍率が変わった直後（設定を変えた時）は、このかたまりの中で前の倍率から滑らかに変える。
fn apply_gain(samples: &mut [f32], applied: &mut f32, target: f32) {
    if samples.is_empty() {
        return;
    }
    let from = std::mem::replace(applied, target);
    if from == target {
        if target != 1.0 {
            samples.iter_mut().for_each(|sample| *sample *= target);
        }
        return;
    }
    let frames = samples.len() / OUTPUT_CHANNELS;
    for (index, frame) in samples
        .as_chunks_mut::<OUTPUT_CHANNELS>()
        .0
        .iter_mut()
        .enumerate()
    {
        let gain = from + (target - from) * (index + 1) as f32 / frames as f32;
        frame.iter_mut().for_each(|sample| *sample *= gain);
    }
}

fn output_closed() -> AppError {
    AppError::Playback("音声の出力を準備できません".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::decoder::tests::{temp_dir, wav_sample, write_wav, write_wav_samples};
    use super::*;
    use std::sync::Mutex;

    /// テスト用の出力のサンプルレート
    const RATE: u32 = 8_000;

    /// 音声デバイスの代わりに、テストから音声を取り出せる出力
    #[derive(Default)]
    struct FakeOutput {
        renderer: Option<Renderer>,
        on_event: Option<Box<dyn FnMut(OutputEvent) + Send>>,
        running: bool,
        /// 出力を開いた回数と、最後に開いたときのデバイス
        opened: usize,
        device_id: Option<String>,
        sample_rate: u32,
        /// 次に出力を動かすときに失敗させる
        fail_next_start: bool,
        /// イコライザとリミッターをつないだままにする（既定では外す。リミッターは音声を
        /// 先読みの時間だけ遅らせるため、取り出した音声の位置を確かめにくい）
        keep_effects: bool,
    }

    struct FakeBackend(Arc<Mutex<FakeOutput>>);

    impl OutputBackend for FakeBackend {
        fn open(
            &mut self,
            device_id: Option<&str>,
            make_renderer: &mut dyn FnMut(u32) -> Renderer,
            on_event: Box<dyn FnMut(OutputEvent) + Send>,
        ) -> AppResult<Box<dyn OutputHandle>> {
            let mut output = self.0.lock().unwrap();
            let renderer = make_renderer(output.sample_rate);
            output.renderer = Some(if output.keep_effects {
                renderer
            } else {
                renderer.without_effects()
            });
            output.on_event = Some(on_event);
            output.running = false;
            output.opened += 1;
            output.device_id = device_id.map(str::to_string);
            Ok(Box::new(FakeHandle(self.0.clone())))
        }
    }

    struct FakeHandle(Arc<Mutex<FakeOutput>>);

    impl OutputHandle for FakeHandle {
        fn start(&mut self) -> AppResult<()> {
            let mut output = self.0.lock().unwrap();
            if std::mem::take(&mut output.fail_next_start) {
                return Err(AppError::Playback("start failed".to_string()));
            }
            output.running = true;
            Ok(())
        }

        fn stop(&mut self) -> AppResult<()> {
            self.0.lock().unwrap().running = false;
            Ok(())
        }
    }

    /// 既定の設定（既定の出力デバイス・音量の正規化なし・クロスフェードなし）
    fn options() -> PlaybackOptions {
        PlaybackOptions {
            output_device_id: None,
            normalization: VolumeNormalization::Off,
            crossfade_seconds: 0,
        }
    }

    fn request(token: u32, path: &Path) -> PlayRequest {
        PlayRequest {
            token,
            path: path.to_path_buf(),
            replay_gain: ReplayGain::default(),
        }
    }

    /// エンジンをスレッドなしで動かす（コマンドの処理と定期的な処理を、テストから順に呼ぶ）
    struct Harness {
        engine: Engine,
        output: Arc<Mutex<FakeOutput>>,
        events: Arc<Mutex<Vec<PlaybackEvent>>>,
        commands: Receiver<Command>,
        /// イコライザの設定（`PlaybackEngine::set_equalizer`が書くもの）
        effects: Arc<EffectParams>,
        dir: PathBuf,
    }

    impl Harness {
        fn new() -> Self {
            Self::with_options(options())
        }

        fn with_options(options: PlaybackOptions) -> Self {
            let output = Arc::new(Mutex::new(FakeOutput {
                sample_rate: RATE,
                ..FakeOutput::default()
            }));
            let events = Arc::new(Mutex::new(Vec::new()));
            let (sender, commands) = mpsc::channel();
            let sink = events.clone();
            let effects = Arc::new(EffectParams::default());
            let engine = Engine::new(
                Box::new(FakeBackend(output.clone())),
                options,
                effects.clone(),
                Box::new(move |event| sink.lock().unwrap().push(event)),
                sender,
            );
            Self {
                engine,
                output,
                events,
                commands,
                effects,
                dir: temp_dir("engine"),
            }
        }

        /// 長さ`frames`のWAV（出力と同じサンプルレート・モノラル）を作る
        fn track(&self, name: &str, frames: u32) -> PathBuf {
            self.track_with_rate(name, RATE, frames)
        }

        fn track_with_rate(&self, name: &str, rate: u32, frames: u32) -> PathBuf {
            let path = self.dir.join(name);
            write_wav(&path, rate, 1, frames);
            path
        }

        fn play(&mut self, token: u32, path: &Path) -> AppResult<PlaybackTrackInfo> {
            let result = self.engine.play(request(token, path));
            self.engine.tick();
            result
        }

        fn set_next(&mut self, token: u32, path: &Path) -> AppResult<()> {
            let result = self.engine.set_next(Some(request(token, path)));
            self.engine.tick();
            result
        }

        fn send(&mut self, command: Command) {
            assert!(self.engine.handle(command));
            self.engine.tick();
        }

        /// 出力のコールバックの代わりに`frames`フレームを取り出し、左チャンネルの値を返す
        /// （取り出せた分だけ。出力が止まっていれば空）
        fn render(&mut self, frames: usize) -> Vec<f32> {
            let mut played = Vec::new();
            // エンジンがリングバッファを補充しながら、少しずつ取り出す。
            // 続けて何も取り出せなくなったら（再生の終わり・一時停止）やめる
            let mut stalled = 0;
            while played.len() < frames && stalled < 3 {
                let want = (frames - played.len()).min(100);
                {
                    let mut output = self.output.lock().unwrap();
                    if !output.running {
                        break;
                    }
                    let renderer = output.renderer.as_mut().unwrap();
                    let mut buffer = vec![0.0f32; want * 2];
                    // 読み捨て（シークなど）の分を除いた、鳴らしたフレームだけを数える
                    renderer.render(&mut [], 2);
                    let before = renderer.frames_read();
                    renderer.render(&mut buffer, 2);
                    let read = (renderer.frames_read() - before) as usize;
                    played.extend(
                        buffer
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .take(read)
                            .map(|frame| frame[0]),
                    );
                    stalled = if read == 0 { stalled + 1 } else { 0 };
                }
                self.engine.tick();
            }
            played
        }

        /// 出力からの通知（デバイスの変更など）を起こし、エンジンに処理させる
        fn fire_output_event(&mut self, event: OutputEvent) {
            let mut on_event = self.output.lock().unwrap().on_event.take().unwrap();
            on_event(event);
            while let Ok(command) = self.commands.try_recv() {
                assert!(self.engine.handle(command));
            }
            self.engine.tick();
        }

        fn take_events(&self) -> Vec<PlaybackEvent> {
            std::mem::take(&mut self.events.lock().unwrap())
        }

        /// 再生位置の通知を除いたイベント
        fn take_transitions(&self) -> Vec<PlaybackEvent> {
            self.take_events()
                .into_iter()
                .filter(|event| !matches!(event, PlaybackEvent::Position { .. }))
                .collect()
        }

        fn position(&self) -> f64 {
            self.engine
                .position()
                .map_or(-1.0, |(_, position)| position)
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn expected(frame: u32) -> f32 {
        f32::from(wav_sample(frame, 0)) / 32768.0
    }

    /// `played`の`index`番目が、曲の`frame`番目のサンプルか
    fn assert_sample(played: &[f32], index: usize, frame: u32) {
        assert!(
            (played[index] - expected(frame)).abs() < 1e-4,
            "index {index}: {} vs frame {frame} ({})",
            played[index],
            expected(frame)
        );
    }

    #[test]
    fn test_plays_a_track_to_the_end() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 4_000);

        let info = harness.play(1, &path).unwrap();
        assert_eq!(info.duration, Some(0.5));
        assert!(harness.output.lock().unwrap().running);

        let played = harness.render(3_000);
        assert_eq!(played.len(), 3_000);
        // 音量は頭の15msで上がりきるため、その後はファイルのとおりの音が出る
        assert_sample(&played, 1_000, 1_000);
        assert_sample(&played, 2_999, 2_999);
        assert!((harness.position() - 0.375).abs() < 1e-9);
        assert!(harness.take_transitions().is_empty());

        // 残りを鳴らしきると、終わりを通知して止まる
        let rest = harness.render(2_000);
        assert_eq!(rest.len(), 1_000);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 1 }]
        );
        assert!(harness.engine.position().is_none());
    }

    #[test]
    fn test_reports_position_while_playing() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(7, &path).unwrap();
        harness.take_events();

        harness.render(800);
        harness.engine.last_position_at = Instant::now() - POSITION_INTERVAL;
        harness.engine.tick();

        assert_eq!(
            harness.take_events(),
            vec![PlaybackEvent::Position {
                token: 7,
                position: 0.1
            }]
        );
    }

    #[test]
    fn test_next_track_follows_without_a_gap() {
        let mut harness = Harness::new();
        // 1曲目はリングバッファ（0.5秒）より長い（続けて再生する曲を用意する時点で、デコードの途中）
        let first = harness.track("a.wav", 6_000);
        let second = harness.track("b.wav", 3_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        assert!(!harness.engine.decode_finished);

        // 1曲目の終わりの手前までは、切り替わらない
        let played = harness.render(5_990);
        assert_eq!(played.len(), 5_990);
        assert!(harness.take_transitions().is_empty());

        // 1曲目の最後のサンプルの直後に、2曲目の最初のサンプルが続く
        let played = harness.render(1_010);
        assert_eq!(played.len(), 1_010);
        assert_sample(&played, 9, 5_999);
        assert_sample(&played, 10, 0);
        assert_sample(&played, 1_009, 999);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Advanced {
                token: 2,
                duration: Some(0.375)
            }]
        );
        assert!((harness.position() - 0.125).abs() < 1e-9);

        // 2曲目の終わりで止まる
        assert_eq!(harness.render(5_000).len(), 2_000);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 2 }]
        );
    }

    #[test]
    fn test_next_track_with_a_different_rate_follows() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 2_000);
        // 出力の2倍のサンプルレート・0.25秒
        let second = harness.track_with_rate("b.wav", RATE * 2, 4_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();

        // 2曲目は出力のレートへ変換され、0.25秒（2,000フレーム）になる
        assert_eq!(harness.render(10_000).len(), 4_000);
        assert_eq!(
            harness.take_transitions(),
            vec![
                PlaybackEvent::Advanced {
                    token: 2,
                    duration: Some(0.25)
                },
                PlaybackEvent::Ended { token: 2 }
            ]
        );
    }

    #[test]
    fn test_next_track_set_after_decoding_finished_still_follows() {
        let mut harness = Harness::new();
        // リングバッファ（0.5秒）より短い曲は、再生を始めた時点でデコードが終わっている
        let first = harness.track("a.wav", 1_000);
        let second = harness.track("b.wav", 1_000);
        harness.play(1, &first).unwrap();
        assert!(harness.engine.decode_finished);

        harness.set_next(2, &second).unwrap();
        let played = harness.render(5_000);

        assert_eq!(played.len(), 2_000);
        assert_sample(&played, 999, 999);
        assert_sample(&played, 1_000, 0);
        assert_eq!(
            harness.take_transitions(),
            vec![
                PlaybackEvent::Advanced {
                    token: 2,
                    duration: Some(0.125)
                },
                PlaybackEvent::Ended { token: 2 }
            ]
        );
    }

    #[test]
    fn test_clearing_next_ends_after_the_current_track() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 6_000);
        let second = harness.track("b.wav", 1_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        assert!(harness.engine.set_next(None).is_ok());

        assert_eq!(harness.render(10_000).len(), 6_000);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 1 }]
        );
    }

    #[test]
    fn test_play_replaces_the_current_track_immediately() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 8_000);
        let second = harness.track("b.wav", 8_000);
        harness.play(1, &first).unwrap();
        harness.render(1_000);

        harness.play(2, &second).unwrap();
        let played = harness.render(1_000);

        // バッファにたまっていた1曲目の音声は出ず、2曲目の頭から鳴る
        assert_eq!(played.len(), 1_000);
        assert_sample(&played, 500, 500);
        assert!((harness.position() - 0.125).abs() < 1e-9);
        assert!(harness.take_transitions().is_empty());
    }

    // 開いているファイルを消せるのはUnixだけ
    #[cfg(unix)]
    #[test]
    fn test_play_reuses_the_prepared_next_track() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 8_000);
        let second = harness.track("b.wav", 8_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        // 用意した後でファイルが消えても、開いてあるデコーダーで再生できる
        std::fs::remove_file(&second).unwrap();

        assert!(harness.play(3, &second).is_ok());
        let played = harness.render(600);
        assert_sample(&played, 500, 500);
    }

    #[test]
    fn test_requests_that_arrive_out_of_order_do_not_replace_newer_ones() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 8_000);
        let second = harness.track("b.wav", 8_000);
        // 2曲を続けて選び、後から選んだ曲（番号2）の要求が先に届いた
        harness.play(2, &second).unwrap();
        assert!(matches!(
            harness.play(1, &first),
            Err(AppError::Playback(_))
        ));
        // 前の再生のための「続けて再生する曲」も無視する
        harness.set_next(1, &first).unwrap();
        assert!(harness.engine.next.is_none());

        // 後から選んだ曲が鳴り続ける
        let played = harness.render(1_000);
        assert_sample(&played, 999, 999);
        assert_eq!(harness.engine.position().map(|(token, _)| token), Some(2));
    }

    #[test]
    fn test_seek_moves_within_the_track() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(1_000);
        harness.take_events();

        harness.send(Command::Seek(0.5));
        assert_eq!(
            harness.take_events(),
            vec![PlaybackEvent::Position {
                token: 1,
                position: 0.5
            }]
        );

        let played = harness.render(1_000);
        assert_eq!(played.len(), 1_000);
        assert_sample(&played, 500, 4_500);
        assert!((harness.position() - 0.625).abs() < 1e-9);

        // 終わりまで鳴らすと、曲の長さどおりに終わる
        assert_eq!(harness.render(10_000).len(), 3_000);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 1 }]
        );
    }

    #[test]
    fn test_seek_back_after_the_next_track_was_buffered() {
        let mut harness = Harness::new();
        // 1曲目はリングバッファより短いため、再生を始めた時点で2曲目の音声も書かれている
        let first = harness.track("a.wav", 2_000);
        let second = harness.track("b.wav", 8_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        assert_eq!(harness.engine.segments.len(), 2);

        // 鳴っているのは1曲目のため、シークは1曲目の中で行う
        harness.send(Command::Seek(0.125));
        let played = harness.render(1_200);

        assert_sample(&played, 500, 1_500);
        // その後、2曲目へ頭から続く
        assert_sample(&played, 1_000, 0);
        assert_sample(&played, 1_199, 199);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Advanced {
                token: 2,
                duration: Some(1.0)
            }]
        );
    }

    #[test]
    fn test_seek_to_the_end_finishes_the_track() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(100);

        harness.send(Command::Seek(99.0));
        harness.render(100);

        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 1 }]
        );
    }

    #[test]
    fn test_pause_and_resume_keep_the_position() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(1_000);

        harness.send(Command::Pause);
        // 音量を下げきる分（15ms）だけ進んで止まる
        let faded = harness.render(1_000).len();
        assert!(faded <= 130, "{faded}");
        let paused_at = harness.position();
        assert_eq!(harness.render(1_000).len(), 0);
        assert_eq!(harness.position(), paused_at);

        // しばらくすると出力を止める（OSのスリープを妨げない）が、曲は保ったまま
        harness.engine.idle_since = Some(Instant::now() - OUTPUT_STOP_DELAY);
        harness.engine.tick();
        assert!(!harness.output.lock().unwrap().running);
        assert_eq!(harness.position(), paused_at);

        harness.send(Command::Resume);
        assert!(harness.output.lock().unwrap().running);
        let played = harness.render(500);
        assert_eq!(played.len(), 500);
        assert_sample(&played, 499, 1_000 + faded as u32 + 499);
    }

    #[test]
    fn test_seek_while_paused_resumes_from_the_new_position() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(500);
        harness.send(Command::Pause);
        harness.render(500);
        harness.engine.idle_since = Some(Instant::now() - OUTPUT_STOP_DELAY);
        harness.engine.tick();

        harness.send(Command::Seek(0.75));
        assert_eq!(harness.position(), 0.75);

        harness.send(Command::Resume);
        let played = harness.render(400);
        assert_sample(&played, 399, 6_399);
    }

    #[test]
    fn test_stop_discards_everything_and_closes_the_output_later() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(500);

        harness.send(Command::Stop);
        assert!(harness.engine.position().is_none());
        assert_eq!(harness.render(500).len(), 0);
        assert!(harness.take_transitions().is_empty());

        // すぐに次の曲を再生する場合は、開いたままの出力を使う
        harness.play(2, &path).unwrap();
        assert_eq!(harness.output.lock().unwrap().opened, 1);
        harness.send(Command::Stop);

        // 何も再生しない状態が続くと、出力を閉じる
        harness.engine.idle_since = Some(Instant::now() - OUTPUT_CLOSE_DELAY);
        harness.engine.tick();
        assert!(harness.engine.output.is_none());
        harness.play(3, &path).unwrap();
        assert_eq!(harness.output.lock().unwrap().opened, 2);
    }

    #[test]
    fn test_play_reports_files_that_cannot_be_opened() {
        let mut harness = Harness::new();
        let playing = harness.track("a.wav", 8_000);
        harness.play(1, &playing).unwrap();

        let missing = harness.dir.join("missing.wav");
        assert!(matches!(
            harness.play(2, &missing),
            Err(AppError::NotFound(_))
        ));
        // 再生中だった曲は止まる（選んだ曲を再生できなかったのに、前の曲が鳴り続けない）
        assert!(harness.engine.position().is_none());
        assert_eq!(harness.render(100).len(), 0);

        // 続けて再生する曲を開けない場合は、エラーを返すだけで再生は続く
        harness.play(3, &playing).unwrap();
        assert!(harness.set_next(4, &missing).is_err());
        assert_eq!(harness.render(100).len(), 100);
    }

    #[test]
    fn test_decode_failure_stops_playback_and_notifies() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(100);

        harness
            .engine
            .fail(AppError::Playback("broken".to_string()));

        assert!(matches!(
            harness.take_transitions().as_slice(),
            [PlaybackEvent::Failed { token: 1, .. }]
        ));
        assert!(harness.engine.position().is_none());
    }

    #[test]
    fn test_volume_is_applied_to_the_output() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.send(Command::SetVolume(0.5));
        harness.play(1, &path).unwrap();

        let played = harness.render(1_000);
        assert!((played[999] - expected(999) * 0.5).abs() < 1e-4);

        // 範囲外の値は丸める
        harness.send(Command::SetVolume(7.0));
        let played = harness.render(1_000);
        assert_sample(&played, 999, 1_999);
    }

    #[test]
    fn test_output_change_reopens_and_continues_from_the_same_position() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(2_000);

        // 出力先が変わり、サンプルレートも変わった
        harness.output.lock().unwrap().sample_rate = RATE * 2;
        harness.fire_output_event(OutputEvent::DeviceChanged);

        assert_eq!(harness.output.lock().unwrap().opened, 2);
        assert!(harness.output.lock().unwrap().running);
        assert!((harness.position() - 0.25).abs() < 1e-9);
        // 新しいレート（2倍）で、続きから鳴る
        assert_eq!(harness.render(2_000).len(), 2_000);
        assert!((harness.position() - 0.375).abs() < 1e-9);
        assert!(harness.take_transitions().is_empty());
    }

    #[test]
    fn test_stale_output_event_is_ignored() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();

        // 閉じた出力からの遅れた通知では、開き直さない
        harness.send(Command::Output {
            generation: 0,
            event: OutputEvent::DeviceChanged,
        });
        assert_eq!(harness.output.lock().unwrap().opened, 1);
    }

    #[test]
    fn test_changing_the_output_device_reopens_the_output() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(1_000);

        harness.send(Command::SetOutputDevice(Some("usb-dac".to_string())));

        let (opened, device_id) = {
            let output = harness.output.lock().unwrap();
            (output.opened, output.device_id.clone())
        };
        assert_eq!((opened, device_id.as_deref()), (2, Some("usb-dac")));
        let played = harness.render(500);
        assert_sample(&played, 499, 1_499);

        // 同じデバイスを選び直しても、開き直さない
        harness.send(Command::SetOutputDevice(Some("usb-dac".to_string())));
        assert_eq!(harness.output.lock().unwrap().opened, 2);
    }

    #[test]
    fn test_output_that_fails_to_start_is_reopened() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 8_000);
        harness.play(1, &path).unwrap();
        harness.render(1_000);
        harness.send(Command::Pause);
        harness.render(500);
        harness.engine.idle_since = Some(Instant::now() - OUTPUT_STOP_DELAY);
        harness.engine.tick();
        let paused_at = harness.position();

        // 止めている間にデバイスが外れ、再開で出力を動かせない
        harness.output.lock().unwrap().fail_next_start = true;
        harness.send(Command::Resume);

        assert_eq!(harness.output.lock().unwrap().opened, 2);
        assert!(harness.output.lock().unwrap().running);
        assert!((harness.position() - paused_at).abs() < 1e-9);
        assert_eq!(harness.render(500).len(), 500);
    }

    #[test]
    fn test_engine_thread_plays_through_the_public_api() {
        let output = Arc::new(Mutex::new(FakeOutput {
            sample_rate: RATE,
            ..FakeOutput::default()
        }));
        let (events, received) = mpsc::channel();
        let engine = PlaybackEngine::start_with_backend(
            Box::new(FakeBackend(output.clone())),
            options(),
            Box::new(move |event| {
                let _ = events.send(event);
            }),
        );
        let dir = temp_dir("engine-thread");
        let path = dir.join("a.wav");
        write_wav(&path, RATE, 1, 800);

        let info = engine.play(request(5, &path)).unwrap();
        assert_eq!(info.duration, Some(0.1));
        engine.set_volume(0.8).unwrap();

        // 出力のコールバックの代わりに音声を取り出し、終わりの通知を待つ
        let deadline = Instant::now() + Duration::from_secs(10);
        let ended = loop {
            if let Some(renderer) = output.lock().unwrap().renderer.as_mut() {
                renderer.render(&mut [0.0f32; 512], 2);
            }
            match received.recv_timeout(Duration::from_millis(5)) {
                Ok(PlaybackEvent::Ended { token }) => break Some(token),
                Ok(_) | Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                _ => break None,
            }
        };
        assert_eq!(ended, Some(5));

        // 終了すると、スレッドも終わる（コマンドがエラーになる）
        let commands = engine.commands.clone();
        drop(engine);
        let deadline = Instant::now() + Duration::from_secs(10);
        while commands.send(Command::Pause).is_ok() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(commands.send(Command::Pause).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    // ---------- 音量の正規化 ----------

    /// トラックのゲインだけを持つReplayGain
    fn track_gain(db: f64) -> ReplayGain {
        ReplayGain {
            track_gain: Some(db),
            ..ReplayGain::default()
        }
    }

    fn from_db(db: f64) -> f32 {
        10f64.powf(db / 20.0) as f32
    }

    #[test]
    fn test_normalization_scales_each_track_by_its_gain() {
        let mut harness = Harness::with_options(PlaybackOptions {
            normalization: VolumeNormalization::Track,
            ..options()
        });
        let first = harness.track("a.wav", 2_000);
        let second = harness.track("b.wav", 2_000);
        harness
            .engine
            .play(PlayRequest {
                replay_gain: track_gain(-6.0),
                ..request(1, &first)
            })
            .unwrap();
        harness
            .engine
            .set_next(Some(PlayRequest {
                replay_gain: track_gain(-12.0),
                ..request(2, &second)
            }))
            .unwrap();

        let played = harness.render(4_000);

        // 曲ごとの倍率がかかり、曲の切れ目で切り替わる
        assert!((played[1_500] - expected(1_500) * from_db(-6.0)).abs() < 1e-4);
        assert!((played[1_999] - expected(1_999) * from_db(-6.0)).abs() < 1e-4);
        assert!((played[2_500] - expected(500) * from_db(-12.0)).abs() < 1e-4);
    }

    #[test]
    fn test_normalization_mode_change_applies_to_the_playing_track() {
        let mut harness = Harness::new();
        let path = harness.track("a.wav", 16_000);
        harness
            .engine
            .play(PlayRequest {
                replay_gain: track_gain(-6.0),
                ..request(1, &path)
            })
            .unwrap();
        // オフの間は、そのままの音量
        let played = harness.render(1_000);
        assert_sample(&played, 999, 999);

        harness.send(Command::SetNormalization(VolumeNormalization::Track));
        // バッファにたまっていた分（0.5秒）の後から、新しい倍率になる（途中は滑らかに変わる）
        let played = harness.render(9_000);
        assert_sample(&played, 1_000, 2_000);
        let scaled = played[8_999] / expected(9_999);
        assert!((scaled - from_db(-6.0)).abs() < 1e-3, "{scaled}");
        let changing: Vec<f32> = (4_000..8_000)
            .map(|index| played[index] / expected(1_000 + index as u32))
            .collect();
        assert!(changing.windows(2).all(|pair| pair[1] <= pair[0] + 1e-4));
        assert!(
            changing
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.01)
        );

        // シークの後は、すぐに新しい倍率で鳴る
        harness.send(Command::SetNormalization(VolumeNormalization::Off));
        harness.send(Command::Seek(0.5));
        let played = harness.render(500);
        assert_sample(&played, 400, 4_400);
    }

    // ---------- クロスフェード ----------

    /// クロスフェードの秒数を指定したハーネス
    fn crossfading(seconds: u8) -> Harness {
        Harness::with_options(PlaybackOptions {
            crossfade_seconds: seconds,
            ..options()
        })
    }

    #[test]
    fn test_crossfade_overlaps_the_end_and_the_start_with_equal_power() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000); // 2秒
        let second = harness.track("b.wav", 24_000); // 3秒
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();

        // 1曲目の終わりの1秒（8,000フレーム）前までは、1曲目だけ
        let played = harness.render(7_990);
        assert_sample(&played, 7_989, 7_989);
        assert!(harness.take_transitions().is_empty());

        // 重なりが鳴り始めた時点で、曲が切り替わったと通知する
        let mut played = [played, harness.render(20)].concat();
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Advanced {
                token: 2,
                duration: Some(3.0)
            }]
        );
        played.extend(harness.render(40_000));

        // 重なった分（1秒）だけ、全体が短くなる
        assert_eq!(played.len(), 16_000 + 24_000 - 8_000);
        // 重なりの間は、1曲目をcos・2曲目をsinの音量で足したもの
        for index in [0usize, 1, 2_000, 4_000, 7_999] {
            let (fade_out, fade_in) = crossfade_gains(index as u64, 8_000);
            let wanted =
                expected(8_000 + index as u32) * fade_out + expected(index as u32) * fade_in;
            assert!(
                (played[8_000 + index] - wanted).abs() < 1e-4,
                "{index}: {} vs {wanted}",
                played[8_000 + index]
            );
        }
        // 真ん中では、どちらも同じ音量（約0.707）
        let (fade_out, fade_in) = crossfade_gains(4_000, 8_000);
        assert!((fade_out - fade_in).abs() < 1e-3 && (fade_out - 0.707).abs() < 1e-3);
        // 重なりの後は、2曲目だけ
        assert_sample(&played, 16_000, 8_000);
        assert_sample(&played, 31_999, 23_999);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 2 }]
        );
    }

    #[test]
    fn test_crossfade_is_limited_to_half_of_each_track() {
        // 設定は5秒だが、2曲目が0.5秒しかないため、重ねるのは0.25秒（2,000フレーム）まで
        let mut harness = crossfading(5);
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 4_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();

        let played = harness.render(40_000);

        assert_eq!(played.len(), 16_000 + 4_000 - 2_000);
        assert_sample(&played, 13_999, 13_999);
        assert_sample(&played, 16_000, 2_000);
    }

    #[test]
    fn test_crossfade_between_different_sample_rates() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000);
        // 出力の2倍のサンプルレート・2秒
        let second = harness.track_with_rate("b.wav", RATE * 2, 32_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();

        let played = harness.render(60_000);

        // 2曲目は出力のレート（16,000フレーム）になり、1秒重なる
        assert_eq!(played.len(), 16_000 + 16_000 - 8_000);
        assert_eq!(
            harness.take_transitions(),
            vec![
                PlaybackEvent::Advanced {
                    token: 2,
                    duration: Some(2.0)
                },
                PlaybackEvent::Ended { token: 2 }
            ]
        );
    }

    #[test]
    fn test_repeating_the_same_track_joins_without_crossfade() {
        let mut harness = crossfading(1);
        let path = harness.track("a.wav", 16_000);
        harness.play(1, &path).unwrap();
        // 1曲リピート: 同じ曲を続けて再生する
        harness.set_next(2, &path).unwrap();

        let played = harness.render(40_000);

        assert_eq!(played.len(), 32_000);
        assert_sample(&played, 15_999, 15_999);
        assert_sample(&played, 16_000, 0);
    }

    #[test]
    fn test_crossfade_uses_the_remaining_time_when_the_next_track_arrives_late() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 24_000);
        harness.play(1, &first).unwrap();
        // 残りが0.5秒を切ってから、続けて再生する曲が届く
        // （デコードは、鳴っている位置より0.5秒（バッファの分）先まで進んでいる）
        let mut played = harness.render(9_000);
        harness.set_next(2, &second).unwrap();
        played.extend(harness.render(40_000));

        let overlap = 16_000 + 24_000 - played.len();
        assert!((2_000..=3_100).contains(&overlap), "{overlap}");
        // 重なりの前後は、それぞれの曲だけ
        assert_sample(&played, 16_000 - overlap - 1, 16_000 - overlap as u32 - 1);
        assert_sample(&played, 16_000, overlap as u32);
    }

    #[test]
    fn test_seek_during_the_crossfade_continues_in_the_new_track() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 24_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        // 重なりの途中まで鳴らす（鳴っている曲は、2曲目に切り替わっている）
        harness.render(10_000);
        assert_eq!(harness.take_transitions().len(), 1);

        harness.send(Command::Seek(1.0));
        let played = harness.render(1_000);

        // 1曲目は止まり、2曲目だけが通常の音量で鳴る
        assert_sample(&played, 500, 8_500);
        assert!((harness.position() - 1.125).abs() < 1e-9);
        assert_eq!(harness.render(40_000).len(), 24_000 - 9_000);
        assert_eq!(
            harness.take_transitions(),
            vec![PlaybackEvent::Ended { token: 2 }]
        );
    }

    #[test]
    fn test_seek_before_the_crossfade_is_heard_stays_in_the_playing_track() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 24_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        // 重なりの直前まで鳴らす（重なりの頭は、もうバッファに書かれている）
        harness.render(7_500);
        assert!(harness.engine.fade.is_some());

        // 鳴っているのは1曲目のため、シークは1曲目の中で行う
        harness.send(Command::Seek(0.25));
        let played = harness.render(1_000);
        assert_sample(&played, 500, 2_500);
        assert!(harness.take_transitions().is_empty());

        // その後、あらためて2曲目へクロスフェードする
        let rest = harness.render(40_000);
        assert_eq!(rest.len(), 16_000 - 3_000 + 24_000 - 8_000);
        assert_eq!(
            harness.take_transitions(),
            vec![
                PlaybackEvent::Advanced {
                    token: 2,
                    duration: Some(3.0)
                },
                PlaybackEvent::Ended { token: 2 }
            ]
        );
    }

    #[test]
    fn test_play_during_the_crossfade_switches_immediately() {
        let mut harness = crossfading(1);
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 24_000);
        let third = harness.track("c.wav", 8_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        harness.render(10_000);
        harness.take_events();

        // 手動で曲を選んだ時は、クロスフェードせずにすぐ切り替える
        harness.play(3, &third).unwrap();
        let played = harness.render(1_000);

        assert_sample(&played, 500, 500);
        assert!(harness.engine.fade.is_none());
        assert!(harness.take_transitions().is_empty());
    }

    #[test]
    fn test_crossfade_setting_can_be_changed_while_playing() {
        let mut harness = Harness::new();
        let first = harness.track("a.wav", 16_000);
        let second = harness.track("b.wav", 24_000);
        harness.play(1, &first).unwrap();
        harness.set_next(2, &second).unwrap();
        harness.render(1_000);

        // 範囲外の値は、上限（12秒）に丸める。重ねるのは、曲の長さの半分（1秒）まで
        harness.send(Command::SetCrossfade(200));
        assert_eq!(harness.engine.crossfade_seconds, 12.0);

        assert_eq!(harness.render(50_000).len(), 39_000 - 8_000);
    }

    // ---------- イコライザ・リミッター ----------

    #[test]
    fn test_output_is_limited_when_the_gain_boosts_past_full_scale() {
        let mut harness = Harness::with_options(PlaybackOptions {
            normalization: VolumeNormalization::Track,
            ..options()
        });
        harness.output.lock().unwrap().keep_effects = true;
        let path = harness.track("a.wav", 16_000);
        // ピークの情報がなく、+12dB（約4倍）上げる曲: そのままでは最大を大きく超える
        harness
            .engine
            .play(PlayRequest {
                replay_gain: track_gain(12.0),
                ..request(1, &path)
            })
            .unwrap();

        let mut output = harness.output.lock().unwrap();
        let mut buffer = vec![0.0f32; 4_000 * 2];
        output.renderer.as_mut().unwrap().render(&mut buffer, 2);

        // 元の音（最大0.12）が4倍になり、最大を超える分はリミッターが抑える
        let peak = buffer
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.4 && peak <= 1.0, "{peak}");
    }

    #[test]
    fn test_equalizer_settings_reach_the_output() {
        let mut harness = Harness::new();
        harness.output.lock().unwrap().keep_effects = true;
        // 1kHzの正弦波（出力と同じサンプルレートでは、8フレームで1周期）
        let path = harness.dir.join("sine.wav");
        let sine: Vec<i16> = (0..32_000)
            .map(|frame| {
                let phase = 2.0 * std::f64::consts::PI * f64::from(frame) / 8.0;
                (phase.sin() * 8_000.0) as i16
            })
            .collect();
        write_wav_samples(&path, RATE, 1, &sine);
        harness.play(1, &path).unwrap();
        let effects = harness.effects.clone();

        // 取り出した音声の大きさ（2乗の平均）
        let mut power = |frames: usize| {
            let played = harness.render(frames);
            assert_eq!(played.len(), frames);
            played.iter().map(|sample| sample * sample).sum::<f32>() / frames as f32
        };
        // 音量が上がりきるまで流してから測る
        power(400);
        let flat = power(1_600);

        // 1kHzのバンドを-12dBにすると、大きさ（2乗）は約1/16になる
        let mut gains = [0.0; EQ_BANDS];
        gains[5] = -12.0;
        effects.set_equalizer(true, &gains);
        // ゲインが目標に届くまで（0.1秒）流してから測る
        power(1_600);
        let cut = power(1_600);

        let ratio = cut / flat;
        assert!((0.055..0.07).contains(&ratio), "{ratio}");
    }

    #[test]
    fn test_options_are_taken_from_the_settings() {
        let settings = Settings {
            output_device_id: Some("usb-dac".to_string()),
            volume_normalization: VolumeNormalization::Album,
            crossfade_seconds: 7,
            ..Settings::default()
        };

        assert_eq!(
            PlaybackOptions::from(&settings),
            PlaybackOptions {
                output_device_id: Some("usb-dac".to_string()),
                normalization: VolumeNormalization::Album,
                crossfade_seconds: 7,
            }
        );
    }

    /// 実際の出力デバイスで再生する（手動の確認用。音量を0にするため、音は出ない）
    ///
    /// ```text
    /// PLAYBACK_TEST_FILES=a.flac:b.mp3 cargo test real_output -- --ignored --nocapture
    /// ```
    ///
    /// 1曲目を再生し、2曲目以降を続けて再生する曲として順に渡す。途中でシーク・一時停止・
    /// 出力デバイスの切り替え（`PLAYBACK_TEST_DEVICE`にデバイスのIDを指定した場合）を行い、
    /// 通知の内容と、かかった時間（実時間で再生されたか）を表示する。
    /// `PLAYBACK_TEST_PLAIN=1`を指定すると、途中の操作をせずに最後まで再生する（長時間の再生の確認用）。
    /// `PLAYBACK_TEST_CROSSFADE`に、クロスフェードの秒数を指定できる。
    #[test]
    #[ignore]
    fn real_output_plays_files_in_real_time() {
        let files: Vec<PathBuf> = std::env::var("PLAYBACK_TEST_FILES")
            .expect("PLAYBACK_TEST_FILESに、再生するファイルを:区切りで指定する")
            .split(':')
            .map(PathBuf::from)
            .collect();
        // エンジンのログ（出力の開き直し・音切れなど）も表示する
        struct StderrLogger;
        impl log::Log for StderrLogger {
            fn enabled(&self, _: &log::Metadata) -> bool {
                true
            }
            fn log(&self, record: &log::Record) {
                eprintln!("[{}] {}", record.level(), record.args());
            }
            fn flush(&self) {}
        }
        let _ = log::set_logger(&StderrLogger);
        log::set_max_level(log::LevelFilter::Info);
        println!(
            "出力デバイス: {:#?}",
            super::super::output::list_output_devices()
        );

        let (events, received) = mpsc::channel();
        let options = PlaybackOptions {
            crossfade_seconds: std::env::var("PLAYBACK_TEST_CROSSFADE")
                .map_or(0, |seconds| seconds.parse().unwrap()),
            ..options()
        };
        let engine = PlaybackEngine::start(options, move |event| {
            let _ = events.send(event);
        });
        engine.set_volume(0.0).unwrap();

        let started = Instant::now();
        let elapsed = || format!("{:7.3}s", started.elapsed().as_secs_f64());
        let info = engine.play(request(0, &files[0])).unwrap();
        println!("{} play 0: {:?}", elapsed(), info);
        let mut next = 1;
        let mut queue_next = |engine: &PlaybackEngine| {
            if let Some(path) = files.get(next) {
                let result = engine.set_next(Some(request(next as u32, path)));
                println!("{} set_next {next}: {:?}", elapsed(), result);
                next += 1;
            }
        };
        queue_next(&engine);

        // 再生を始めて1秒後にシーク、2秒後に一時停止（1秒間）、4秒後にデバイスの切り替え
        let mut steps = if std::env::var("PLAYBACK_TEST_PLAIN").is_ok() {
            Vec::new()
        } else {
            vec![
                (Duration::from_secs(1), "seek"),
                (Duration::from_secs(2), "pause"),
                (Duration::from_secs(3), "resume"),
                (Duration::from_secs(4), "device"),
            ]
        };
        let mut last_event = Instant::now();
        let mut last_position = Instant::now() - Duration::from_secs(1);
        loop {
            if let Some((at, step)) = steps.first().copied()
                && started.elapsed() >= at
            {
                steps.remove(0);
                match step {
                    "seek" => {
                        // 終わりの3秒前へ
                        let position = (info.duration.unwrap_or(10.0) - 3.0).max(0.0);
                        println!("{} seek {position}", elapsed());
                        engine.seek(position).unwrap();
                    }
                    "pause" => {
                        println!("{} pause", elapsed());
                        engine.pause().unwrap();
                    }
                    "resume" => {
                        println!("{} resume", elapsed());
                        engine.resume().unwrap();
                    }
                    _ => {
                        if let Ok(device) = std::env::var("PLAYBACK_TEST_DEVICE") {
                            println!("{} set_output_device {device}", elapsed());
                            engine.set_output_device(Some(device)).unwrap();
                        }
                    }
                }
            }
            let event = received.recv_timeout(Duration::from_millis(20));
            if event.is_ok() {
                last_event = Instant::now();
            }
            match event {
                Ok(PlaybackEvent::Position { token, position }) => {
                    // 再生位置は、1秒ごとに表示する
                    if last_position.elapsed() >= Duration::from_secs(1) {
                        println!("{} position {token}: {position:.3}", elapsed());
                        last_position = Instant::now();
                    }
                }
                Ok(PlaybackEvent::Advanced { token, duration }) => {
                    println!("{} advanced {token}: {duration:?}", elapsed());
                    queue_next(&engine);
                }
                Ok(PlaybackEvent::Ended { token }) => {
                    println!("{} ended {token}", elapsed());
                    break;
                }
                Ok(PlaybackEvent::Failed { token, error }) => {
                    panic!("{} failed {token}: {error}", elapsed());
                }
                // 一時停止の間（1秒）を除き、再生位置の通知が届き続ける
                Err(_) => assert!(
                    last_event.elapsed() < Duration::from_secs(10),
                    "エンジンからの通知が止まった"
                ),
            }
        }
    }
}
