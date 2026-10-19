//! APE（Monkey's Audio。`.ape`）のデコード
//!
//! 純Rustのデコーダー（`ape-decoder`）を使う。APEのファイルは、独立にデコードできるフレームの
//! 並びになっているため、フレームをブロックとして扱う（`BlockSource`）。フレームの長さは
//! 圧縮レベルによって違い、最高の圧縮（Insane）では1フレームが数十秒になる。
//! 対応するのは、Monkey's Audio 3.95以降で作ったファイル（それより前のものは、開く時にエラーにする）。

use super::block_decoder::BlockSource;
use crate::error::{AppError, AppResult};
use ape_decoder::ApeDecoder;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// デコードしたPCM（バイト列）の、1サンプルの形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleFormat {
    /// 8ビット（符号なし。`signed`なら符号付き）
    Int8 {
        signed: bool,
    },
    Int16,
    Int24,
    Int32,
    Float32,
}

impl SampleFormat {
    fn bytes(self) -> usize {
        match self {
            Self::Int8 { .. } => 1,
            Self::Int16 => 2,
            Self::Int24 => 3,
            Self::Int32 | Self::Float32 => 4,
        }
    }

    /// 1サンプル分のバイト列（`bytes()`の長さ）を、-1.0〜1.0の値にする
    fn to_f32(self, bytes: &[u8], big_endian: bool) -> f32 {
        // リトルエンディアンの並びにそろえる
        let mut le = [0u8; 4];
        for (index, byte) in bytes.iter().enumerate() {
            let position = if big_endian {
                bytes.len() - 1 - index
            } else {
                index
            };
            le[position] = *byte;
        }
        match self {
            Self::Int8 { signed: true } => f32::from(le[0] as i8) / 128.0,
            Self::Int8 { signed: false } => (f32::from(le[0]) - 128.0) / 128.0,
            Self::Int16 => f32::from(i16::from_le_bytes([le[0], le[1]])) / 32_768.0,
            // 上位の1バイトを符号で埋めてから、8ビット分を戻す
            Self::Int24 => (i32::from_le_bytes([0, le[0], le[1], le[2]]) >> 8) as f32 / 8_388_608.0,
            Self::Int32 => i32::from_le_bytes(le) as f32 / 2_147_483_648.0,
            Self::Float32 => f32::from_le_bytes(le),
        }
    }
}

/// 開いたAPEのファイル
pub struct ApeSource {
    decoder: ApeDecoder<BufReader<File>>,
    sample_rate: u32,
    channels: usize,
    bits_per_sample: u32,
    format: SampleFormat,
    big_endian: bool,
    /// 1フレームのサンプル数（最後のフレームだけ、これより短い）
    frames_per_block: u64,
    total_frames: u64,
    block_count: usize,
}

impl ApeSource {
    pub fn open(path: &Path) -> AppResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("ファイルが見つかりません: {}", path.display()))
            }
            _ => AppError::Io(format!("ファイルを開けません: {}", e)),
        })?;
        let decoder = ApeDecoder::new(BufReader::new(file)).map_err(|e| match e {
            // Monkey's Audio 3.95（2004年）より前のファイル
            ape_decoder::ApeError::UnsupportedVersion(_) => AppError::Playback(
                "古いバージョン（3.95より前）のAPEには対応していません".to_string(),
            ),
            _ => AppError::Playback(format!("対応していないAPEのファイルです: {}", e)),
        })?;

        let info = decoder.info();
        let format = match (info.bits_per_sample, info.is_floating_point) {
            (32, true) => SampleFormat::Float32,
            (8, false) => SampleFormat::Int8 {
                signed: info.is_signed_8bit,
            },
            (16, false) => SampleFormat::Int16,
            (24, false) => SampleFormat::Int24,
            (32, false) => SampleFormat::Int32,
            (bits, _) => {
                return Err(AppError::Playback(format!(
                    "対応していないAPEのファイルです: {}ビット",
                    bits
                )));
            }
        };
        Ok(Self {
            sample_rate: info.sample_rate,
            channels: usize::from(info.channels),
            bits_per_sample: u32::from(info.bits_per_sample),
            format,
            big_endian: info.is_big_endian,
            frames_per_block: u64::from(info.blocks_per_frame),
            total_frames: info.total_samples,
            block_count: decoder.total_frames() as usize,
            decoder,
        })
    }
}

impl ApeSource {
    /// フレーム（ブロック）の範囲（曲の頭からの、先頭のフレームの位置と、フレーム数）
    fn range(&self, index: usize) -> (u64, u64) {
        let start = index as u64 * self.frames_per_block;
        (
            start,
            self.frames_per_block
                .min(self.total_frames.saturating_sub(start)),
        )
    }
}

impl BlockSource for ApeSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        self.channels
    }

    fn bits_per_sample(&self) -> u32 {
        self.bits_per_sample
    }

    fn total_frames(&self) -> u64 {
        match self.block_count {
            0 => 0,
            count => {
                let (start, frames) = self.range(count - 1);
                start + frames
            }
        }
    }

    fn block_range(&mut self, index: usize) -> AppResult<Option<(u64, u64)>> {
        Ok((index < self.block_count).then(|| self.range(index)))
    }

    fn find_block(&mut self, frame: u64) -> AppResult<Option<usize>> {
        if frame >= self.total_frames() || self.frames_per_block == 0 {
            return Ok(None);
        }
        let index = (frame / self.frames_per_block) as usize;
        Ok((index < self.block_count).then_some(index))
    }

    fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()> {
        out.clear();
        let pcm = self
            .decoder
            .decode_frame(index as u32)
            .map_err(|e| AppError::Playback(format!("APEのデコードに失敗しました: {}", e)))?;
        let format = self.format;
        let big_endian = self.big_endian;
        out.extend(
            pcm.chunks_exact(format.bytes())
                .map(|bytes| format.to_f32(bytes, big_endian)),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/playback")
            .join(name)
    }

    #[test]
    fn test_sample_formats_convert_to_unit_range() {
        let int16 = SampleFormat::Int16;
        assert_eq!(int16.to_f32(&[0x00, 0x40], false), 0.5);
        assert_eq!(int16.to_f32(&[0x00, 0x80], false), -1.0);
        // ビッグエンディアン
        assert_eq!(int16.to_f32(&[0x40, 0x00], true), 0.5);

        let int24 = SampleFormat::Int24;
        assert_eq!(int24.to_f32(&[0x00, 0x00, 0x40], false), 0.5);
        assert_eq!(int24.to_f32(&[0x00, 0x00, 0x80], false), -1.0);
        assert_eq!(int24.to_f32(&[0xff, 0xff, 0xff], false), -1.0 / 8_388_608.0);
        assert_eq!(int24.to_f32(&[0x40, 0x00, 0x00], true), 0.5);

        assert_eq!(SampleFormat::Int32.to_f32(&[0, 0, 0, 0x40], false), 0.5);
        assert_eq!(
            SampleFormat::Float32.to_f32(&0.25f32.to_le_bytes(), false),
            0.25
        );
        assert_eq!(
            SampleFormat::Float32.to_f32(&0.25f32.to_be_bytes(), true),
            0.25
        );
        // 8ビットは符号なし（128が無音）。符号付きの印があれば、符号付きとして読む
        assert_eq!(
            SampleFormat::Int8 { signed: false }.to_f32(&[128], false),
            0.0
        );
        assert_eq!(
            SampleFormat::Int8 { signed: false }.to_f32(&[192], false),
            0.5
        );
        assert_eq!(
            SampleFormat::Int8 { signed: true }.to_f32(&[0xc0], false),
            -0.5
        );
    }

    #[test]
    fn test_reads_the_format_and_the_frames() {
        let mut source = ApeSource::open(&fixture("monkey.ape")).unwrap();
        assert_eq!(source.sample_rate(), 44_100);
        assert_eq!(source.channels(), 1);
        assert_eq!(source.format, SampleFormat::Int16);
        // 0.5秒の音源は、1フレームに収まる
        assert_eq!(source.total_frames(), 22_050);
        assert_eq!(source.block_range(0).unwrap(), Some((0, 22_050)));
        assert_eq!(source.block_range(1).unwrap(), None);
        assert_eq!(source.find_block(22_049).unwrap(), Some(0));
        assert_eq!(source.find_block(22_050).unwrap(), None);

        let mut source = ApeSource::open(&fixture("monkey24.ape")).unwrap();
        assert_eq!(source.sample_rate(), 48_000);
        assert_eq!(source.channels(), 2);
        assert_eq!(source.format, SampleFormat::Int24);
        assert_eq!(source.block_range(0).unwrap(), Some((0, 24_000)));
    }

    #[test]
    fn test_rejects_files_that_are_not_ape() {
        let dir = std::env::temp_dir().join(format!("muspice-ape-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.ape");
        std::fs::write(&broken, b"this is not an ape file").unwrap();

        assert!(matches!(
            ApeSource::open(&broken),
            Err(AppError::Playback(_))
        ));
        assert!(matches!(
            ApeSource::open(&dir.join("missing.ape")),
            Err(AppError::NotFound(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
