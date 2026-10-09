//! 音声ファイルのデコード
//!
//! `symphonia`でファイルを開き、インターリーブのf32のサンプルとして順に取り出す。
//! Opusは`symphonia`にデコーダーがないため、libopusのアダプターを登録して使う。
//!
//! # 曲の頭と終わりの余分
//!
//! 非可逆の形式は、曲の頭にエンコーダーの遅延、終わりにパディングを持つ。ギャップレス再生の
//! ために、どちらも取り除いて返す。取り除く範囲は、パケットのタイムスタンプから決める。
//!
//! - 返し始める位置（開いた直後は曲の頭、シークの直後は目的の位置）より前のフレームは捨てる。
//!   デコードした結果の先頭がファイルの中のどこにあたるかは、パケットのタイムスタンプで分かる
//! - 「ファイルの中の位置」と「曲の頭からの位置」の差（`offset`）は形式によって違う。
//!   `symphonia`は、MP3・Ogg Vorbisではタイムスタンプを曲の頭が0になるようにずらすが、
//!   Opusではずらさない。最初のパケットのタイムスタンプと、トラックの遅延の和が、その差になる
//! - MP4のAACは、`symphonia`が遅延を扱わないため、ファイルの情報（`mp4_gapless`）を読んで足す
//! - 終わりは、曲の長さ（フレーム数）を超える分を返さない

use super::mp4_gapless;
use crate::error::{AppError, AppResult};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

/// 続けてデコードに失敗したパケットがこの数を超えたら、再生をあきらめる
/// （壊れたパケットが少し混じっているだけのファイルは、そのパケットを飛ばして再生を続ける）
const MAX_CONSECUTIVE_DECODE_ERRORS: u32 = 50;

/// シークで、目的の位置の何分の1秒手前からデコードし直すか（1/10秒）
///
/// 非可逆の形式は、前のパケットの内容を使ってデコードする（MP3のビットリザーバー、
/// AAC・Vorbis・Opusの重ね合わせ）。目的の位置のパケットから始めると頭の音が乱れるため、
/// 少し手前から読んで、目的の位置までを捨てる。
const SEEK_PREROLL_DIVISOR: u32 = 10;

/// MP4のコンテナを表す`symphonia`の名前
const ISOMP4_FORMAT_NAME: &str = "isomp4";

/// 使えるデコーダーの一覧（`symphonia`のデコーダーに、libopusを加える）
fn codec_registry() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
        registry
    })
}

/// 1つの音声ファイルのデコーダー
pub struct TrackDecoder {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    /// タイムスタンプ1単位の秒数
    seconds_per_ts: f64,
    /// ファイルの中の位置（フレーム）から、曲の頭からの位置を求めるために引く数
    offset: i64,
    /// 曲の長さ（フレーム数。頭・終わりの余分を除く。分からなければNone）
    total_frames: Option<u64>,
    /// 次に返すフレームの位置（曲の頭から）
    position: u64,
    /// 開いた直後・シークの直後の、返し始める位置（曲の頭から）。これより前のフレームは捨てる
    start_at: Option<u64>,
    /// デコードしたサンプル（インターリーブ）
    samples: Vec<f32>,
    /// `samples`のうち、まだ返していない範囲（フレーム）
    pending: std::ops::Range<usize>,
    /// `samples`の先頭のフレームの、タイムスタンプ
    pending_ts: i64,
    /// 最初のパケットをデコードして、実際の形式を確かめたか
    primed: bool,
    /// 曲の終わりへシークした（次の`next_chunk`はNoneを返す）
    finished: bool,
    /// ファイルを終わりまで読んだ
    exhausted: bool,
    consecutive_errors: u32,
}

impl TrackDecoder {
    /// ファイルを開き、最初のパケットをデコードして形式（サンプルレート・チャンネル数）を確かめる
    pub fn open(path: &Path) -> AppResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("ファイルが見つかりません: {}", path.display()))
            }
            _ => AppError::Io(format!("ファイルを開けません: {}", e)),
        })?;

        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(extension);
        }
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| AppError::Playback(format!("対応していない形式のファイルです: {}", e)))?;

        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| AppError::Playback("音声のトラックがありません".to_string()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or_else(|| AppError::Playback("音声の形式を読み取れません".to_string()))?;
        let decoder = codec_registry()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| AppError::Playback(format!("対応していないコーデックです: {}", e)))?;

        let declared_rate = params.sample_rate;
        let declared_channels = params.channels.as_ref().map(|channels| channels.count());
        let seconds_per_ts = track.time_base.map(f64::from).unwrap_or(0.0);
        let track_id = track.id;
        let codec_delay = i64::from(track.delay.unwrap_or(0));
        let declared_frames = track.num_frames;

        // MP4のAACは、エンコーダーの遅延とパディングがデコード結果に残る
        let gapless = (params.codec == CODEC_ID_AAC
            && format.format_info().short_name == ISOMP4_FORMAT_NAME)
            .then(|| mp4_gapless::read(path, declared_rate?))
            .flatten();

        let mut this = Self {
            path: path.to_path_buf(),
            format,
            decoder,
            track_id,
            sample_rate: declared_rate.unwrap_or(0),
            channels: declared_channels.unwrap_or(0),
            seconds_per_ts,
            offset: 0,
            total_frames: None,
            position: 0,
            start_at: Some(0),
            samples: Vec::new(),
            pending: 0..0,
            pending_ts: 0,
            primed: false,
            finished: false,
            exhausted: false,
            consecutive_errors: 0,
        };

        // 宣言された形式と、実際にデコードした形式が違う・宣言がないファイルがあるため、
        // 最初のパケットをデコードして確かめる（デコードした分は、最初の`next_chunk`で返す）
        let first_ts = this.decode_packet()?;
        if this.sample_rate == 0 || this.channels == 0 {
            return Err(AppError::Playback("音声のデータがありません".to_string()));
        }
        if this.seconds_per_ts <= 0.0 {
            this.seconds_per_ts = 1.0 / f64::from(this.sample_rate);
        }

        // ファイルの中の位置と、曲の頭からの位置の差
        let codec_offset = this.ts_to_frames(first_ts.unwrap_or(0)) + codec_delay;
        match gapless {
            Some(gapless) => {
                this.offset = codec_offset + gapless.delay as i64;
                this.total_frames = gapless.total_frames;
            }
            None => {
                this.offset = codec_offset;
                this.total_frames =
                    declared_frames.map(|frames| (frames as i64 - codec_offset).max(0) as u64);
            }
        }
        Ok(this)
    }

    /// サンプルレート（Hz）
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// チャンネル数
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// 曲の長さ（秒。分からなければNone）
    pub fn duration_seconds(&self) -> Option<f64> {
        self.total_frames
            .map(|frames| frames as f64 / f64::from(self.sample_rate))
    }

    /// まだ返していない音声の長さ（秒。曲の長さが分からなければNone）
    pub fn remaining_seconds(&self) -> Option<f64> {
        self.total_frames
            .map(|frames| frames.saturating_sub(self.position) as f64 / f64::from(self.sample_rate))
    }

    /// 次のかたまり（インターリーブのサンプル）を返す。曲の終わりではNone
    pub fn next_chunk(&mut self) -> AppResult<Option<&[f32]>> {
        self.next_chunk_limited(None)
    }

    /// 次のかたまりを、多くても`limit`フレームまで返す（残りは、次に呼んだ時に返す）
    ///
    /// 曲の中の決まった位置（クロスフェードを始める位置）で、かたまりを区切るために使う。
    pub fn next_chunk_limited(&mut self, limit: Option<u64>) -> AppResult<Option<&[f32]>> {
        loop {
            if self.finished || (self.pending.is_empty() && self.decode_packet()?.is_none()) {
                return Ok(None);
            }
            let mut range = std::mem::take(&mut self.pending);

            // 開いた直後・シークの直後: 返し始める位置より前のフレームを捨てる
            if let Some(start_at) = self.start_at {
                let chunk_start = self.ts_to_frames(self.pending_ts) - self.offset;
                let skipped = (start_at as i64 - chunk_start).max(0) as u64;
                if skipped >= range.len() as u64 {
                    continue;
                }
                range.start += skipped as usize;
                self.start_at = None;
                self.position = (chunk_start + skipped as i64).max(0) as u64;
            }

            // 曲の長さを超える分（終わりのパディング）は返さない
            if let Some(total) = self.total_frames {
                let remaining = total.saturating_sub(self.position);
                if remaining == 0 {
                    return Ok(None);
                }
                range.end = range.end.min(range.start + remaining as usize);
            }

            // 上限を超える分は、次に返す
            if let Some(limit) = limit
                && range.len() as u64 > limit.max(1)
            {
                let split = range.start + limit.max(1) as usize;
                self.pending = split..range.end;
                range.end = split;
            }

            self.position += range.len() as u64;
            return Ok(Some(
                &self.samples[range.start * self.channels..range.end * self.channels],
            ));
        }
    }

    /// 指定した位置（秒）へ移動し、移動した位置（秒）を返す
    ///
    /// 曲の長さを超える位置は、曲の終わりになる（次の`next_chunk`がNoneを返す）。
    pub fn seek(&mut self, seconds: f64) -> AppResult<f64> {
        let rate = f64::from(self.sample_rate);
        let target = (seconds.max(0.0) * rate).round() as u64;
        self.pending = 0..0;
        self.consecutive_errors = 0;
        if let Some(total) = self.total_frames
            && target >= total
        {
            self.finished = true;
            self.position = total;
            return Ok(total as f64 / rate);
        }

        // `symphonia`のMP4のリーダーは、ファイルを終わりまで読んだ後にシークすると続きを読めなく
        // なる。終わりまで読んだ後（曲の終わりが鳴っている間・短い曲）は、ファイルを開き直す
        if self.exhausted {
            *self = Self::open(&self.path.clone())?;
        }

        // ファイルの中の位置に直し、少し手前から読み始める
        let preroll = i64::from(self.sample_rate / SEEK_PREROLL_DIVISOR);
        let file_frames = (target as i64 + self.offset - preroll).max(0);
        let time = Time::try_from_secs_f64(file_frames as f64 / rate)
            .ok_or_else(|| AppError::Playback("再生位置が正しくありません".to_string()))?;
        match self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        ) {
            Ok(_) => {
                self.decoder.reset();
                self.finished = false;
                self.start_at = Some(target);
            }
            // 長さの分からないファイルで、終わりを超える位置を指定した場合
            Err(SymphoniaError::SeekError(_)) if target > 0 => self.finished = true,
            Err(e) => {
                return Err(AppError::Playback(format!(
                    "再生位置を移動できません: {}",
                    e
                )));
            }
        }
        self.position = target;
        Ok(target as f64 / rate)
    }

    /// タイムスタンプを、フレーム数に直す
    fn ts_to_frames(&self, ts: i64) -> i64 {
        (ts as f64 * self.seconds_per_ts * f64::from(self.sample_rate)).round() as i64
    }

    /// 次のパケットをデコードして`samples`に入れ、読んだ最初のパケットのタイムスタンプを返す
    /// （ファイルの終わりならNone）
    ///
    /// デコードしても音声が出ないパケット（Vorbisの最初のパケットなど）は読み飛ばすため、
    /// 返すタイムスタンプは、`samples`に入れたパケットのものとは限らない。
    fn decode_packet(&mut self) -> AppResult<Option<i64>> {
        let mut first_ts = None;
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                // 連結されたストリームの切れ目（`ResetRequired`）は、曲の終わりとして扱う
                Ok(None) | Err(SymphoniaError::ResetRequired) => {
                    self.exhausted = true;
                    return Ok(None);
                }
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.exhausted = true;
                    return Ok(None);
                }
                // コンテナを読み進められない（壊れた・途中で切れたファイル）: そこで曲を終える
                Err(SymphoniaError::DecodeError(message)) => {
                    log::warn!(
                        "ファイルの続きを読み取れないため、曲を終えます（{}フレーム目）: {}",
                        self.position,
                        message
                    );
                    self.exhausted = true;
                    return Ok(None);
                }
                Err(e) => {
                    return Err(AppError::Playback(format!(
                        "ファイルを読み取れません: {}",
                        e
                    )));
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            first_ts.get_or_insert(packet.pts.get());

            let buffer = match self.decoder.decode(&packet) {
                Ok(buffer) => buffer,
                Err(SymphoniaError::DecodeError(message)) => {
                    self.consecutive_errors += 1;
                    if self.consecutive_errors > MAX_CONSECUTIVE_DECODE_ERRORS {
                        return Err(AppError::Playback(format!(
                            "音声をデコードできません: {}",
                            message
                        )));
                    }
                    continue;
                }
                Err(e) => {
                    return Err(AppError::Playback(format!(
                        "音声をデコードできません: {}",
                        e
                    )));
                }
            };
            self.consecutive_errors = 0;

            let frames = buffer.frames();
            if frames == 0 {
                continue;
            }
            let spec = buffer.spec();
            let (rate, channels) = (spec.rate(), spec.channels().count());
            // デコーダーが頭を切った分（`trim_start`）だけ、先頭のフレームは後ろにある
            let mut trimmed = packet.trim_start.get() as i64;
            if !self.primed {
                // 最初のパケット: 実際の形式を採る
                self.primed = true;
                if rate != 0 {
                    self.sample_rate = rate;
                }
                self.channels = channels;
                // Opusのデコーダー（libopusのアダプター）は、最初にデコードするパケットの頭から
                // 遅延の分を自分で切り、パケットの`trim_start`には表れない。パケットの長さより
                // 短く出てきた分を、頭を切った分として数える
                let expected = packet.dur.get() as i64 - trimmed - packet.trim_end.get() as i64;
                trimmed += (expected - frames as i64).max(0);
            } else if channels != self.channels {
                return Err(AppError::Playback(
                    "曲の途中でチャンネル数が変わるファイルは再生できません".to_string(),
                ));
            }
            if self.channels == 0 || self.sample_rate == 0 {
                return Err(AppError::Playback("音声の形式を読み取れません".to_string()));
            }

            self.samples.resize(buffer.samples_interleaved(), 0.0);
            buffer.copy_to_slice_interleaved(&mut self.samples);
            self.pending = 0..frames;
            self.pending_ts = packet.pts.get() + trimmed;
            return Ok(first_ts);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// テスト用のWAV（16bit）を作る。サンプルの値は、フレームの位置（とチャンネル）から決まる
    pub(crate) fn write_wav(path: &Path, sample_rate: u32, channels: u16, frames: u32) {
        let samples: Vec<i16> = (0..frames)
            .flat_map(|frame| (0..channels).map(move |channel| wav_sample(frame, channel)))
            .collect();
        write_wav_samples(path, sample_rate, channels, &samples);
    }

    /// サンプル（インターリーブ）を、WAV（16bit）として書く
    pub(crate) fn write_wav_samples(path: &Path, sample_rate: u32, channels: u16, samples: &[i16]) {
        let data_len = samples.len() as u32 * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        File::create(path).unwrap().write_all(&bytes).unwrap();
    }

    /// `write_wav`が書く、フレームの位置ごとのサンプルの値
    pub(crate) fn wav_sample(frame: u32, channel: u16) -> i16 {
        ((frame % 20_000) as i16) * if channel == 0 { 1 } else { -1 }
    }

    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muspice-playback-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// デコーダーの残りをすべて読む
    fn read_all(decoder: &mut TrackDecoder) -> Vec<f32> {
        let mut samples = Vec::new();
        while let Some(chunk) = decoder.next_chunk().unwrap() {
            samples.extend_from_slice(chunk);
        }
        samples
    }

    fn expected(frame: u32, channel: u16) -> f32 {
        f32::from(wav_sample(frame, channel)) / 32768.0
    }

    #[test]
    fn test_decodes_all_frames_in_order() {
        let dir = temp_dir("decode");
        let path = dir.join("a.wav");
        write_wav(&path, 44_100, 2, 10_000);

        let mut decoder = TrackDecoder::open(&path).unwrap();
        assert_eq!(decoder.sample_rate(), 44_100);
        assert_eq!(decoder.channels(), 2);
        assert!((decoder.duration_seconds().unwrap() - 10_000.0 / 44_100.0).abs() < 1e-9);

        let samples = read_all(&mut decoder);
        assert_eq!(samples.len(), 10_000 * 2);
        for frame in [0u32, 1, 4_999, 9_999] {
            let index = frame as usize * 2;
            assert!((samples[index] - expected(frame, 0)).abs() < 1e-4);
            assert!((samples[index + 1] - expected(frame, 1)).abs() < 1e-4);
        }
        // 終わりの後は、何度呼んでもNone
        assert!(decoder.next_chunk().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_limited_chunks_split_at_the_given_frame() {
        let dir = temp_dir("limit");
        let path = dir.join("a.wav");
        write_wav(&path, 44_100, 2, 10_000);
        let mut decoder = TrackDecoder::open(&path).unwrap();

        // 上限までで区切り、残りは次に返す（合わせると、区切らない場合と同じ）
        let first = decoder.next_chunk_limited(Some(100)).unwrap().unwrap();
        assert_eq!(first.len(), 100 * 2);
        assert!((first[99 * 2] - expected(99, 0)).abs() < 1e-4);
        assert!((decoder.remaining_seconds().unwrap() - 9_900.0 / 44_100.0).abs() < 1e-9);

        let rest = read_all(&mut decoder);
        assert_eq!(rest.len(), 9_900 * 2);
        assert!((rest[0] - expected(100, 0)).abs() < 1e-4);
        assert_eq!(decoder.remaining_seconds(), Some(0.0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_seek_continues_from_the_exact_frame() {
        let dir = temp_dir("seek");
        let path = dir.join("a.wav");
        write_wav(&path, 48_000, 1, 48_000);

        let mut decoder = TrackDecoder::open(&path).unwrap();
        let position = decoder.seek(0.5).unwrap();
        assert!((position - 0.5).abs() < 1e-9);

        let samples = read_all(&mut decoder);
        assert_eq!(samples.len(), 24_000);
        assert!((samples[0] - expected(24_000, 0)).abs() < 1e-4);
        assert!((samples[23_999] - expected(47_999, 0)).abs() < 1e-4);

        // 読み終えた後も、頭へ戻って読み直せる
        assert_eq!(decoder.seek(0.0).unwrap(), 0.0);
        assert_eq!(read_all(&mut decoder).len(), 48_000);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_seek_past_the_end_ends_the_track() {
        let dir = temp_dir("seek-end");
        let path = dir.join("a.wav");
        write_wav(&path, 44_100, 2, 4_410);

        let mut decoder = TrackDecoder::open(&path).unwrap();
        let position = decoder.seek(10.0).unwrap();
        assert!((position - 0.1).abs() < 1e-9);
        assert!(decoder.next_chunk().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_open_reports_missing_and_unsupported_files() {
        let dir = temp_dir("open-error");
        let missing = TrackDecoder::open(&dir.join("missing.wav"));
        assert!(matches!(missing, Err(AppError::NotFound(_))));

        let broken = dir.join("broken.mp3");
        std::fs::write(&broken, b"this is not an audio file").unwrap();
        assert!(matches!(
            TrackDecoder::open(&broken),
            Err(AppError::Playback(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    // ---------- 非可逆の形式のファイル（曲の頭・終わりの余分と、シークの位置） ----------
    //
    // `tests/fixtures/playback/`のファイルは、`fixture_signal`の音源（0.5秒・モノラル）を
    // 各形式にエンコードしたもの。作り直すときは、次の手順で行う。
    //
    //   FIXTURE_SOURCE_DIR=<作業フォルダ> cargo test write_fixture_sources -- --ignored
    //   ffmpeg -i source44.wav -c:a libmp3lame -b:a 128k lame.mp3
    //   ffmpeg -i source44.wav -c:a aac -b:a 128k ffmpeg_aac.m4a
    //   afconvert -f m4af -d aac -b 128000 source44.wav apple_aac.m4a   （macOS）
    //   ffmpeg -i source44.wav -c:a vorbis -strict -2 -ac 2 vorbis.ogg
    //   ffmpeg -i source48.wav -c:a libopus -b:a 96k opus.opus

    /// フィクスチャの元の音源: 200Hzから4kHzへ上がる音にノイズを足したもの（頭から終わりまで音がある）
    fn fixture_signal(rate: u32) -> Vec<i16> {
        let mut noise = 0x2545_f491u32;
        (0..rate / 2)
            .map(|frame| {
                let seconds = f64::from(frame) / f64::from(rate);
                let chirp =
                    (2.0 * std::f64::consts::PI * (200.0 * seconds + 3_800.0 * seconds * seconds))
                        .sin();
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = f64::from(noise >> 16) / 32_768.0 - 1.0;
                ((chirp * 0.4 + white * 0.2) * 32_767.0) as i16
            })
            .collect()
    }

    /// フィクスチャの元の音源をWAVとして書き出す（フィクスチャを作り直すときに使う）
    #[test]
    #[ignore]
    fn write_fixture_sources() {
        let dir = PathBuf::from(std::env::var("FIXTURE_SOURCE_DIR").unwrap());
        write_wav_samples(
            &dir.join("source44.wav"),
            44_100,
            1,
            &fixture_signal(44_100),
        );
        write_wav_samples(
            &dir.join("source48.wav"),
            48_000,
            1,
            &fixture_signal(48_000),
        );
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/playback")
            .join(name)
    }

    /// デコーダーの残りをすべて読み、最初のチャンネルだけを返す
    fn read_first_channel(decoder: &mut TrackDecoder) -> Vec<f32> {
        let channels = decoder.channels();
        let mut samples = Vec::new();
        while let Some(chunk) = decoder.next_chunk().unwrap() {
            samples.extend(chunk.chunks_exact(channels).map(|frame| frame[0]));
        }
        samples
    }

    /// `signal`の`at`から始まる部分が、`reference`の`at`から何フレームずれた位置に最もよく
    /// 一致するか（0なら、ずれていない。正は、`signal`の方が遅れている）
    fn lag(reference: &[f32], signal: &[f32], at: usize) -> i64 {
        const WINDOW: usize = 2_048;
        // AACの遅延（2,112フレーム）より広い範囲を調べる
        const MAX_LAG: i64 = 2_400;
        // 1つおきのサンプルで比べる（デバッグビルドのテストを遅くしない）
        let window = || reference[at..at + WINDOW].iter().step_by(2);
        let score = |lag: i64| {
            let start = at as i64 + lag;
            if start < 0 || start as usize + WINDOW > signal.len() {
                return f32::MIN;
            }
            let other = || {
                signal[start as usize..start as usize + WINDOW]
                    .iter()
                    .step_by(2)
            };
            let dot: f32 = window().zip(other()).map(|(a, b)| a * b).sum();
            let energy: f32 = other().map(|a| a * a).sum();
            dot / energy.sqrt().max(f32::MIN_POSITIVE)
        };
        (-MAX_LAG..=MAX_LAG)
            .max_by(|a, b| score(*a).total_cmp(&score(*b)))
            .unwrap()
    }

    #[test]
    fn test_lossy_formats_start_and_end_at_the_original_samples() {
        // (ファイル, サンプルレート, 長さが元の音源とちょうど同じになるか)
        // FFmpegのVorbisのエンコーダーは、ファイルに書く長さが少し長くなる
        for (name, rate, exact_length) in [
            ("lame.mp3", 44_100, true),
            ("ffmpeg_aac.m4a", 44_100, true),
            ("apple_aac.m4a", 44_100, true),
            ("opus.opus", 48_000, true),
            ("vorbis.ogg", 44_100, false),
        ] {
            let source: Vec<f32> = fixture_signal(rate)
                .iter()
                .map(|&sample| f32::from(sample) / 32_768.0)
                .collect();
            let mut decoder = TrackDecoder::open(&fixture(name)).unwrap();
            assert_eq!(decoder.sample_rate(), rate, "{name}");
            let decoded = read_first_channel(&mut decoder);

            // エンコーダーの遅延とパディングが取り除かれ、元の音源と同じ位置に同じ音がある
            if exact_length {
                assert_eq!(decoded.len(), source.len(), "{name}: 長さ");
                assert_eq!(decoder.duration_seconds(), Some(0.5), "{name}: 曲の長さ");
            } else {
                assert!(decoded.len().abs_diff(source.len()) < 1_200, "{name}: 長さ");
            }
            assert_eq!(lag(&source, &decoded, 1_000), 0, "{name}: 頭");
            assert_eq!(
                lag(&source, &decoded, source.len() - 5_000),
                0,
                "{name}: 終わり"
            );

            // シークした位置から、元の音源のその位置の音が出る（終わりまで読んだ後でも）
            let position = decoder.seek(0.2).unwrap();
            assert!((position - 0.2).abs() < 1e-9, "{name}: シークした位置");
            let target = (0.2 * f64::from(rate)) as usize;
            let mut after_seek = vec![0.0; target];
            after_seek.extend(read_first_channel(&mut decoder));
            assert_eq!(
                after_seek.len(),
                decoded.len(),
                "{name}: シークした後の長さ"
            );
            assert_eq!(
                lag(&source, &after_seek, target + 500),
                0,
                "{name}: シークした後"
            );

            // 頭へ戻すと、最初に読んだときと同じ結果になる
            decoder.seek(0.0).unwrap();
            assert_eq!(
                read_first_channel(&mut decoder).len(),
                decoded.len(),
                "{name}: 頭へ戻した後の長さ"
            );
        }
    }
}
