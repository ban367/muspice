//! WavPack（`.wv`）のデコード
//!
//! 純Rustのデコーダー（`wavicle`）を使う。対応するのは、可逆のWavPack（16・24・32ビットの整数と
//! 32ビットの浮動小数点、モノラル・ステレオ）。ハイブリッド（非可逆）・DSD・3チャンネル以上は
//! `wavicle`が対応していないため、開く時にエラーにする。
//!
//! `wavicle`は、渡したバイト列をまとめてデコードする。WavPackのブロックは独立にデコードできる
//! ため、ファイルを読み込んでブロックの位置を調べておき、1ブロックずつ渡す（`BlockSource`）。
//! ファイルの後ろに付くタグ（APEv2）は、ブロックではないため読み飛ばす。

use super::block_decoder::BlockSource;
use crate::error::{AppError, AppResult};
use std::path::Path;

/// WavPackのブロックの先頭の印
const BLOCK_MAGIC: &[u8; 4] = b"wvpk";

/// 音声の入ったブロックの、ファイルの中の位置
struct BlockEntry {
    offset: usize,
    length: usize,
    /// 曲の頭からの、先頭のフレームの位置
    start: u64,
    frames: u64,
}

/// 開いたWavPackのファイル
pub struct WavPackSource {
    bytes: Vec<u8>,
    blocks: Vec<BlockEntry>,
    sample_rate: u32,
    channels: usize,
    /// サンプルの値の範囲（整数は2の「ビット数-1」乗。浮動小数点はNone）
    full_scale: Option<f32>,
}

/// `wavicle`が読めなかった理由を、再生のエラーにする
///
/// WavPackとしては正しいが対応していない種類のファイル（ハイブリッドなど）は、その種類を伝える。
fn unsupported(error: wavicle::Error) -> AppError {
    use wavicle::{Error, Scope};

    let kind = match &error {
        Error::OutOfScope(Scope::Hybrid | Scope::CorrectionFile) => "ハイブリッド（非可逆）",
        Error::OutOfScope(Scope::Dsd) => "DSD",
        Error::OutOfScope(Scope::MoreThanTwoChannels | Scope::MultichannelSpanning) => {
            "3チャンネル以上"
        }
        Error::UnsupportedVersion(_) => "古いバージョン",
        _ => {
            return AppError::Playback(format!("対応していないWavPackのファイルです: {}", error));
        }
    };
    AppError::Playback(format!("{}のWavPackには対応していません", kind))
}

impl WavPackSource {
    pub fn open(path: &Path) -> AppResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("ファイルが見つかりません: {}", path.display()))
            }
            _ => AppError::Io(format!("ファイルを開けません: {}", e)),
        })?;
        Self::from_bytes(bytes)
    }

    fn from_bytes(bytes: Vec<u8>) -> AppResult<Self> {
        // 先頭から続くブロックを調べる（ブロックでないもの（タグなど）が出てきたら、そこまで）
        let mut blocks = Vec::new();
        let mut offset = 0;
        while bytes.len() >= offset + 4 && &bytes[offset..offset + 4] == BLOCK_MAGIC {
            let header = wavicle::BlockHeader::parse(&bytes[offset..]).map_err(unsupported)?;
            let length = header.block_len();
            // 途中で切れたファイル: 読めるブロックまでを再生する
            if length < 32 || offset + length > bytes.len() {
                break;
            }
            // 音声のないブロック（情報だけのブロック）は、デコードの対象にしない
            if header.block_samples > 0 {
                blocks.push(BlockEntry {
                    offset,
                    length,
                    start: header.block_index,
                    frames: u64::from(header.block_samples),
                });
            }
            offset += length;
        }
        if blocks.is_empty() {
            return Err(AppError::Playback(
                "WavPackの音声のデータがありません".to_string(),
            ));
        }

        // 形式（サンプルレート・チャンネル数・ビット数）は、ブロックの並びから読む
        // （対応していない形式は、ここでエラーになる）
        let info = wavicle::StreamInfo::scan(&bytes[..offset]).map_err(unsupported)?;
        let full_scale = (!info.is_float).then(|| 2f32.powi(info.bits_per_sample as i32 - 1));
        Ok(Self {
            bytes,
            blocks,
            sample_rate: info.sample_rate,
            channels: info.channels as usize,
            full_scale,
        })
    }
}

impl BlockSource for WavPackSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        self.channels
    }

    fn block_count(&self) -> usize {
        self.blocks.len()
    }

    fn block_range(&self, index: usize) -> (u64, u64) {
        let block = &self.blocks[index];
        (block.start, block.frames)
    }

    fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()> {
        out.clear();
        let block = &self.blocks[index];
        let decoded =
            wavicle::decode_stream(&self.bytes[block.offset..block.offset + block.length])
                .map_err(|e| {
                    AppError::Playback(format!("WavPackのデコードに失敗しました: {}", e))
                })?;
        match self.full_scale {
            Some(full_scale) => out.extend(
                decoded
                    .samples
                    .iter()
                    .map(|&sample| sample as f32 / full_scale),
            ),
            // 浮動小数点のサンプルは、ビットの並びのまま入っている
            None => out.extend(
                decoded
                    .samples
                    .iter()
                    .map(|&sample| f32::from_bits(sample as u32)),
            ),
        }
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
    fn test_reads_the_format_and_the_blocks() {
        let source = WavPackSource::open(&fixture("wavpack.wv")).unwrap();

        assert_eq!(source.sample_rate(), 44_100);
        assert_eq!(source.channels(), 1);
        // 0.5秒の音源。ブロックは位置の順に、切れ目なく並ぶ
        let mut expected_start = 0;
        for index in 0..source.block_count() {
            let (start, frames) = source.block_range(index);
            assert_eq!(start, expected_start);
            expected_start += frames;
        }
        assert_eq!(expected_start, 22_050);
        assert!(source.block_count() > 1);
    }

    #[test]
    fn test_trailing_tag_and_truncated_data_are_ignored() {
        let bytes = std::fs::read(fixture("wavpack.wv")).unwrap();
        let complete = WavPackSource::from_bytes(bytes.clone()).unwrap();

        // 後ろにタグ（ブロックでないデータ）が付いていても、同じように読める
        let mut tagged = bytes.clone();
        tagged.extend_from_slice(b"APETAGEX and some more bytes");
        let with_tag = WavPackSource::from_bytes(tagged).unwrap();
        assert_eq!(with_tag.block_count(), complete.block_count());

        // 途中で切れたファイルは、読めるブロックまで
        let cut = WavPackSource::from_bytes(bytes[..bytes.len() / 2].to_vec()).unwrap();
        assert!(cut.block_count() > 0);
        assert!(cut.block_count() < complete.block_count());
    }

    #[test]
    fn test_rejects_files_that_are_not_wavpack() {
        assert!(matches!(
            WavPackSource::from_bytes(b"this is not a wavpack file".to_vec()),
            Err(AppError::Playback(_))
        ));
        assert!(matches!(
            WavPackSource::from_bytes(Vec::new()),
            Err(AppError::Playback(_))
        ));
        assert!(matches!(
            WavPackSource::open(&fixture("missing.wv")),
            Err(AppError::NotFound(_))
        ));
        // 印だけで、中身が壊れているファイル
        let mut broken = b"wvpk".to_vec();
        broken.extend_from_slice(&[0xff; 64]);
        assert!(WavPackSource::from_bytes(broken).is_err());
    }

    /// 対応していない種類のWavPack（ハイブリッドなど）は、その種類を伝えるエラーにする
    #[test]
    fn test_reports_the_kind_of_unsupported_wavpack() {
        /// ブロックのヘッダーの、フラグの位置
        const FLAGS_OFFSET: usize = 24;
        let message = |flag: u32| {
            let mut bytes = std::fs::read(fixture("wavpack.wv")).unwrap();
            let flags =
                u32::from_le_bytes(bytes[FLAGS_OFFSET..FLAGS_OFFSET + 4].try_into().unwrap());
            bytes[FLAGS_OFFSET..FLAGS_OFFSET + 4].copy_from_slice(&(flags | flag).to_le_bytes());
            match WavPackSource::from_bytes(bytes) {
                Err(AppError::Playback(message)) => message,
                _ => panic!("エラーにならない"),
            }
        };

        // ハイブリッド（ビット3）・DSD（ビット31）
        assert_eq!(
            message(1 << 3),
            "ハイブリッド（非可逆）のWavPackには対応していません"
        );
        assert_eq!(message(1 << 31), "DSDのWavPackには対応していません");
    }
}
