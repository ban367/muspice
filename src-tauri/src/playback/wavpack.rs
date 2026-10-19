//! WavPack（`.wv`）のデコード
//!
//! 純Rustのデコーダー（`wavicle`）を使う。対応するのは、可逆のWavPack（16・24・32ビットの整数と
//! 32ビットの浮動小数点、モノラル・ステレオ）。ハイブリッド（非可逆）・DSD・3チャンネル以上は
//! `wavicle`が対応していないため、開く時にエラーにする。
//!
//! `wavicle`は、渡したバイト列をまとめてデコードする。WavPackのブロックは独立にデコードできる
//! ため、ファイルからブロックを1つずつ読み、1ブロックずつ渡す（`BlockSource`）。
//!
//! ブロックの位置は、開いた時点では最初のブロックの分しか調べない。デコードを進めながら（シークでは、
//! 目的の位置まで）ブロックの頭（32バイト）を読んで調べる。大きなファイルでも、開く時に読むのは
//! 最初のブロックまでで、メモリに持つのは1ブロック分と、調べたブロックの位置だけになる。
//! 曲の長さは、最初のブロックの頭に書かれた値を使う（書かれていないファイルは、開く時にすべての
//! ブロックを調べる）。ファイルの後ろに付くタグ（APEv2）・途中で切れたブロックは、ブロックの
//! 終わりとして扱う（長さの値より早く終わる）。

use super::block_decoder::BlockSource;
use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use wavicle::block::HEADER_LEN;

/// WavPackのブロックの先頭の印
const BLOCK_MAGIC: &[u8; 4] = b"wvpk";

/// ファイルを読む時に、まとめて読む大きさ（バイト）
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// 音声の入ったブロックの、ファイルの中の位置
struct BlockEntry {
    offset: u64,
    length: usize,
    /// 曲の頭からの、先頭のフレームの位置
    start: u64,
    frames: u64,
}

/// 開いたWavPackのファイル（`R`は、ファイルの読み取り。テストではメモリ上のバイト列）
pub struct WavPackSource<R = File> {
    reader: BufReader<R>,
    /// `reader`が次に読む位置（ファイルの頭からのバイト数）。読み取りに失敗した後は、分からない
    read_position: Option<u64>,
    /// ファイルの大きさ（バイト）
    len: u64,
    /// 調べたブロック（曲の頭から、位置の順に切れ目なく並ぶ）
    blocks: Vec<BlockEntry>,
    /// 次に調べるブロックの位置（ファイルの頭からのバイト数）。最後のブロックまで調べたらNone
    scan_offset: Option<u64>,
    total_frames: u64,
    /// 読み込んだブロック（使い回す）
    block_bytes: Vec<u8>,
    sample_rate: u32,
    channels: usize,
    bits_per_sample: u32,
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

fn read_error(error: std::io::Error) -> AppError {
    AppError::Io(format!("ファイルを読めません: {}", error))
}

impl WavPackSource {
    pub fn open(path: &Path) -> AppResult<Self> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("ファイルが見つかりません: {}", path.display()))
            }
            _ => AppError::Io(format!("ファイルを開けません: {}", e)),
        })?;
        let len = file.metadata().map_err(read_error)?.len();
        Self::from_reader(file, len)
    }
}

impl<R: Read + Seek> WavPackSource<R> {
    /// 大きさ`len`（バイト）のファイルを読む`reader`から開く
    fn from_reader(reader: R, len: u64) -> AppResult<Self> {
        let mut this = Self {
            reader: BufReader::with_capacity(READ_BUFFER_BYTES, reader),
            read_position: Some(0),
            len,
            blocks: Vec::new(),
            scan_offset: Some(0),
            total_frames: 0,
            block_bytes: Vec::new(),
            sample_rate: 0,
            channels: 0,
            bits_per_sample: 0,
            full_scale: None,
        };

        // 最初の音声のブロックを調べる（先頭がブロックでなければ、WavPackとして読めない）
        let Some(first) = this.scan_next()? else {
            return Err(AppError::Playback(
                "WavPackの音声のデータがありません".to_string(),
            ));
        };
        // 形式（サンプルレート・チャンネル数・ビット数）は、最初の音声のブロックまでから読む
        // （前に情報だけのブロックがあれば、それも含める。対応していない形式は、ここでエラーになる）
        let mut head = vec![0; this.blocks[0].offset as usize + this.blocks[0].length];
        this.read_at(0, &mut head)?;
        let info = wavicle::StreamInfo::scan(&head).map_err(unsupported)?;
        this.sample_rate = info.sample_rate;
        this.channels = info.channels as usize;
        this.bits_per_sample = info.bits_per_sample;
        this.full_scale = (!info.is_float).then(|| 2f32.powi(info.bits_per_sample as i32 - 1));

        this.total_frames = match first.total_samples {
            Some(total) => total,
            // 曲の長さが書かれていない: すべてのブロックを調べる
            None => {
                while this.scan_next()?.is_some() {}
                this.blocks
                    .last()
                    .map_or(0, |block| block.start + block.frames)
            }
        };
        Ok(this)
    }

    /// ファイルの`offset`から、`buffer`の長さだけ読む
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> AppResult<()> {
        // 次に読む位置からの差で動かす（読み込んである範囲の中なら、ファイルを読み直さない）
        let moved = match self.read_position.take() {
            Some(position) => self.reader.seek_relative(offset as i64 - position as i64),
            None => self.reader.seek(SeekFrom::Start(offset)).map(|_| ()),
        };
        moved
            .and_then(|()| self.reader.read_exact(buffer))
            .map_err(read_error)?;
        self.read_position = Some(offset + buffer.len() as u64);
        Ok(())
    }

    /// `offset`にあるブロックの頭を読む（ファイルの終わり・ブロックでないもの（タグなど）・
    /// 途中で切れたブロックなら、None）
    fn read_header(&mut self, offset: u64) -> AppResult<Option<wavicle::BlockHeader>> {
        if offset + HEADER_LEN as u64 > self.len {
            return Ok(None);
        }
        let mut bytes = [0; HEADER_LEN];
        self.read_at(offset, &mut bytes)?;
        if &bytes[..4] != BLOCK_MAGIC {
            return Ok(None);
        }
        let header = wavicle::BlockHeader::parse(&bytes).map_err(unsupported)?;
        // 途中で切れたファイル: 読めるブロックまでを再生する
        if offset + header.block_len() as u64 > self.len {
            return Ok(None);
        }
        Ok(Some(header))
    }

    /// 次の音声のブロックを調べて`blocks`に足し、その頭を返す（音声のない、情報だけのブロックは
    /// 飛ばす）。もうブロックがなければNone
    fn scan_next(&mut self) -> AppResult<Option<wavicle::BlockHeader>> {
        while let Some(offset) = self.scan_offset {
            let Some(header) = self.read_header(offset)? else {
                self.scan_offset = None;
                break;
            };
            self.scan_offset = Some(offset + header.block_len() as u64);
            if header.block_samples == 0 {
                continue;
            }
            let start = self
                .blocks
                .last()
                .map_or(0, |block| block.start + block.frames);
            if header.block_index != start {
                // 位置が続いていない（壊れたファイル）: その手前までを再生する
                log::warn!(
                    "WavPackのブロックの位置が続いていません（{}フレーム目のはずが{}）",
                    start,
                    header.block_index
                );
                self.scan_offset = None;
                break;
            }
            self.blocks.push(BlockEntry {
                offset,
                length: header.block_len(),
                start,
                frames: u64::from(header.block_samples),
            });
            return Ok(Some(header));
        }
        Ok(None)
    }
}

impl<R: Read + Seek + Send> BlockSource for WavPackSource<R> {
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
        self.total_frames
    }

    fn block_range(&mut self, index: usize) -> AppResult<Option<(u64, u64)>> {
        while self.blocks.len() <= index && self.scan_next()?.is_some() {}
        Ok(self
            .blocks
            .get(index)
            .map(|block| (block.start, block.frames)))
    }

    fn find_block(&mut self, frame: u64) -> AppResult<Option<usize>> {
        if frame >= self.total_frames {
            return Ok(None);
        }
        // まだ調べていない位置なら、その位置を含むブロックまで調べる
        while self
            .blocks
            .last()
            .is_none_or(|block| block.start + block.frames <= frame)
            && self.scan_next()?.is_some()
        {}
        let index = self
            .blocks
            .partition_point(|block| block.start + block.frames <= frame);
        Ok((index < self.blocks.len()).then_some(index))
    }

    fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()> {
        out.clear();
        let (offset, length) = (self.blocks[index].offset, self.blocks[index].length);
        let mut bytes = std::mem::take(&mut self.block_bytes);
        bytes.resize(length, 0);
        let decoded = self.read_at(offset, &mut bytes).and_then(|()| {
            wavicle::decode_stream(&bytes)
                .map_err(|e| AppError::Playback(format!("WavPackのデコードに失敗しました: {}", e)))
        });
        self.block_bytes = bytes;
        let decoded = decoded?;
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
    use std::io::Cursor;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/playback")
            .join(name)
    }

    fn from_bytes(bytes: Vec<u8>) -> AppResult<WavPackSource<Cursor<Vec<u8>>>> {
        let len = bytes.len() as u64;
        WavPackSource::from_reader(Cursor::new(bytes), len)
    }

    /// すべてのブロックの範囲を、頭から順に調べる
    fn all_ranges(source: &mut impl BlockSource) -> Vec<(u64, u64)> {
        (0..)
            .map_while(|index| source.block_range(index).unwrap())
            .collect()
    }

    #[test]
    fn test_reads_the_format_and_the_blocks() {
        let mut source = WavPackSource::open(&fixture("wavpack.wv")).unwrap();

        assert_eq!(source.sample_rate(), 44_100);
        assert_eq!(source.channels(), 1);
        assert_eq!(source.total_frames(), 22_050);
        // 0.5秒の音源。ブロックは位置の順に、切れ目なく並ぶ
        let ranges = all_ranges(&mut source);
        let mut expected_start = 0;
        for (start, frames) in &ranges {
            assert_eq!(*start, expected_start);
            expected_start += frames;
        }
        assert_eq!(expected_start, 22_050);
        assert!(ranges.len() > 1);
    }

    /// 開く時に調べるのは最初のブロックだけで、残りはデコード・シークで必要になった分だけ調べる
    #[test]
    fn test_blocks_are_located_only_as_far_as_needed() {
        let mut source = WavPackSource::open(&fixture("wavpack.wv")).unwrap();
        assert_eq!(source.blocks.len(), 1);

        // 2つ目のブロックの範囲を尋ねると、そこまで調べる
        let (start, _) = source.block_range(1).unwrap().unwrap();
        assert_eq!(source.blocks.len(), 2);
        assert_eq!(start, source.blocks[0].frames);

        // 曲の終わりの近くを探すと、その位置を含むブロックまで調べる
        let index = source.find_block(22_000).unwrap().unwrap();
        let (start, frames) = source.block_range(index).unwrap().unwrap();
        assert!((start..start + frames).contains(&22_000));
        assert_eq!(source.blocks.len(), index + 1);
        // 調べ終えた範囲の中は、読み直さずに探す
        assert_eq!(source.find_block(0).unwrap(), Some(0));
        assert_eq!(source.find_block(22_050).unwrap(), None);

        // 調べた順によらず、どのブロックもデコードできる
        let mut samples = Vec::new();
        source.decode_block(index, &mut samples).unwrap();
        assert_eq!(samples.len() as u64, frames);
        source.decode_block(0, &mut samples).unwrap();
        assert_eq!(samples.len() as u64, source.blocks[0].frames);
    }

    #[test]
    fn test_trailing_tag_and_truncated_data_are_ignored() {
        let bytes = std::fs::read(fixture("wavpack.wv")).unwrap();
        let complete = all_ranges(&mut from_bytes(bytes.clone()).unwrap());

        // 後ろにタグ（ブロックでないデータ）が付いていても、同じように読める
        let mut tagged = bytes.clone();
        tagged.extend_from_slice(b"APETAGEX and some more bytes");
        assert_eq!(all_ranges(&mut from_bytes(tagged).unwrap()), complete);

        // 途中で切れたファイルは、読めるブロックまで（長さは、最初のブロックに書かれた値のまま）
        let mut cut = from_bytes(bytes[..bytes.len() / 2].to_vec()).unwrap();
        assert_eq!(cut.total_frames(), 22_050);
        let ranges = all_ranges(&mut cut);
        assert!(!ranges.is_empty() && ranges.len() < complete.len());
        assert_eq!(ranges[..], complete[..ranges.len()]);
        assert_eq!(cut.find_block(22_000).unwrap(), None);
    }

    /// 曲の長さが書かれていないファイルは、開く時にすべてのブロックを調べて長さを決める
    #[test]
    fn test_unknown_length_is_found_from_the_blocks() {
        /// 最初のブロックの頭の、曲の長さ（下位32ビット）の位置
        const TOTAL_SAMPLES_OFFSET: usize = 12;
        let mut bytes = std::fs::read(fixture("wavpack.wv")).unwrap();
        // 全ビットが1で、長さが分からないことを表す
        bytes[TOTAL_SAMPLES_OFFSET..TOTAL_SAMPLES_OFFSET + 4].copy_from_slice(&[0xff; 4]);

        let source = from_bytes(bytes).unwrap();
        assert_eq!(source.total_frames(), 22_050);
        assert!(source.scan_offset.is_none());
    }

    #[test]
    fn test_rejects_files_that_are_not_wavpack() {
        assert!(matches!(
            from_bytes(b"this is not a wavpack file".to_vec()),
            Err(AppError::Playback(_))
        ));
        assert!(matches!(from_bytes(Vec::new()), Err(AppError::Playback(_))));
        assert!(matches!(
            WavPackSource::open(&fixture("missing.wv")),
            Err(AppError::NotFound(_))
        ));
        // 印だけで、中身が壊れているファイル
        let mut broken = b"wvpk".to_vec();
        broken.extend_from_slice(&[0xff; 64]);
        assert!(from_bytes(broken).is_err());
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
            match from_bytes(bytes) {
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
