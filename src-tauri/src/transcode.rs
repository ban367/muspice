//! 形式の変換（曲をデコードして、別の形式にエンコードする）
//!
//! 選んだ曲を、AAC・ALAC・FLAC・WAV・AIFFに変換する（ADR-054）。
//!
//! - デコードは、再生エンジンのデコーダー（`TrackDecoder`）で行う。取り込める形式のすべてを
//!   変換元にでき、再生で聴く音（曲の頭・終わりの余分を除いたもの）が変換される
//! - エンコードは、macOSに入っている`afconvert`（AudioToolbox）で行う。デコードした音声を
//!   一時ファイル（CAF）に書き、それを変換先の形式にする。macOSだけで使える
//! - 可逆の形式の曲は、元と同じビット数（16か24）で渡す。非可逆の形式の曲は、16ビットで渡す
//! - 3チャンネル以上の曲は、再生と同じまとめ方でステレオにする
//! - タグ・評価・ゲインとピーク・アルバムアートは、変換した後に写す（`metadata::copy_file_tags`）

use crate::error::{AppError, AppResult};
use crate::playback::{TrackDecoder, append_stereo};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// エンコードに使う、OSのコマンド
const AFCONVERT: &str = "/usr/bin/afconvert";

/// 選べるAACのビットレート（kbps。ステレオでの値。モノラルは半分にする）
pub const AAC_BITRATES_KBPS: [u32; 4] = [128, 192, 256, 320];

/// 既定のAACのビットレート（kbps）
pub const DEFAULT_AAC_BITRATE_KBPS: u32 = 256;

/// エンコードが終わるのを待つ間に、中断を確かめる間隔
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 変換先の形式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConvertFormat {
    /// AAC（`.m4a`。非可逆）
    Aac,
    /// Apple Lossless（`.m4a`。可逆）
    Alac,
    Flac,
    Wav,
    Aiff,
}

impl ConvertFormat {
    /// 変換後のファイルの拡張子
    pub fn extension(self) -> &'static str {
        match self {
            Self::Aac | Self::Alac => "m4a",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
        }
    }
}

/// この環境で、形式を変換できるか（macOSで、`afconvert`がある場合）
pub fn is_available() -> bool {
    cfg!(target_os = "macos") && Path::new(AFCONVERT).is_file()
}

/// 曲が可逆の形式か（拡張子から決める。`.m4a`・`.ogg`は、中のコーデックを読む。分からなければNone）
pub fn is_lossless_source(path: &Path) -> Option<bool> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "flac" | "wav" | "aiff" | "aif" | "wv" | "ape" => Some(true),
        "mp3" | "mp2" | "aac" | "opus" => Some(false),
        // Oggの中は、Vorbis・Opus（非可逆）か、FLAC（可逆）
        "ogg" => {
            let mut file = File::open(path).ok()?;
            crate::ogg_flac::is_ogg_flac(&mut file).ok()
        }
        "m4a" | "mp4" | "m4b" => {
            use lofty::file::AudioFile;
            use lofty::mp4::{Mp4Codec, Mp4File};
            let mut file = File::open(path).ok()?;
            let options = lofty::config::ParseOptions::new().read_cover_art(false);
            let mp4 = Mp4File::read_from(&mut file, options).ok()?;
            let codec = mp4.properties().codec()?;
            Some(matches!(codec, Mp4Codec::ALAC | Mp4Codec::FLAC))
        }
        _ => None,
    }
}

/// 変換後の長さが、元とこれ以上違えば、失敗として扱う（秒）
///
/// AACは、エンコーダーの都合で、頭・終わりに余分が付く（再生では取り除く）。その分の誤差を見込む。
const DURATION_TOLERANCE_SECONDS: f64 = 0.05;

/// 一時ファイルに書いた音声の形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pcm {
    channels: usize,
    /// 1サンプルのビット数（16か24）
    bits: u32,
    sample_rate: u32,
    /// 書いたフレームの数
    frames: u64,
}

/// 一時ファイル（CAF）のヘッダーを書く
///
/// CAFは、データの大きさを後から書き直さなくてよい（大きさに-1を書くと、ファイルの終わりまでが
/// データになる）ため、曲の長さが分かる前から書き始められる。4GBを超える音声も入る。
fn write_caf_header(out: &mut impl Write, sample_rate: u32, pcm: Pcm) -> std::io::Result<()> {
    /// サンプルが、下位のバイトから並ぶ（リトルエンディアン）
    const FLAG_LITTLE_ENDIAN: u32 = 2;
    let bytes_per_frame = pcm.bits / 8 * pcm.channels as u32;

    out.write_all(b"caff")?;
    out.write_all(&1u16.to_be_bytes())?;
    out.write_all(&0u16.to_be_bytes())?;
    // 音声の形式
    out.write_all(b"desc")?;
    out.write_all(&32i64.to_be_bytes())?;
    out.write_all(&f64::from(sample_rate).to_be_bytes())?;
    out.write_all(b"lpcm")?;
    out.write_all(&FLAG_LITTLE_ENDIAN.to_be_bytes())?;
    out.write_all(&bytes_per_frame.to_be_bytes())?;
    out.write_all(&1u32.to_be_bytes())?;
    out.write_all(&(pcm.channels as u32).to_be_bytes())?;
    out.write_all(&pcm.bits.to_be_bytes())?;
    // 音声のデータ（大きさは、ファイルの終わりまで）
    out.write_all(b"data")?;
    out.write_all(&(-1i64).to_be_bytes())?;
    out.write_all(&0u32.to_be_bytes())
}

/// サンプル（-1〜1）を、整数（リトルエンディアン）にして書く
fn write_samples(out: &mut impl Write, samples: &[f32], bits: u32) -> std::io::Result<()> {
    let scale = 2f64.powi(bits as i32 - 1);
    let bytes = (bits / 8) as usize;
    for sample in samples {
        // 非可逆の形式をデコードした音声は、範囲を少し超えることがある
        let value = (f64::from(*sample) * scale)
            .round()
            .clamp(-scale, scale - 1.0) as i32;
        out.write_all(&value.to_le_bytes()[..bytes])?;
    }
    Ok(())
}

/// 曲をデコードして、一時ファイル（CAF）に書く
///
/// @returns 書いた音声の形式（中断した場合はNone）
fn decode_to_caf(source: &Path, temp: &Path, cancelled: &AtomicBool) -> AppResult<Option<Pcm>> {
    let io_error =
        |e: std::io::Error| AppError::Io(format!("一時ファイルに書き込めませんでした: {}", e));
    let mut decoder = TrackDecoder::open(source)?;
    let source_channels = decoder.channels();
    if source_channels == 0 || decoder.sample_rate() == 0 {
        return Err(AppError::Playback(
            "音声の形式が正しくありません".to_string(),
        ));
    }
    let mut pcm = Pcm {
        channels: source_channels.min(2),
        bits: match decoder.bits_per_sample() {
            Some(bits) if bits > 16 => 24,
            _ => 16,
        },
        sample_rate: decoder.sample_rate(),
        frames: 0,
    };

    let mut out = BufWriter::new(File::create(temp).map_err(io_error)?);
    write_caf_header(&mut out, decoder.sample_rate(), pcm).map_err(io_error)?;
    let mut stereo: Vec<f32> = Vec::new();
    while let Some(chunk) = decoder.next_chunk()? {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        pcm.frames += (chunk.len() / source_channels) as u64;
        if source_channels > 2 {
            stereo.clear();
            append_stereo(chunk, source_channels, &mut stereo);
            write_samples(&mut out, &stereo, pcm.bits).map_err(io_error)?;
        } else {
            write_samples(&mut out, chunk, pcm.bits).map_err(io_error)?;
        }
    }
    out.flush().map_err(io_error)?;
    Ok(Some(pcm))
}

/// `afconvert`に渡す、変換先の形式の指定
///
/// @param aac_bitrate - AACのビットレート（bps。Noneは、エンコーダーの既定の値）
fn encoder_args(format: ConvertFormat, pcm: Pcm, aac_bitrate: Option<u32>) -> Vec<String> {
    let arg = |value: &str| value.to_string();
    match format {
        ConvertFormat::Aac => {
            let mut args = vec![
                arg("-f"),
                arg("m4af"),
                arg("-d"),
                arg("aac"),
                arg("-q"),
                arg("127"),
            ];
            if let Some(bitrate) = aac_bitrate {
                // 音の複雑さに合わせて、指定したビットレートの前後で変える（制限付きのVBR）
                args.extend([arg("-b"), bitrate.to_string(), arg("-s"), arg("2")]);
            }
            args
        }
        ConvertFormat::Alac => vec![arg("-f"), arg("m4af"), arg("-d"), arg("alac")],
        ConvertFormat::Flac => vec![arg("-f"), arg("flac"), arg("-d"), arg("flac")],
        ConvertFormat::Wav => vec![
            arg("-f"),
            arg("WAVE"),
            arg("-d"),
            format!("LEI{}", pcm.bits),
        ],
        ConvertFormat::Aiff => vec![
            arg("-f"),
            arg("AIFF"),
            arg("-d"),
            format!("BEI{}", pcm.bits),
        ],
    }
}

/// `afconvert`を実行する
///
/// 標準エラー出力は、実行中から別のスレッドで読み取る（終わってから読むと、出力が多い時に
/// パイプが一杯になり、エンコーダーが書き込みを待ったまま終わらなくなる）。
/// @returns 終わったか（中断した場合はfalse）
fn run_afconvert(
    args: &[String],
    input: &Path,
    output: &Path,
    cancelled: &AtomicBool,
) -> AppResult<bool> {
    let mut child = Command::new(AFCONVERT)
        .args(args)
        .arg(input)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Io(format!("エンコーダーを起動できませんでした: {}", e)))?;
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut message = Vec::new();
            stderr.read_to_end(&mut message).ok();
            String::from_utf8_lossy(&message).into_owned()
        })
    });

    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(true),
            Ok(Some(_)) => {
                let message = stderr_reader
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                let reason = message
                    .lines()
                    .next()
                    .unwrap_or("理由は分かりません")
                    .trim();
                return Err(AppError::Playback(format!(
                    "エンコードに失敗しました: {}",
                    reason
                )));
            }
            Ok(None) => {}
            Err(e) => {
                return Err(AppError::Io(format!(
                    "エンコーダーの終了を待てませんでした: {}",
                    e
                )));
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            child.kill().ok();
            child.wait().ok();
            return Ok(false);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// 一時ファイルを、変換先の形式にエンコードする
///
/// AACのビットレートは、チャンネル数・サンプルレートによって、使える値が違う。指定した値で
/// エンコードできなければ、エンコーダーの既定の値でやり直す。
/// @returns 終わったか（中断した場合はfalse）
fn encode(
    input: &Path,
    output: &Path,
    format: ConvertFormat,
    aac_bitrate_kbps: u32,
    pcm: Pcm,
    cancelled: &AtomicBool,
) -> AppResult<bool> {
    let bitrate = aac_bitrate_kbps * 1000 / if pcm.channels == 1 { 2 } else { 1 };
    match run_afconvert(
        &encoder_args(format, pcm, Some(bitrate)),
        input,
        output,
        cancelled,
    ) {
        Err(_) if format == ConvertFormat::Aac => {
            fs::remove_file(output).ok();
            run_afconvert(&encoder_args(format, pcm, None), input, output, cancelled)
        }
        result => result,
    }
}

/// 変換後のファイルを開いて、元の曲と同じ長さの音声が入っているかを確かめる
///
/// エンコーダーが、音声を落としたファイルを作ることがある（macOSのFLACのエンコーダーは、
/// 1ブロック（4608フレーム。44.1kHzで約0.1秒）より短い音声から、音声のないファイルを作る）。
/// 置き換えで、そのようなファイルに入れ替えないよう、長さが合わなければ失敗として扱う。
fn verify_output(output: &Path, pcm: Pcm) -> AppResult<()> {
    let mismatch = || {
        AppError::Playback(
            "変換後のファイルの長さが、元の曲と合いません（とても短い曲は、FLACに変換できません）"
                .to_string(),
        )
    };
    let expected = pcm.frames as f64 / f64::from(pcm.sample_rate);
    let actual = TrackDecoder::open(output)
        .map_err(|_| mismatch())?
        .duration_seconds()
        .ok_or_else(mismatch)?;
    if (actual - expected).abs() > DURATION_TOLERANCE_SECONDS {
        return Err(mismatch());
    }
    Ok(())
}

/// 破棄する時に、ファイルを消す（一時ファイル・途中まで書いた出力を、残さない）
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        fs::remove_file(&self.0).ok();
    }
}

/// 1曲を変換して、`output`に書く（タグ・アルバムアートも写す）
///
/// `output`にすでにファイルがある場合は、上書きする（呼び出し側が、変換先の場所ではない、
/// 一時的な名前を渡す）。失敗・中断した場合は、`output`を残さない。
/// @param temp_dir - デコードした音声の一時ファイルを置くフォルダ
/// @returns 変換したか（中断した場合はfalse）
pub fn convert_file(
    source: &Path,
    output: &Path,
    format: ConvertFormat,
    aac_bitrate_kbps: u32,
    temp_dir: &Path,
    cancelled: &AtomicBool,
) -> AppResult<bool> {
    if !is_available() {
        return Err(AppError::Playback(
            "この環境では、形式を変換できません".to_string(),
        ));
    }
    fs::create_dir_all(temp_dir)
        .map_err(|e| AppError::Io(format!("一時フォルダを作れませんでした: {}", e)))?;
    let temp = RemoveOnDrop(temp_dir.join(format!("{}.caf", uuid::Uuid::new_v4())));
    let Some(pcm) = decode_to_caf(source, &temp.0, cancelled)? else {
        return Ok(false);
    };

    // 失敗・中断した時に、途中まで書いた出力を残さない
    let partial = RemoveOnDrop(output.to_path_buf());
    if !encode(&temp.0, output, format, aac_bitrate_kbps, pcm, cancelled)? {
        return Ok(false);
    }
    verify_output(output, pcm)?;
    crate::metadata::copy_file_tags(source, output)?;
    std::mem::forget(partial);
    Ok(true)
}

/// 変換したファイルの出力先
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConvertMode {
    /// 別のフォルダへ書き出す（ライブラリの曲は変えない）
    Export,
    /// 元のファイルを置き換える（曲のIDを保ったまま、ファイルと形式を入れ替える）
    Replace,
}

/// 曲を変換するか、変換しない理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConvertStatus {
    /// 変換する
    Convert,
    /// ファイルが見つからない
    Missing,
    /// 出力先に、同じ名前のファイルがある
    TargetExists,
    /// 選んだ曲の中に、同じ出力先になる曲がある
    SameTarget,
}

/// 1曲の計画（一覧に表示する）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConvertEntry {
    pub track_id: String,
    /// 元のファイルの名前
    pub source_name: String,
    /// 出力先（書き出しは、書き出すフォルダからの場所。置き換えは、ファイルの名前。決まらない曲はNone）
    pub output_path: Option<String>,
    pub status: ConvertStatus,
    /// 元の曲が可逆の形式か（分からなければNone）
    pub source_lossless: Option<bool>,
}

/// 1曲の変換（実行用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedConversion {
    pub track_id: String,
    pub source: PathBuf,
    pub output: PathBuf,
}

/// 計画を立てるための指定
pub struct PlanOptions<'a> {
    pub format: ConvertFormat,
    pub mode: ConvertMode,
    /// 書き出す場所の書式（書き出しだけ）
    pub template: &'a crate::file_organize::Template,
    /// 書き出すフォルダ（書き出しで、まだ選んでいなければNone。その間は、同じ名前のファイルを調べない）
    pub export_folder: Option<&'a Path>,
    /// ライブラリに登録されている、すべての曲のパス（置き換えで、ほかの曲の場所を出力先にしない）
    pub registered_paths: &'a std::collections::HashSet<String>,
}

/// 選んだ曲の、変換の計画を立てる
///
/// - 書き出し: 出力先は、書き出すフォルダの中の、書式に従った場所（拡張子は、変換先の形式）
/// - 置き換え: 出力先は、元のファイルと同じ場所・同じ名前（拡張子は、変換先の形式）
/// - 出力先に同じ名前のファイルがある曲・選んだ曲の中で同じ出力先になる曲は、変換しない
///   （上書きしない・連番も付けない。ファイルの整理と同じ）
pub fn plan(
    tracks: &[crate::models::Track],
    options: &PlanOptions<'_>,
) -> (Vec<ConvertEntry>, Vec<PlannedConversion>) {
    use crate::file_organize::location_key;
    use std::collections::{HashMap, HashSet};

    let extension = options.format.extension();
    let mut entries: Vec<ConvertEntry> = Vec::with_capacity(tracks.len());
    // (entriesの中の位置, 変換)
    let mut candidates: Vec<(usize, PlannedConversion)> = Vec::new();

    for track in tracks {
        let source = PathBuf::from(&track.file_path);
        let mut entry = ConvertEntry {
            track_id: track.id.clone(),
            source_name: track.file_name.clone(),
            output_path: None,
            status: ConvertStatus::Convert,
            source_lossless: is_lossless_source(&source),
        };
        if track.is_missing || !source.is_file() {
            entry.status = ConvertStatus::Missing;
            entries.push(entry);
            continue;
        }

        let (output, display) = match options.mode {
            ConvertMode::Export => {
                let relative: PathBuf = options
                    .template
                    .components_with_extension(track, &source, Some(extension))
                    .iter()
                    .collect();
                let output = options
                    .export_folder
                    .map_or_else(|| relative.clone(), |folder| folder.join(&relative));
                (output, relative.to_string_lossy().into_owned())
            }
            ConvertMode::Replace => {
                let output = source.with_extension(extension);
                let name = output
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (output, name)
            }
        };
        entry.output_path = Some(display);
        candidates.push((
            entries.len(),
            PlannedConversion {
                track_id: track.id.clone(),
                source,
                output,
            },
        ));
        entries.push(entry);
    }

    let mut users: HashMap<String, HashSet<usize>> = HashMap::new();
    for (index, planned) in &candidates {
        users
            .entry(location_key(&planned.output))
            .or_default()
            .insert(*index);
    }

    let mut conversions = Vec::with_capacity(candidates.len());
    for (index, planned) in candidates {
        let key = location_key(&planned.output);
        let shared = users.get(&key).is_some_and(|users| users.len() > 1);
        // 置き換えで、拡張子が同じ（出力先が、元のファイル自身）場合は、ふさがっているとみなさない
        let replaces_itself =
            options.mode == ConvertMode::Replace && key == location_key(&planned.source);
        let checks_disk = options.mode == ConvertMode::Replace || options.export_folder.is_some();
        let occupied = !replaces_itself
            && checks_disk
            && (fs::symlink_metadata(&planned.output).is_ok()
                || (options.mode == ConvertMode::Replace
                    && planned
                        .output
                        .to_str()
                        .is_some_and(|path| options.registered_paths.contains(path))));

        entries[index].status = if shared {
            ConvertStatus::SameTarget
        } else if occupied {
            ConvertStatus::TargetExists
        } else {
            conversions.push(planned);
            ConvertStatus::Convert
        };
    }
    (entries, conversions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_extensions() {
        assert_eq!(ConvertFormat::Aac.extension(), "m4a");
        assert_eq!(ConvertFormat::Alac.extension(), "m4a");
        assert_eq!(ConvertFormat::Flac.extension(), "flac");
        assert_eq!(ConvertFormat::Wav.extension(), "wav");
        assert_eq!(ConvertFormat::Aiff.extension(), "aiff");
        assert!(AAC_BITRATES_KBPS.contains(&DEFAULT_AAC_BITRATE_KBPS));
    }

    #[test]
    fn test_lossless_sources_are_known_by_extension_and_codec() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        assert_eq!(is_lossless_source(Path::new("/music/a.FLAC")), Some(true));
        assert_eq!(is_lossless_source(Path::new("/music/a.wv")), Some(true));
        assert_eq!(is_lossless_source(Path::new("/music/a.mp3")), Some(false));
        assert_eq!(is_lossless_source(Path::new("/music/a.opus")), Some(false));
        // `.m4a`は、中のコーデックで決まる
        assert_eq!(
            is_lossless_source(&fixtures.join("apple_aac.m4a")),
            Some(false)
        );
        assert_eq!(is_lossless_source(Path::new("/music/missing.m4a")), None);
        // `.ogg`は、中がFLACなら可逆
        assert_eq!(
            is_lossless_source(&fixtures.join("vorbis.ogg")),
            Some(false)
        );
        assert_eq!(is_lossless_source(&fixtures.join("opus.opus")), Some(false));
        assert_eq!(
            is_lossless_source(&fixtures.join("ogg_flac.ogg")),
            Some(true)
        );
        assert_eq!(is_lossless_source(Path::new("/music/missing.ogg")), None);
        assert_eq!(is_lossless_source(Path::new("/music/a.txt")), None);
        assert_eq!(is_lossless_source(Path::new("/music/noext")), None);
    }

    #[test]
    fn test_caf_header_and_samples() {
        let mut bytes = Vec::new();
        let pcm = Pcm {
            channels: 2,
            bits: 24,
            sample_rate: 48_000,
            frames: 0,
        };
        write_caf_header(&mut bytes, 48_000, pcm).unwrap();
        assert_eq!(&bytes[..4], b"caff");
        assert_eq!(&bytes[8..12], b"desc");
        assert_eq!(bytes[20..28], 48_000f64.to_be_bytes());
        assert_eq!(&bytes[28..32], b"lpcm");
        // 1フレームのバイト数（3バイト × 2チャンネル）・チャンネル数・ビット数
        assert_eq!(bytes[36..40], 6u32.to_be_bytes());
        assert_eq!(bytes[44..48], 2u32.to_be_bytes());
        assert_eq!(bytes[48..52], 24u32.to_be_bytes());
        assert_eq!(&bytes[52..56], b"data");
        assert_eq!(bytes[56..64], (-1i64).to_be_bytes());
        assert_eq!(bytes.len(), 68);

        // 16ビット: 範囲の外の値は、端に丸める
        let mut samples = Vec::new();
        write_samples(&mut samples, &[0.0, 0.5, -1.0, 1.0, 1.5, -2.0], 16).unwrap();
        let values: Vec<i16> = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| i16::from_le_bytes(*bytes))
            .collect();
        assert_eq!(values, [0, 16384, -32768, 32767, 32767, -32768]);
        // 24ビット: 3バイトずつ書く
        let mut samples = Vec::new();
        write_samples(&mut samples, &[0.5, -1.0], 24).unwrap();
        assert_eq!(samples, [0x00, 0x00, 0x40, 0x00, 0x00, 0x80]);
    }

    #[test]
    fn test_encoder_args() {
        let stereo = Pcm {
            channels: 2,
            bits: 24,
            sample_rate: 44_100,
            frames: 0,
        };
        assert_eq!(
            encoder_args(ConvertFormat::Aac, stereo, Some(256_000)).join(" "),
            "-f m4af -d aac -q 127 -b 256000 -s 2"
        );
        assert_eq!(
            encoder_args(ConvertFormat::Aac, stereo, None).join(" "),
            "-f m4af -d aac -q 127"
        );
        assert_eq!(
            encoder_args(ConvertFormat::Alac, stereo, None).join(" "),
            "-f m4af -d alac"
        );
        assert_eq!(
            encoder_args(ConvertFormat::Flac, stereo, None).join(" "),
            "-f flac -d flac"
        );
        assert_eq!(
            encoder_args(ConvertFormat::Wav, stereo, None).join(" "),
            "-f WAVE -d LEI24"
        );
        let mono16 = Pcm {
            channels: 1,
            bits: 16,
            ..stereo
        };
        assert_eq!(
            encoder_args(ConvertFormat::Aiff, mono16, None).join(" "),
            "-f AIFF -d BEI16"
        );
    }

    mod planning {
        use super::*;
        use crate::device_manifest::test_support::TempDir;
        use crate::file_organize::Template;
        use crate::models::{ReplayGain, Track};
        use std::collections::HashSet;

        fn track(id: &str, path: &Path) -> Track {
            Track {
                id: id.to_string(),
                file_path: path.to_str().unwrap().to_string(),
                file_name: path.file_name().unwrap().to_str().unwrap().to_string(),
                title: Some("Song".to_string()),
                artist: Some("Artist".to_string()),
                album: Some("Album".to_string()),
                album_artist: None,
                genre: None,
                year: None,
                track_number: Some(1),
                disc_number: None,
                duration: Some(200),
                file_size: 4,
                format: "wav".to_string(),
                bitrate: None,
                sample_rate: None,
                is_favorite: false,
                rating: 0,
                play_count: 0,
                skip_count: 0,
                last_played_at: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                replay_gain: ReplayGain::default(),
                sort_tags: Default::default(),
                is_missing: false,
            }
        }

        fn touch(path: &Path) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"data").unwrap();
        }

        fn plan_with(
            tracks: &[Track],
            format: ConvertFormat,
            mode: ConvertMode,
            export_folder: Option<&Path>,
        ) -> (Vec<ConvertEntry>, Vec<PlannedConversion>) {
            let template = Template::parse("%albumartist%/%album%/%track% %title%").unwrap();
            let registered: HashSet<String> = tracks.iter().map(|t| t.file_path.clone()).collect();
            plan(
                tracks,
                &PlanOptions {
                    format,
                    mode,
                    template: &template,
                    export_folder,
                    registered_paths: &registered,
                },
            )
        }

        fn statuses(entries: &[ConvertEntry]) -> Vec<ConvertStatus> {
            entries.iter().map(|entry| entry.status).collect()
        }

        #[test]
        fn test_export_places_files_by_the_template() {
            let dir = TempDir::new();
            let source = dir.path().join("music/a.wav");
            touch(&source);
            let export = dir.path().join("export");
            let tracks = [track("t1", &source)];

            // フォルダを選ぶ前: 書き出すフォルダからの場所だけが決まる
            let (entries, _) = plan_with(&tracks, ConvertFormat::Aac, ConvertMode::Export, None);
            let relative = Path::new("Artist").join("Album").join("01 Song.m4a");
            assert_eq!(
                entries,
                [ConvertEntry {
                    track_id: "t1".to_string(),
                    source_name: "a.wav".to_string(),
                    output_path: Some(relative.display().to_string()),
                    status: ConvertStatus::Convert,
                    source_lossless: Some(true),
                }]
            );

            // フォルダを選んだ後: 出力先が決まり、同じ名前のファイルがあれば変換しない
            let (_, conversions) = plan_with(
                &tracks,
                ConvertFormat::Aac,
                ConvertMode::Export,
                Some(&export),
            );
            assert_eq!(
                conversions,
                [PlannedConversion {
                    track_id: "t1".to_string(),
                    source: source.clone(),
                    output: export.join(&relative),
                }]
            );
            touch(&export.join(&relative));
            let (entries, conversions) = plan_with(
                &tracks,
                ConvertFormat::Aac,
                ConvertMode::Export,
                Some(&export),
            );
            assert_eq!(statuses(&entries), [ConvertStatus::TargetExists]);
            assert!(conversions.is_empty());
        }

        #[test]
        fn test_replace_keeps_the_place_and_name() {
            let dir = TempDir::new();
            let wav = dir.path().join("a.wav");
            let flac = dir.path().join("b.flac");
            touch(&wav);
            touch(&flac);

            let (entries, conversions) = plan_with(
                &[track("t1", &wav), track("t2", &flac)],
                ConvertFormat::Flac,
                ConvertMode::Replace,
                None,
            );

            assert_eq!(
                statuses(&entries),
                [ConvertStatus::Convert, ConvertStatus::Convert]
            );
            assert_eq!(entries[0].output_path.as_deref(), Some("a.flac"));
            assert_eq!(conversions[0].output, dir.path().join("a.flac"));
            // 同じ形式への置き換えは、出力先が元のファイル自身（ふさがっているとみなさない）
            assert_eq!(conversions[1].output, flac);
        }

        #[test]
        fn test_tracks_that_cannot_be_converted_are_reported() {
            let dir = TempDir::new();
            let wav = dir.path().join("a.wav");
            let aiff = dir.path().join("a.aiff");
            let blocked = dir.path().join("b.wav");
            let gone = dir.path().join("gone.wav");
            for path in [&wav, &aiff, &blocked] {
                touch(path);
            }
            // 置き換えの出力先に、ライブラリにないファイルがある
            touch(&dir.path().join("b.flac"));
            let mut marked = track("t5", &blocked);
            marked.id = "t5".to_string();
            marked.is_missing = true;

            let (entries, conversions) = plan_with(
                &[
                    track("t1", &wav),
                    track("t2", &aiff),
                    track("t3", &blocked),
                    track("t4", &gone),
                    marked,
                ],
                ConvertFormat::Flac,
                ConvertMode::Replace,
                None,
            );

            assert_eq!(
                statuses(&entries),
                [
                    // `a.wav`と`a.aiff`は、どちらも`a.flac`になる
                    ConvertStatus::SameTarget,
                    ConvertStatus::SameTarget,
                    ConvertStatus::TargetExists,
                    ConvertStatus::Missing,
                    ConvertStatus::Missing,
                ]
            );
            assert!(conversions.is_empty());
            assert_eq!(entries[3].output_path, None);
        }

        #[test]
        fn test_replace_avoids_paths_registered_for_other_tracks() {
            let dir = TempDir::new();
            let wav = dir.path().join("a.wav");
            touch(&wav);
            let template = Template::parse("%title%").unwrap();
            // ファイルはないが、見つからない曲として登録されている場所
            let registered =
                HashSet::from([dir.path().join("a.flac").to_str().unwrap().to_string()]);

            let (entries, _) = plan(
                &[track("t1", &wav)],
                &PlanOptions {
                    format: ConvertFormat::Flac,
                    mode: ConvertMode::Replace,
                    template: &template,
                    export_folder: None,
                    registered_paths: &registered,
                },
            );

            assert_eq!(statuses(&entries), [ConvertStatus::TargetExists]);
        }
    }

    /// 実際にエンコードする確認（`afconvert`のあるmacOSだけ）
    #[cfg(target_os = "macos")]
    mod encoding {
        use super::*;
        use crate::device_manifest::test_support::TempDir;
        use crate::metadata::{
            EmbeddedPicture, extract_album_art, extract_all_file_info, read_file_tags,
            set_file_album_art, update_file_metadata, update_file_rating, update_file_replay_gain,
        };
        use crate::models::{Metadata, ReplayGain};
        use crate::playback::test_audio::write_wav_samples;

        fn fixture(name: &str) -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/playback")
                .join(name)
        }

        /// ファイルをデコードする（サンプルレート・チャンネル数・ビット数・サンプル）
        fn decode(path: &Path) -> (u32, usize, Option<u32>, Vec<f32>) {
            let mut decoder = TrackDecoder::open(path).unwrap();
            let mut samples = Vec::new();
            while let Some(chunk) = decoder.next_chunk().unwrap() {
                samples.extend_from_slice(chunk);
            }
            (
                decoder.sample_rate(),
                decoder.channels(),
                decoder.bits_per_sample(),
                samples,
            )
        }

        fn convert(source: &Path, output: &Path, format: ConvertFormat) -> AppResult<bool> {
            convert_file(
                source,
                output,
                format,
                DEFAULT_AAC_BITRATE_KBPS,
                &output.parent().unwrap().join("temp"),
                &AtomicBool::new(false),
            )
        }

        #[test]
        fn test_lossless_formats_keep_every_sample() {
            let dir = TempDir::new();
            for (name, bits) in [("flac.flac", 16), ("monkey24.ape", 24), ("wavpack.wv", 16)] {
                let source = fixture(name);
                let original = decode(&source);
                for format in [
                    ConvertFormat::Flac,
                    ConvertFormat::Alac,
                    ConvertFormat::Wav,
                    ConvertFormat::Aiff,
                ] {
                    let output = dir.path().join(format!("{name}.{}", format.extension()));
                    assert!(
                        convert(&source, &output, format).unwrap(),
                        "{name} {format:?}"
                    );

                    let converted = decode(&output);
                    assert_eq!(converted.0, original.0, "{name} {format:?}");
                    assert_eq!(converted.1, original.1, "{name} {format:?}");
                    // 元と同じビット数で書き出し、サンプルは1つも変わらない
                    assert_eq!(converted.2, Some(bits), "{name} {format:?}");
                    assert_eq!(converted.3.len(), original.3.len(), "{name} {format:?}");
                    assert!(converted.3 == original.3, "{name} {format:?}");
                    assert_eq!(is_lossless_source(&output), Some(true), "{name} {format:?}");
                    std::fs::remove_file(&output).unwrap();
                }
            }
            // 一時ファイルは残さない
            assert_eq!(
                std::fs::read_dir(dir.path().join("temp")).unwrap().count(),
                0
            );
        }

        #[test]
        fn test_every_source_format_converts_to_aac() {
            let dir = TempDir::new();
            for name in [
                "lame.mp3",
                "flac.flac",
                "apple_aac.m4a",
                "vorbis.ogg",
                "opus.opus",
                "wavpack.wv",
                "monkey.ape",
            ] {
                let source = fixture(name);
                let original = decode(&source);
                let output = dir.path().join(format!("{name}.m4a"));

                assert!(
                    convert(&source, &output, ConvertFormat::Aac).unwrap(),
                    "{name}"
                );

                let converted = decode(&output);
                assert_eq!(converted.0, original.0, "{name}");
                assert_eq!(converted.1, original.1, "{name}");
                // 曲の頭・終わりの余分を除いた長さが、元と同じ（再生で聴く音を変換している）
                let frames = |decoded: &(u32, usize, Option<u32>, Vec<f32>)| {
                    (decoded.3.len() / decoded.1) as i64
                };
                assert!(
                    (frames(&converted) - frames(&original)).abs() <= 64,
                    "{name}: {} / {}",
                    frames(&converted),
                    frames(&original)
                );
                assert_eq!(is_lossless_source(&output), Some(false), "{name}");
            }
        }

        #[test]
        fn test_aac_falls_back_when_the_bitrate_does_not_fit_the_audio() {
            let dir = TempDir::new();
            // モノラル・22.05kHz（320kbpsは使えない）
            let source = dir.path().join("mono.wav");
            let samples: Vec<i16> = (0..22_050)
                .map(|frame| ((frame % 100) * 50) as i16)
                .collect();
            write_wav_samples(&source, 22_050, 1, &samples);
            let output = dir.path().join("mono.m4a");

            let converted = convert_file(
                &source,
                &output,
                ConvertFormat::Aac,
                320,
                &dir.path().join("temp"),
                &AtomicBool::new(false),
            )
            .unwrap();

            assert!(converted);
            let (rate, channels, _, _) = decode(&output);
            assert_eq!((rate, channels), (22_050, 1));
        }

        #[test]
        fn test_output_without_the_whole_audio_is_a_failure() {
            let dir = TempDir::new();
            // macOSのFLACのエンコーダーは、1ブロック（4608フレーム）より短い音声を落とす
            let source = dir.path().join("short.wav");
            let samples: Vec<i16> = (0..4_000 * 2).map(|i| (i % 100 * 50) as i16).collect();
            write_wav_samples(&source, 44_100, 2, &samples);

            let flac = dir.path().join("short.flac");
            let error = convert(&source, &flac, ConvertFormat::Flac).unwrap_err();
            assert!(error.to_string().contains("長さ"), "{error}");
            assert!(!flac.exists());

            // ほかの形式には、変換できる
            let alac = dir.path().join("short.m4a");
            assert!(convert(&source, &alac, ConvertFormat::Alac).unwrap());
            assert_eq!(decode(&alac).3.len(), samples.len());
        }

        #[test]
        fn test_tags_rating_gain_and_album_art_are_copied() {
            let dir = TempDir::new();
            let source = dir.path().join("source.mp3");
            std::fs::copy(fixture("lame.mp3"), &source).unwrap();
            let metadata = Metadata {
                title: Some("タイトル".to_string()),
                artist: Some("A; B".to_string()),
                album: Some("アルバム".to_string()),
                album_artist: Some("アルバムアーティスト".to_string()),
                genre: Some("Rock".to_string()),
                year: Some(2001),
                track_number: Some(3),
                disc_number: Some(2),
                composer: Some("作曲者".to_string()),
                comment: Some("コメント".to_string()),
                ..Metadata::default()
            };
            update_file_metadata(&source, &metadata, true).unwrap();
            update_file_rating(&source, 4).unwrap();
            let gain = ReplayGain {
                track_gain: Some(-6.48),
                track_peak: Some(0.988769),
                album_gain: Some(-7.0),
                album_peak: Some(1.0),
            };
            update_file_replay_gain(&source, &gain).unwrap();
            let picture = EmbeddedPicture {
                data: crate::metadata::test_images::png(8, 8),
                mime_type: "image/png".to_string(),
            };
            set_file_album_art(&source, &picture).unwrap();
            let expected = read_file_tags(&source).unwrap();

            for format in [ConvertFormat::Aac, ConvertFormat::Flac, ConvertFormat::Wav] {
                let output = dir.path().join(format!("output.{}", format.extension()));
                assert!(convert(&source, &output, format).unwrap(), "{format:?}");

                assert_eq!(read_file_tags(&output).unwrap(), expected, "{format:?}");
                let info = extract_all_file_info(&output).unwrap();
                assert_eq!(info.rating, 4, "{format:?}");
                assert_eq!(info.replay_gain, gain, "{format:?}");
                let art = extract_album_art(&output)
                    .unwrap()
                    .expect("画像が写っているはず");
                assert_eq!(art.data, picture.data, "{format:?}");
                assert_eq!(art.mime_type, picture.mime_type, "{format:?}");
                std::fs::remove_file(&output).unwrap();
            }
        }

        #[test]
        fn test_cancelled_and_failed_conversions_leave_no_output() {
            let dir = TempDir::new();
            let temp = dir.path().join("temp");
            let output = dir.path().join("out.flac");

            // 中断
            let cancelled = convert_file(
                &fixture("flac.flac"),
                &output,
                ConvertFormat::Flac,
                DEFAULT_AAC_BITRATE_KBPS,
                &temp,
                &AtomicBool::new(true),
            )
            .unwrap();
            assert!(!cancelled);
            assert!(!output.exists());

            // 読めないファイル
            let broken = dir.path().join("broken.flac");
            std::fs::write(&broken, b"not audio").unwrap();
            assert!(convert(&broken, &output, ConvertFormat::Flac).is_err());
            assert!(!output.exists());
            assert!(
                convert(
                    &dir.path().join("missing.flac"),
                    &output,
                    ConvertFormat::Flac
                )
                .is_err()
            );
            assert!(!output.exists());
            assert_eq!(std::fs::read_dir(&temp).unwrap().count(), 0);
        }
    }
}
