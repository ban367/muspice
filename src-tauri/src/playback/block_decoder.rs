//! ブロック単位でデコードする形式（WavPack・APE）の、共通のデコーダー
//!
//! これらの形式は、ファイルが独立にデコードできるブロックの並びになっている。形式ごとの実装
//! （`BlockSource`）は「何番目のブロックをデコードする」だけを行い、曲の中の位置・シーク・
//! 返すかたまりの区切りは、ここでまとめて扱う。可逆の形式のため、曲の頭と終わりに余分はない。
//!
//! ブロックの位置は、形式によっては開いた時点で分からず、ファイルを読み進めながら調べる（WavPack）。
//! そのため、ブロックの範囲を尋ねる操作も、ファイルを読むことがある（失敗しうる）。

use crate::error::{AppError, AppResult};

/// 1回に返すかたまりの上限（フレーム数）
///
/// 形式によっては1つのブロックが数十秒になる（APEの最高圧縮）。後段の処理（サンプルレートの変換・
/// リングバッファへの書き込み）が大きなかたまりを一度に抱えないよう、小分けにして返す。
const MAX_CHUNK_FRAMES: usize = 8192;

/// 続けてデコードに失敗したブロックがこの数を超えたら、再生をあきらめる
/// （壊れたブロックが少し混じっているだけのファイルは、そのブロックを無音にして再生を続ける）
const MAX_CONSECUTIVE_BLOCK_ERRORS: u32 = 3;

/// 外部のデコーダーの呼び出しを、パニックしてもエラーとして返す形で行う
///
/// 壊れたファイル・細工されたファイルで、デコーダーが範囲外の読み出しなどでパニックすることが
/// ある。再生エンジンのスレッドが止まらないよう、ここで受け止める。
pub fn catch_panic<T>(run: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| {
        Err(AppError::Playback(
            "デコーダーが異常終了しました（ファイルが壊れている可能性があります）".to_string(),
        ))
    })
}

/// ブロック単位でデコードできるファイル
pub trait BlockSource: Send {
    /// サンプルレート（Hz）
    fn sample_rate(&self) -> u32;
    /// チャンネル数
    fn channels(&self) -> usize;
    /// 元の音声のビット数（形式の変換で、同じビット数で書き出すために使う）
    fn bits_per_sample(&self) -> u32 {
        16
    }
    /// 曲の長さ（フレーム数）
    fn total_frames(&self) -> u64;
    /// ブロックの範囲（曲の頭からの、先頭のフレームの位置と、フレーム数）。そのブロックがなければ
    /// （最後のブロックより後ろ）None
    fn block_range(&mut self, index: usize) -> AppResult<Option<(u64, u64)>>;
    /// `frame`（曲の頭からの位置）を含むブロックの番号（曲の終わりより後ろの位置は、None）
    fn find_block(&mut self, frame: u64) -> AppResult<Option<usize>>;
    /// ブロックをデコードし、インターリーブのサンプルを`out`へ入れる（`out`は空にしてから使う）
    fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()>;
}

/// `BlockSource`を、曲の頭から順に読むデコーダー
pub struct BlockDecoder {
    source: Box<dyn BlockSource>,
    sample_rate: u32,
    channels: usize,
    /// 曲の長さ（フレーム数）
    total_frames: u64,
    /// 次に返すフレームの位置（曲の頭から）
    position: u64,
    /// デコードしたブロック（インターリーブ）
    samples: Vec<f32>,
    /// `samples`の先頭のフレームの位置（曲の頭から）
    samples_start: u64,
    /// 次にデコードするブロック（Noneは、もうデコードするブロックがない）
    next_block: Option<usize>,
    consecutive_errors: u32,
}

impl BlockDecoder {
    pub fn new(source: Box<dyn BlockSource>) -> AppResult<Self> {
        let sample_rate = source.sample_rate();
        let channels = source.channels();
        if sample_rate == 0 || channels == 0 {
            return Err(AppError::Playback("音声のデータがありません".to_string()));
        }
        let total_frames = source.total_frames();
        Ok(Self {
            source,
            sample_rate,
            channels,
            total_frames,
            position: 0,
            samples: Vec::new(),
            samples_start: 0,
            next_block: Some(0),
            consecutive_errors: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn bits_per_sample(&self) -> u32 {
        self.source.bits_per_sample()
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        Some(self.total_frames as f64 / f64::from(self.sample_rate))
    }

    pub fn remaining_seconds(&self) -> Option<f64> {
        Some(self.total_frames.saturating_sub(self.position) as f64 / f64::from(self.sample_rate))
    }

    /// `samples`に入っているフレーム数
    fn buffered_frames(&self) -> u64 {
        (self.samples.len() / self.channels) as u64
    }

    /// 次のブロックをデコードして`samples`に入れる（デコードできないブロックは、無音にする）。
    /// もうブロックがなければ、falseを返す
    fn decode_next_block(&mut self) -> AppResult<bool> {
        let Some(index) = self.next_block else {
            return Ok(false);
        };
        let Some((start, frames)) = self.source.block_range(index)? else {
            self.next_block = None;
            return Ok(false);
        };
        self.next_block = Some(index + 1);
        self.samples_start = start;

        let expected = frames as usize * self.channels;
        // 外部のデコーダーが、壊れたファイルでパニックしても、再生のスレッドを止めない
        // （デコードの失敗として扱う。途中まで書かれた`samples`は、下で作り直す）
        let source = &mut self.source;
        let samples = &mut self.samples;
        let result = catch_panic(|| source.decode_block(index, samples));
        match result {
            Ok(()) if self.samples.len() == expected => self.consecutive_errors = 0,
            result => {
                self.consecutive_errors += 1;
                if self.consecutive_errors > MAX_CONSECUTIVE_BLOCK_ERRORS {
                    return Err(AppError::Playback(match result {
                        Err(e) => format!("デコードに失敗しました: {}", e),
                        Ok(()) => "デコードした音声の長さが正しくありません".to_string(),
                    }));
                }
                // 位置がずれないよう、ブロックの長さの無音にする
                match result {
                    Err(e) => log::warn!(
                        "Cannot decode block {} (filling with silence): {}",
                        index,
                        e
                    ),
                    Ok(()) => {
                        log::warn!(
                            "Block {} has an invalid length (filling with silence)",
                            index
                        )
                    }
                }
                self.samples.clear();
                self.samples.resize(expected, 0.0);
            }
        }
        Ok(true)
    }

    /// 次のかたまりを、多くても`limit`フレームまで返す（曲の終わりではNone）
    pub fn next_chunk_limited(&mut self, limit: Option<u64>) -> AppResult<Option<&[f32]>> {
        loop {
            if self.position >= self.total_frames {
                return Ok(None);
            }
            // `position`が、デコード済みのブロックの中にあるか
            let offset = self.position.checked_sub(self.samples_start);
            let offset = match offset {
                Some(offset) if offset < self.buffered_frames() => offset as usize,
                _ => {
                    if !self.decode_next_block()? {
                        return Ok(None);
                    }
                    continue;
                }
            };

            let available = self.buffered_frames() as usize - offset;
            let mut frames = available.min(MAX_CHUNK_FRAMES);
            if let Some(limit) = limit {
                frames = frames.min(limit.max(1) as usize);
            }
            self.position += frames as u64;
            return Ok(Some(
                &self.samples[offset * self.channels..(offset + frames) * self.channels],
            ));
        }
    }

    /// 指定した位置（秒）へ移動し、移動した位置（秒）を返す（曲の長さを超える位置は、曲の終わり）
    pub fn seek(&mut self, seconds: f64) -> AppResult<f64> {
        let rate = f64::from(self.sample_rate);
        let target = ((seconds.max(0.0) * rate).round() as u64).min(self.total_frames);
        self.position = target;
        self.consecutive_errors = 0;
        // 目的の位置を含むブロックから、デコードし直す
        self.next_block = self.source.find_block(target)?;
        self.samples.clear();
        self.samples_start = 0;
        Ok(target as f64 / rate)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// テスト用のファイル: フレームの値が、曲の頭からの位置になっている（チャンネルごとに符号を変える）
    pub struct CountingSource {
        pub rate: u32,
        pub channels: usize,
        /// ブロックごとのフレーム数
        pub blocks: Vec<u64>,
        /// ファイルに書かれた曲の長さ（Noneなら、ブロックの合計）
        pub declared_frames: Option<u64>,
        /// デコードに失敗させるブロック
        pub broken: Vec<usize>,
        /// デコードしたブロック（呼ばれた順）
        pub decoded: std::sync::Arc<std::sync::Mutex<Vec<usize>>>,
    }

    impl CountingSource {
        pub fn new(blocks: &[u64]) -> Self {
            Self {
                rate: 1000,
                channels: 2,
                blocks: blocks.to_vec(),
                declared_frames: None,
                broken: Vec::new(),
                decoded: Default::default(),
            }
        }
    }

    impl BlockSource for CountingSource {
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn channels(&self) -> usize {
            self.channels
        }
        fn total_frames(&self) -> u64 {
            self.declared_frames
                .unwrap_or_else(|| self.blocks.iter().sum())
        }
        fn block_range(&mut self, index: usize) -> AppResult<Option<(u64, u64)>> {
            Ok((index < self.blocks.len())
                .then(|| (self.blocks[..index].iter().sum(), self.blocks[index])))
        }
        fn find_block(&mut self, frame: u64) -> AppResult<Option<usize>> {
            let mut end = 0;
            for (index, frames) in self.blocks.iter().enumerate() {
                end += frames;
                if frame < end {
                    return Ok(Some(index));
                }
            }
            Ok(None)
        }
        fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()> {
            self.decoded.lock().unwrap().push(index);
            out.clear();
            if self.broken.contains(&index) {
                return Err(AppError::Playback("壊れたブロック".to_string()));
            }
            let (start, frames) = self.block_range(index)?.unwrap();
            for frame in start..start + frames {
                for channel in 0..self.channels {
                    let sign = if channel % 2 == 0 { 1.0 } else { -1.0 };
                    out.push(sign * frame as f32);
                }
            }
            Ok(())
        }
    }

    /// 残りをすべて読み、最初のチャンネルの値（＝フレームの位置）を返す
    fn read_positions(decoder: &mut BlockDecoder) -> Vec<u64> {
        let channels = decoder.channels();
        let mut positions = Vec::new();
        while let Some(chunk) = decoder.next_chunk_limited(None).unwrap() {
            assert_eq!(chunk.len() % channels, 0);
            for frame in chunk.chunks_exact(channels) {
                assert_eq!(frame[1], -frame[0]);
                positions.push(frame[0] as u64);
            }
        }
        positions
    }

    #[test]
    fn test_reads_all_blocks_in_order() {
        let mut decoder =
            BlockDecoder::new(Box::new(CountingSource::new(&[300, 300, 150]))).unwrap();

        assert_eq!(decoder.sample_rate(), 1000);
        assert_eq!(decoder.channels(), 2);
        assert_eq!(decoder.duration_seconds(), Some(0.75));
        assert_eq!(decoder.remaining_seconds(), Some(0.75));

        let positions = read_positions(&mut decoder);
        assert_eq!(positions, (0..750).collect::<Vec<_>>());
        assert_eq!(decoder.remaining_seconds(), Some(0.0));
        // 終わりの後は、何度呼んでもNone
        assert!(decoder.next_chunk_limited(None).unwrap().is_none());
    }

    #[test]
    fn test_large_blocks_are_returned_in_small_chunks() {
        let frames = MAX_CHUNK_FRAMES as u64 * 2 + 100;
        let mut decoder = BlockDecoder::new(Box::new(CountingSource::new(&[frames]))).unwrap();

        let mut sizes = Vec::new();
        while let Some(chunk) = decoder.next_chunk_limited(None).unwrap() {
            sizes.push(chunk.len() / 2);
        }
        assert_eq!(sizes, [MAX_CHUNK_FRAMES, MAX_CHUNK_FRAMES, 100]);
    }

    #[test]
    fn test_limit_splits_a_chunk_and_the_rest_follows() {
        let mut decoder = BlockDecoder::new(Box::new(CountingSource::new(&[100, 100]))).unwrap();

        let first = decoder.next_chunk_limited(Some(30)).unwrap().unwrap();
        assert_eq!(first.len(), 60);
        assert_eq!((first[0], first[58]), (0.0, 29.0));
        // 0を指定しても、1フレームは返す（進まなくならないようにする）
        assert_eq!(
            decoder.next_chunk_limited(Some(0)).unwrap().unwrap().len(),
            2
        );
        let rest = read_positions(&mut decoder);
        assert_eq!(rest, (31..200).collect::<Vec<_>>());
    }

    #[test]
    fn test_seek_starts_inside_the_block_that_contains_the_position() {
        let source = CountingSource::new(&[300, 300, 150]);
        let decoded = source.decoded.clone();
        let mut decoder = BlockDecoder::new(Box::new(source)).unwrap();

        assert_eq!(decoder.seek(0.4).unwrap(), 0.4);
        assert_eq!(decoder.remaining_seconds(), Some(0.35));
        let positions = read_positions(&mut decoder);
        assert_eq!(positions, (400..750).collect::<Vec<_>>());
        // 目的の位置より前のブロックは、デコードしない
        assert_eq!(*decoded.lock().unwrap(), [1, 2]);

        // ブロックの境目・曲の頭へも移れる
        decoder.seek(0.3).unwrap();
        assert_eq!(read_positions(&mut decoder).first(), Some(&300));
        decoder.seek(0.0).unwrap();
        assert_eq!(read_positions(&mut decoder).len(), 750);
    }

    #[test]
    fn test_seek_past_the_end_finishes_the_track() {
        let mut decoder = BlockDecoder::new(Box::new(CountingSource::new(&[300]))).unwrap();

        assert_eq!(decoder.seek(10.0).unwrap(), 0.3);
        assert!(decoder.next_chunk_limited(None).unwrap().is_none());
        // 負の位置は、曲の頭
        assert_eq!(decoder.seek(-1.0).unwrap(), 0.0);
        assert_eq!(read_positions(&mut decoder).len(), 300);
    }

    /// ブロックが、書かれた曲の長さより手前で終わる（途中で切れたファイル）: 読めるブロックまで返す
    #[test]
    fn test_track_ends_where_the_blocks_end() {
        let mut source = CountingSource::new(&[100, 100]);
        source.declared_frames = Some(300);
        let mut decoder = BlockDecoder::new(Box::new(source)).unwrap();

        assert_eq!(decoder.duration_seconds(), Some(0.3));
        assert_eq!(read_positions(&mut decoder), (0..200).collect::<Vec<_>>());
        assert!(decoder.next_chunk_limited(None).unwrap().is_none());
        // ブロックのない位置へのシークは、曲の終わり
        assert_eq!(decoder.seek(0.25).unwrap(), 0.25);
        assert!(decoder.next_chunk_limited(None).unwrap().is_none());
        decoder.seek(0.15).unwrap();
        assert_eq!(read_positions(&mut decoder), (150..200).collect::<Vec<_>>());
    }

    #[test]
    fn test_a_broken_block_becomes_silence_and_playback_continues() {
        let mut source = CountingSource::new(&[100, 100, 100]);
        source.broken = vec![1];
        let mut decoder = BlockDecoder::new(Box::new(source)).unwrap();

        let positions = read_positions(&mut decoder);
        // 長さは変わらず、壊れたブロックの分は無音（0）
        assert_eq!(positions.len(), 300);
        assert_eq!(positions[99], 99);
        assert!(positions[100..200].iter().all(|&value| value == 0));
        assert_eq!(positions[200], 200);
    }

    /// デコーダーがパニックしても、エラーとして扱う（再生のスレッドを止めない）
    #[test]
    fn test_a_panicking_decoder_is_treated_as_a_broken_block() {
        struct PanickingSource;
        impl BlockSource for PanickingSource {
            fn sample_rate(&self) -> u32 {
                1000
            }
            fn channels(&self) -> usize {
                1
            }
            fn total_frames(&self) -> u64 {
                20
            }
            fn block_range(&mut self, index: usize) -> AppResult<Option<(u64, u64)>> {
                Ok((index < 2).then_some((index as u64 * 10, 10)))
            }
            fn find_block(&mut self, frame: u64) -> AppResult<Option<usize>> {
                Ok((frame < 20).then_some(frame as usize / 10))
            }
            fn decode_block(&mut self, index: usize, out: &mut Vec<f32>) -> AppResult<()> {
                out.clear();
                if index == 0 {
                    out.push(1.0);
                    panic!("壊れたファイルでのパニック");
                }
                out.resize(10, 0.5);
                Ok(())
            }
        }
        // パニックの内容を、テストの出力に出さない
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut decoder = BlockDecoder::new(Box::new(PanickingSource)).unwrap();
        let first = decoder
            .next_chunk_limited(None)
            .map(|chunk| chunk.map(<[f32]>::to_vec));
        std::panic::set_hook(hook);

        // パニックしたブロックは無音になり、次のブロックは読める
        assert_eq!(first.unwrap().unwrap(), vec![0.0; 10]);
        assert_eq!(
            decoder.next_chunk_limited(None).unwrap().unwrap(),
            [0.5; 10]
        );
        assert!(catch_panic(|| Ok(1)).is_ok());
    }

    #[test]
    fn test_too_many_broken_blocks_fail() {
        let mut source = CountingSource::new(&[10; 8]);
        source.broken = (1..8).collect();
        let mut decoder = BlockDecoder::new(Box::new(source)).unwrap();

        let mut result = Ok(());
        while result.is_ok() {
            match decoder.next_chunk_limited(None) {
                Ok(Some(_)) => {}
                Ok(None) => panic!("壊れたファイルを最後まで読めてしまった"),
                Err(e) => result = Err(e),
            }
        }
        assert!(matches!(result, Err(AppError::Playback(_))));
    }

    #[test]
    fn test_empty_or_invalid_sources() {
        let mut empty = BlockDecoder::new(Box::new(CountingSource::new(&[]))).unwrap();
        assert_eq!(empty.duration_seconds(), Some(0.0));
        assert!(empty.next_chunk_limited(None).unwrap().is_none());

        let mut silent = CountingSource::new(&[10]);
        silent.channels = 0;
        assert!(matches!(
            BlockDecoder::new(Box::new(silent)),
            Err(AppError::Playback(_))
        ));
    }
}
