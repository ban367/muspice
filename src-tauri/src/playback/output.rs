//! 音声の出力
//!
//! 出力デバイスの一覧の取得と、出力のストリームの作成を`cpal`で行う（macOSはCoreAudio）。
//! エンジンは`OutputBackend`を通して出力を開くため、テストでは音声デバイスのない環境でも、
//! 代わりの実装でエンジンを動かせる。

use super::render::Renderer;
use crate::error::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample};
use serde::Serialize;
use specta::Type;
use std::str::FromStr;
use std::time::Duration;

/// 出力のストリームの準備を待つ時間の上限（上限を扱うバックエンドだけに効く）
const STREAM_BUILD_TIMEOUT: Duration = Duration::from_secs(10);

/// 出力デバイス
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    /// デバイスを選ぶためのID（設定に保存する。接続し直しても変わらない）
    pub id: String,
    /// 表示名
    pub name: String,
    /// OSの既定の出力デバイスか
    pub is_default: bool,
}

/// 出力で起きた、エンジンに伝える出来事
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputEvent {
    /// 出力先が変わった・使えなくなった（出力を開き直す）
    DeviceChanged,
}

/// 出力を開く手段
pub trait OutputBackend: Send {
    /// 出力を開く（開いた直後は止まっている。`OutputHandle::start`で動かす）
    ///
    /// - `device_id`: 出力デバイスのID。Noneまたは見つからない場合は、OSの既定のデバイスを使う
    /// - `make_renderer`: 出力のサンプルレートを受け取り、音声を取り出す`Renderer`を作る
    /// - `on_event`: 出力先が変わったときなどに、出力のスレッドから呼ばれる
    fn open(
        &mut self,
        device_id: Option<&str>,
        make_renderer: &mut dyn FnMut(u32) -> Renderer,
        on_event: Box<dyn FnMut(OutputEvent) + Send>,
    ) -> AppResult<Box<dyn OutputHandle>>;
}

/// 開いた出力
pub trait OutputHandle {
    /// 出力を動かす（コールバックが呼ばれ始める）
    fn start(&mut self) -> AppResult<()>;
    /// 出力を止める（コールバックが呼ばれなくなる。止めている間は、OSのスリープを妨げない）
    fn stop(&mut self) -> AppResult<()>;
}

/// 出力デバイスの一覧を返す
pub fn list_output_devices() -> AppResult<Vec<OutputDevice>> {
    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .and_then(|device| device.id().ok());
    let devices = host
        .output_devices()
        .map_err(|e| AppError::Playback(format!("出力デバイスの一覧を取得できません: {}", e)))?;
    Ok(devices
        .filter_map(|device| {
            let id = device.id().ok()?;
            let name = device.description().ok()?.name().to_string();
            Some(OutputDevice {
                is_default: Some(&id) == default_id.as_ref(),
                id: id.to_string(),
                name,
            })
        })
        .collect())
}

/// `cpal`で出力を開く
pub struct CpalBackend;

impl OutputBackend for CpalBackend {
    fn open(
        &mut self,
        device_id: Option<&str>,
        make_renderer: &mut dyn FnMut(u32) -> Renderer,
        on_event: Box<dyn FnMut(OutputEvent) + Send>,
    ) -> AppResult<Box<dyn OutputHandle>> {
        let host = cpal::default_host();
        // 選んだデバイスが外れている場合は、既定のデバイスで鳴らす（設定は変えないため、
        // つなぎ直せば次に開くときからそのデバイスに戻る）
        let selected = device_id.and_then(|id| {
            let device = cpal::DeviceId::from_str(id)
                .ok()
                .and_then(|id| host.device_by_id(&id));
            if device.is_none() {
                log::warn!(
                    "選択した出力デバイスが見つからないため、既定のデバイスを使います: {}",
                    id
                );
            }
            device
        });
        let device = selected
            .or_else(|| host.default_output_device())
            .ok_or_else(|| AppError::Playback("出力デバイスが見つかりません".to_string()))?;

        let supported = device.default_output_config().map_err(|e| {
            AppError::Playback(format!("出力デバイスの設定を取得できません: {}", e))
        })?;
        let config = supported.config();
        let renderer = make_renderer(config.sample_rate);

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, config, renderer, on_event),
            SampleFormat::F64 => build_stream::<f64>(&device, config, renderer, on_event),
            SampleFormat::I8 => build_stream::<i8>(&device, config, renderer, on_event),
            SampleFormat::I16 => build_stream::<i16>(&device, config, renderer, on_event),
            SampleFormat::I24 => build_stream::<cpal::I24>(&device, config, renderer, on_event),
            SampleFormat::I32 => build_stream::<i32>(&device, config, renderer, on_event),
            SampleFormat::I64 => build_stream::<i64>(&device, config, renderer, on_event),
            SampleFormat::U8 => build_stream::<u8>(&device, config, renderer, on_event),
            SampleFormat::U16 => build_stream::<u16>(&device, config, renderer, on_event),
            SampleFormat::U24 => build_stream::<cpal::U24>(&device, config, renderer, on_event),
            SampleFormat::U32 => build_stream::<u32>(&device, config, renderer, on_event),
            SampleFormat::U64 => build_stream::<u64>(&device, config, renderer, on_event),
            format => Err(AppError::Playback(format!(
                "出力デバイスのサンプル形式（{}）には対応していません",
                format
            ))),
        }?;
        // 作った直後に動き始める環境があるため、止めておく
        let _ = stream.pause();
        Ok(Box::new(CpalOutput { stream }))
    }
}

/// デバイスのサンプル形式`T`で出力のストリームを作る
fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut renderer: Renderer,
    mut on_event: Box<dyn FnMut(OutputEvent) + Send>,
) -> AppResult<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    // f32で取り出してから、デバイスの形式へ変換する（確保は最初のコールバックだけ）
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                scratch.resize(data.len(), 0.0);
                renderer.render(&mut scratch, channels);
                for (out, sample) in data.iter_mut().zip(&scratch) {
                    *out = T::from_sample(*sample);
                }
            },
            move |error: cpal::Error| match error.kind() {
                // 出力先が変わった・デバイスが外れた・設定が無効になった: 出力を開き直す
                ErrorKind::DeviceChanged
                | ErrorKind::DeviceNotAvailable
                | ErrorKind::StreamInvalidated => {
                    log::info!("出力デバイスが変わりました: {}", error);
                    on_event(OutputEvent::DeviceChanged);
                }
                // 音切れ（バッファの不足）は、続けて再生できる
                ErrorKind::Xrun => {}
                _ => log::warn!("音声の出力でエラーが発生しました: {}", error),
            },
            Some(STREAM_BUILD_TIMEOUT),
        )
        .map_err(|e| AppError::Playback(format!("音声の出力を開始できません: {}", e)))
}

struct CpalOutput {
    stream: cpal::Stream,
}

impl OutputHandle for CpalOutput {
    fn start(&mut self) -> AppResult<()> {
        self.stream
            .play()
            .map_err(|e| AppError::Playback(format!("音声の出力を開始できません: {}", e)))
    }

    fn stop(&mut self) -> AppResult<()> {
        self.stream
            .pause()
            .map_err(|e| AppError::Playback(format!("音声の出力を停止できません: {}", e)))
    }
}
