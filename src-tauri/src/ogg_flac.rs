//! Oggの中のFLAC（Ogg FLAC）の読み書き
//!
//! lofty（0.25）はOgg FLACに対応しておらず、中身から形式を判定できない。拡張子が`.ogg`の
//! Ogg FLACは、FLACのフレームの同期の値をADTSのAACと誤って判定し、誤った長さ・サンプルレートを
//! 返して、タグを読めない（書き込むと、Oggのファイルの先頭にID3v2タグを足してしまう）。
//!
//! ここでは、Oggのページからヘッダーのパケット（STREAMINFOと、FLACのメタデータのブロック）を
//! 取り出し、ネイティブのFLACのメタデータの並びにしてloftyで読む（タグ・画像は、FLACと同じに
//! 読める）。長さは、STREAMINFOにサンプル数がない場合（ffmpegは書かない）、最後のページの
//! グラニュール位置（そこまでのサンプル数）から求める。
//!
//! 書き込みでは、コメントのブロックを差し替えてヘッダーのページを作り直し、音声のページは
//! 番号を付け直して続ける（中身は変えない）。画像は、Ogg Vorbis・Opusと同じく、コメントの
//! `METADATA_BLOCK_PICTURE`の項目に書く（ffmpeg・TagLibは、Ogg FLACの画像のブロック
//! （PICTURE）を読まないため）。読む時は、どちらの画像も読む。
//!
//! Oggへの入れ方: <https://xiph.org/flac/ogg_mapping.html>

use lofty::config::ParseOptions;
use lofty::error::{FileEncodingError, FileParseError};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::properties::FileProperties;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

/// Oggのページのヘッダーの大きさ（区切りの値の表を除く）
const PAGE_HEADER_SIZE: usize = 27;

/// 1ページに入る区切りの値の数
const MAX_SEGMENTS: usize = 255;

/// Oggのページの最大の大きさ
const MAX_PAGE_SIZE: usize = PAGE_HEADER_SIZE + MAX_SEGMENTS + MAX_SEGMENTS * 255;

/// ページの種類: 前のページから続くパケットで始まる
const CONTINUED_PACKET: u8 = 0x01;

/// ページの種類: ストリームの最初のページ
const FIRST_PAGE: u8 = 0x02;

/// ページの種類: ストリームの最後のページ
const LAST_PAGE: u8 = 0x04;

/// 終わるパケットがないページのグラニュール位置（-1）
const NO_GRANULE: u64 = u64::MAX;

/// 最初のパケットの先頭（パケットの種類0x7Fと`FLAC`）
const FIRST_PACKET_SIGNATURE: &[u8] = b"\x7FFLAC";

/// 最初のパケットの中の、ヘッダーのパケットの数（ビッグエンディアンの2バイト）の位置
const HEADER_COUNT_POSITION: usize = 7;

/// 最初のパケットの中の、ネイティブのFLACのシグネチャ（`fLaC`）の位置
const NATIVE_SIGNATURE_POSITION: usize = 9;

/// 最初のパケットの中の、STREAMINFOのブロック（ブロックのヘッダーから）の位置
const STREAMINFO_POSITION: usize = 13;

/// STREAMINFOのブロックの本体の大きさ
const STREAMINFO_SIZE: usize = 34;

/// メタデータのブロックのヘッダーの大きさ（種類1バイトと、長さ3バイト）
const BLOCK_HEADER_SIZE: usize = 4;

/// メタデータのブロックの長さの最大（24ビット）
const MAX_BLOCK_SIZE: usize = 0xFF_FFFF;

/// ブロックの種類の値のうち、最後のブロックであることを示すビット
const LAST_BLOCK: u8 = 0x80;

/// ブロックの種類
const BLOCK_PADDING: u8 = 1;
const BLOCK_VORBIS_COMMENT: u8 = 4;
const BLOCK_PICTURE: u8 = 6;

/// OggのページのCRC（多項式0x04C11DB7。ビットの順序を反転しない）の表
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = (index as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

/// OggのページのCRCを計算する（CRCの欄を0にしたページのバイト列から）
fn crc32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |crc, &byte| {
        (crc << 8) ^ CRC_TABLE[((crc >> 24) ^ u32::from(byte)) as usize]
    })
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Oggのページ
struct Page {
    /// ページの種類（続きのパケットで始まる・ストリームの最初・最後）
    flags: u8,
    /// グラニュール位置（FLACでは、このページで終わるパケットまでのサンプル数）
    granule: u64,
    serial: u32,
    sequence: u32,
    /// 区切りの値（パケットを255バイトずつに分けた長さ。255未満の値でパケットが終わる）
    lacing: Vec<u8>,
    body: Vec<u8>,
}

impl Page {
    fn read<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut header = [0u8; PAGE_HEADER_SIZE];
        reader.read_exact(&mut header)?;
        if &header[..4] != b"OggS" || header[4] != 0 {
            return Err(invalid_data("Oggのページではありません"));
        }
        let mut lacing = vec![0; usize::from(header[26])];
        reader.read_exact(&mut lacing)?;
        let mut body = vec![0; lacing.iter().map(|&value| usize::from(value)).sum()];
        reader.read_exact(&mut body)?;

        let u32_at = |at: usize| {
            u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        Ok(Self {
            flags: header[5],
            granule: u64::from(u32_at(6)) | (u64::from(u32_at(10)) << 32),
            serial: u32_at(14),
            sequence: u32_at(18),
            lacing,
            body,
        })
    }

    /// ページの大きさ（バイト数）
    fn len(&self) -> usize {
        PAGE_HEADER_SIZE + self.lacing.len() + self.body.len()
    }

    /// ページのバイト列（CRCは計算し直す）
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len());
        bytes.extend_from_slice(b"OggS");
        bytes.push(0);
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.granule.to_le_bytes());
        bytes.extend_from_slice(&self.serial.to_le_bytes());
        bytes.extend_from_slice(&self.sequence.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.push(self.lacing.len() as u8);
        bytes.extend_from_slice(&self.lacing);
        bytes.extend_from_slice(&self.body);
        let crc = crc32(&bytes);
        bytes[22..26].copy_from_slice(&crc.to_le_bytes());
        bytes
    }
}

/// パケットを、ページに分ける（パケットごとに新しいページから始める）
///
/// ヘッダーのページだけに使う（グラニュール位置は0。パケットが終わらないページは-1）。
fn paginate(packets: impl IntoIterator<Item = Vec<u8>>, serial: u32) -> Vec<Page> {
    let mut pages: Vec<Page> = Vec::new();
    for packet in packets {
        // 長さが255の倍数のパケットは、0の区切りの値で終わる
        let mut lacing = vec![255u8; packet.len() / 255];
        lacing.push((packet.len() % 255) as u8);
        let mut offset = 0;
        for (index, chunk) in lacing.chunks(MAX_SEGMENTS).enumerate() {
            let size: usize = chunk.iter().map(|&value| usize::from(value)).sum();
            let finished = chunk.last().is_some_and(|&value| value < 255);
            pages.push(Page {
                flags: if index > 0 { CONTINUED_PACKET } else { 0 },
                granule: if finished { 0 } else { NO_GRANULE },
                serial,
                sequence: pages.len() as u32,
                lacing: chunk.to_vec(),
                body: packet[offset..offset + size].to_vec(),
            });
            offset += size;
        }
    }
    if let Some(first) = pages.first_mut() {
        first.flags |= FIRST_PAGE;
    }
    pages
}

/// Ogg FLACのファイルか（最初のページが、FLACの最初のパケットだけを持つか）
///
/// 読んだ後は、ファイルの先頭に戻す。
pub fn is_ogg_flac<R: Read + Seek>(reader: &mut R) -> io::Result<bool> {
    reader.rewind()?;
    // ページのヘッダー・1つの区切りの値・パケットの先頭
    let mut start = [0u8; PAGE_HEADER_SIZE + 1 + FIRST_PACKET_SIGNATURE.len()];
    let result = match reader.read_exact(&mut start) {
        Ok(()) => {
            start.starts_with(b"OggS")
                && start[5] & FIRST_PAGE != 0
                && start[26] == 1
                && start[PAGE_HEADER_SIZE + 1..] == *FIRST_PACKET_SIGNATURE
        }
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => false,
        Err(e) => return Err(e),
    };
    reader.rewind()?;
    Ok(result)
}

/// メタデータのブロックの種類（最後のブロックのビットを除く）
fn block_type(block: &[u8]) -> u8 {
    block[0] & !LAST_BLOCK
}

/// メタデータのブロックの長さが、ブロックのヘッダーの値と合っているか
fn is_valid_block(block: &[u8]) -> bool {
    block.len() >= BLOCK_HEADER_SIZE
        && u32::from_be_bytes([0, block[1], block[2], block[3]]) as usize
            == block.len() - BLOCK_HEADER_SIZE
}

/// Ogg FLACのヘッダー
struct Header {
    serial: u32,
    /// 最初のパケット（Oggへの入れ方の情報・`fLaC`・STREAMINFOのブロック）
    first_packet: Vec<u8>,
    /// STREAMINFOの後のメタデータのブロック（1つのパケットに1つ。ブロックのヘッダーを含む）
    blocks: Vec<Vec<u8>>,
    /// ヘッダーのページの数（このストリームのページ）
    page_count: u32,
    /// 最初の音声のページの位置
    audio_start: u64,
    /// ヘッダーのページの間に、ほかのストリームのページがあったか
    interleaved: bool,
    /// 最後のヘッダーのパケットの後に、同じページで音声のパケットが続いていたか
    shares_page_with_audio: bool,
}

impl Header {
    /// STREAMINFOのブロック（ブロックのヘッダーを含む）
    fn stream_info_block(&self) -> &[u8] {
        &self.first_packet[STREAMINFO_POSITION..]
    }
}

/// ファイルの先頭から、Ogg FLACのヘッダーを読む
fn read_header<R: Read + Seek>(reader: &mut R) -> io::Result<Header> {
    reader.rewind()?;
    let first = Page::read(reader)?;
    let first_packet = first.body;
    if first.flags & FIRST_PAGE == 0
        || first.lacing.len() != 1
        || !first_packet.starts_with(FIRST_PACKET_SIGNATURE)
        || first_packet.len() != STREAMINFO_POSITION + BLOCK_HEADER_SIZE + STREAMINFO_SIZE
        || &first_packet[NATIVE_SIGNATURE_POSITION..STREAMINFO_POSITION] != b"fLaC"
        || block_type(&first_packet[STREAMINFO_POSITION..]) != 0
        || !is_valid_block(&first_packet[STREAMINFO_POSITION..])
    {
        return Err(invalid_data("Ogg FLACの最初のパケットが壊れています"));
    }
    let serial = first.serial;

    // ヘッダーのパケットの数（0は不明。最後のブロックのビットが立ったブロックまでを読む）
    let declared = usize::from(u16::from_be_bytes([
        first_packet[HEADER_COUNT_POSITION],
        first_packet[HEADER_COUNT_POSITION + 1],
    ]));
    let stream_info_is_last = first_packet[STREAMINFO_POSITION] & LAST_BLOCK != 0;
    let is_complete = |blocks: &[Vec<u8>]| {
        if declared > 0 {
            blocks.len() == declared
        } else {
            blocks
                .last()
                .map_or(stream_info_is_last, |block| block[0] & LAST_BLOCK != 0)
        }
    };

    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut packet = Vec::new();
    let mut page_count = 1;
    let mut interleaved = false;
    let mut shares_page_with_audio = false;
    while !is_complete(&blocks) {
        let page = Page::read(reader)?;
        if page.serial != serial {
            interleaved = true;
            continue;
        }
        page_count += 1;
        let mut offset = 0;
        for (index, &value) in page.lacing.iter().enumerate() {
            let size = usize::from(value);
            packet.extend_from_slice(&page.body[offset..offset + size]);
            offset += size;
            if value == 255 {
                continue;
            }
            let block = std::mem::take(&mut packet);
            if !is_valid_block(&block) {
                return Err(invalid_data("Ogg FLACのメタデータのブロックが壊れています"));
            }
            blocks.push(block);
            if is_complete(&blocks) {
                shares_page_with_audio = index + 1 < page.lacing.len();
                break;
            }
        }
    }

    Ok(Header {
        serial,
        first_packet,
        blocks,
        page_count,
        audio_start: reader.stream_position()?,
        interleaved,
        shares_page_with_audio,
    })
}

/// ヘッダーを、ネイティブのFLACのメタデータの並び（`fLaC`とブロック）にする（loftyで読むため）
fn native_flac(header: &Header) -> Vec<u8> {
    let blocks: Vec<&[u8]> = std::iter::once(header.stream_info_block())
        .chain(header.blocks.iter().map(Vec::as_slice))
        .collect();
    let mut bytes = b"fLaC".to_vec();
    for (index, block) in blocks.iter().enumerate() {
        let last = if index + 1 == blocks.len() {
            LAST_BLOCK
        } else {
            0
        };
        bytes.push(block_type(block) | last);
        bytes.extend_from_slice(&block[1..]);
    }
    bytes
}

/// ヘッダーのメタデータを、loftyのFLACとして読む（音声の情報は読まない）
fn read_native_flac(header: &Header, options: ParseOptions) -> Result<FlacFile, FileParseError> {
    let mut options = options;
    FlacFile::read_from(
        &mut Cursor::new(native_flac(header)),
        options.read_properties(false),
    )
}

/// このストリームの最後のページの、グラニュール位置（最後までのサンプル数）
///
/// ファイルの終わりからページを探す（CRCが合うページだけを使う）。見つからなければNone。
fn last_granule<R: Read + Seek>(reader: &mut R, serial: u32) -> io::Result<Option<u64>> {
    let file_length = reader.seek(SeekFrom::End(0))?;
    let tail_length = file_length.min(MAX_PAGE_SIZE as u64);
    reader.seek(SeekFrom::Start(file_length - tail_length))?;
    let mut tail = vec![0; tail_length as usize];
    reader.read_exact(&mut tail)?;

    let mut end = tail.len();
    while let Some(start) = tail[..end].windows(4).rposition(|bytes| bytes == b"OggS") {
        let bytes = &tail[start..];
        if let Ok(page) = Page::read(&mut &*bytes)
            && page.serial == serial
            && page.granule != NO_GRANULE
            && page.to_bytes() == bytes[..page.len()]
        {
            return Ok(Some(page.granule));
        }
        end = start;
    }
    Ok(None)
}

/// 長さ・ビットレートなどの情報
fn read_properties<R: Read + Seek>(reader: &mut R, header: &Header) -> io::Result<FileProperties> {
    // STREAMINFOの本体の10バイト目から: サンプルレート（20ビット）・チャンネル数-1（3ビット）・
    // 1サンプルのビット数-1（5ビット）・サンプル数（36ビット。0は不明）
    let body = &header.stream_info_block()[BLOCK_HEADER_SIZE..];
    let mut info = [0u8; 8];
    info.copy_from_slice(&body[10..18]);
    let info = u64::from_be_bytes(info);
    let sample_rate = (info >> 44) as u32;
    let channels = ((info >> 41) & 0x7) as u8 + 1;
    let bit_depth = ((info >> 36) & 0x1F) as u8 + 1;
    let total_samples = info & 0xF_FFFF_FFFF;

    let samples = match total_samples {
        0 => last_granule(reader, header.serial)?,
        samples => Some(samples),
    };
    // ネイティブのFLAC（loftyの計算）と同じく、長さはミリ秒で切り捨てる
    let millis = match samples {
        Some(samples) if sample_rate > 0 => {
            u64::try_from(u128::from(samples) * 1000 / u128::from(sample_rate)).unwrap_or(u64::MAX)
        }
        _ => 0,
    };

    let file_length = reader.seek(SeekFrom::End(0))?;
    let audio_length = file_length.saturating_sub(header.audio_start);
    let bitrate = |length: u64| (millis > 0).then(|| (length * 8 / millis) as u32);
    Ok(FileProperties::new(
        Duration::from_millis(millis),
        bitrate(file_length),
        bitrate(audio_length),
        Some(sample_rate),
        Some(bit_depth),
        Some(channels),
        None,
    ))
}

/// Ogg FLACのファイルを読む
///
/// FLACのファイル（`FileType::Flac`）として返す。タグ・画像は、ネイティブのFLACと同じく
/// Vorbisコメントの汎用タグに入る。
pub fn read_file<R: Read + Seek>(
    reader: &mut R,
    options: ParseOptions,
) -> Result<TaggedFile, FileParseError> {
    let header = read_header(reader)?;
    let tags = TaggedFile::from(read_native_flac(&header, options)?)
        .tags()
        .to_vec();
    let properties = read_properties(reader, &header)?;
    Ok(TaggedFile::new(FileType::Flac, properties, tags))
}

/// Ogg FLACのファイルのVorbisコメントを読む（コメントのブロックがなければNone）
///
/// 画像のブロック（PICTURE）の画像は含まない（コメントの`METADATA_BLOCK_PICTURE`の画像だけ）。
pub fn read_comments<R: Read + Seek>(
    reader: &mut R,
    options: ParseOptions,
) -> Result<Option<VorbisComments>, FileParseError> {
    let header = read_header(reader)?;
    Ok(read_native_flac(&header, options)?.remove_vorbis_comments())
}

/// Vorbisコメントを、FLACのメタデータのブロック（VORBIS_COMMENT）にする
///
/// 画像は、`METADATA_BLOCK_PICTURE`の項目（画像のブロックをBase64にしたもの）として入れる。
fn comment_block(comments: &VorbisComments) -> io::Result<Vec<u8>> {
    let mut fields: Vec<Vec<u8>> = comments
        .items()
        .map(|(key, value)| format!("{key}={value}").into_bytes())
        .collect();
    for (picture, information) in comments.pictures() {
        let mut field = b"METADATA_BLOCK_PICTURE=".to_vec();
        field.extend(picture.as_flac_bytes(*information, true));
        fields.push(field);
    }

    // ベンダーの文字列と項目（それぞれ長さ4バイトと中身）、項目の数（4バイト）
    let vendor = comments.vendor().as_bytes();
    let size = 4 + vendor.len() + 4 + fields.iter().map(|field| 4 + field.len()).sum::<usize>();
    if size > MAX_BLOCK_SIZE {
        return Err(invalid_data("タグが大きすぎます"));
    }
    // ここからの長さは、ブロックの最大（24ビット）以下で、u32に収まる
    let mut block = Vec::with_capacity(BLOCK_HEADER_SIZE + size);
    block.push(BLOCK_VORBIS_COMMENT);
    block.extend_from_slice(&(size as u32).to_be_bytes()[1..]);
    block.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    block.extend_from_slice(vendor);
    block.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for field in &fields {
        block.extend_from_slice(&(field.len() as u32).to_le_bytes());
        block.extend_from_slice(field);
    }
    Ok(block)
}

/// Ogg FLACのファイルのVorbisコメントを、この内容で置き換える
///
/// 画像はコメントの中に書き、画像のブロック（PICTURE）と、詰め物のブロック（PADDING）は
/// 取り除く（ヘッダーのページを作り直すため、詰め物は役に立たない）。ほかのブロック
/// （SEEKTABLEなど）は、そのまま残す。ヘッダーのページの数が変わった場合は、音声のページの
/// 番号を付け直す。
///
/// ヘッダーのページにほかのストリームのページが混ざるファイルと、最後のヘッダーのパケットと
/// 音声が同じページにあるファイル（どちらも、Oggへの入れ方の決まりに合わない）は、書き換えない。
pub fn write_comments(
    file_path: &Path,
    comments: &VorbisComments,
) -> Result<(), FileEncodingError> {
    let data = std::fs::read(file_path)?;
    let header = read_header(&mut Cursor::new(&data))?;
    let mut blocks = vec![comment_block(comments)?];
    blocks.extend(
        header
            .blocks
            .iter()
            .filter(|block| {
                !matches!(
                    block_type(block),
                    BLOCK_VORBIS_COMMENT | BLOCK_PICTURE | BLOCK_PADDING
                )
            })
            .cloned(),
    );
    std::fs::write(file_path, replace_blocks(&data, &header, blocks)?)?;
    Ok(())
}

/// ファイル（`data`）のヘッダーのメタデータのブロックを、`blocks`に置き換えたバイト列を作る
///
/// 最初のブロックはVORBIS_COMMENTであること（Oggへの入れ方の決まり）。
fn replace_blocks(data: &[u8], header: &Header, mut blocks: Vec<Vec<u8>>) -> io::Result<Vec<u8>> {
    if header.interleaved || header.shares_page_with_audio {
        return Err(invalid_data(
            "この形のOgg FLACのファイルには、タグを書き込めません",
        ));
    }

    let last = blocks.len() - 1;
    for (index, block) in blocks.iter_mut().enumerate() {
        block[0] = block_type(block) | if index == last { LAST_BLOCK } else { 0 };
    }
    let mut first_packet = header.first_packet.clone();
    let count = u16::try_from(blocks.len()).map_err(|_| invalid_data("ブロックが多すぎます"))?;
    first_packet[HEADER_COUNT_POSITION..HEADER_COUNT_POSITION + 2]
        .copy_from_slice(&count.to_be_bytes());
    first_packet[STREAMINFO_POSITION] &= !LAST_BLOCK;

    let pages = paginate(std::iter::once(first_packet).chain(blocks), header.serial);
    let page_count = pages.len() as u32;
    let mut output = Vec::with_capacity(data.len());
    for page in &pages {
        output.extend(page.to_bytes());
    }

    // 音声のページ: このストリームの最後のページまで、番号を付け直す（その後に続くストリーム・
    // 読めないバイト列は、そのまま残す）
    let mut rest = &data[header.audio_start as usize..];
    let mut renumbering = page_count != header.page_count;
    while renumbering && !rest.is_empty() {
        let Ok(mut page) = Page::read(&mut &*rest) else {
            break;
        };
        let length = page.len();
        if page.serial == header.serial {
            page.sequence = page
                .sequence
                .wrapping_sub(header.page_count)
                .wrapping_add(page_count);
            output.extend(page.to_bytes());
            renumbering = page.flags & LAST_PAGE == 0;
        } else {
            output.extend_from_slice(&rest[..length]);
        }
        rest = &rest[length..];
    }
    output.extend_from_slice(rest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::{jpeg, png};
    use crate::metadata::{
        EmbeddedPicture, extract_album_art, extract_all_file_info, read_file_tags,
        remove_file_album_art, set_file_album_art, update_file_metadata,
    };
    use crate::models::Metadata;
    use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
    use std::fs::File;
    use std::path::PathBuf;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback")
    }

    /// ffmpegで作ったOgg FLAC（6秒・44.1kHz・2チャンネル・16ビットの無音。タイトルとアーティストあり。
    /// 作り方は`playback/decoder.rs`のテストのコメント）
    fn fixture() -> PathBuf {
        fixtures().join("ogg_flac.ogg")
    }

    /// テスト用のファイルを、一時フォルダへ写す（フォルダとファイルを返す）
    fn copy_fixture() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ogg_flac.ogg");
        std::fs::copy(fixture(), &file).unwrap();
        (dir, file)
    }

    /// すべてのページ（読んだページと、そのバイト列）
    fn read_pages(data: &[u8]) -> Vec<(Page, &[u8])> {
        let mut pages = Vec::new();
        let mut rest = data;
        while !rest.is_empty() {
            let page = Page::read(&mut &*rest).unwrap();
            let (bytes, next) = rest.split_at(page.len());
            pages.push((page, bytes));
            rest = next;
        }
        pages
    }

    /// Oggのストリームとして正しいか（CRC・ページの番号・最初と最後のページ・ヘッダーのパケットの数）
    /// を確かめ、音声のページ（グラニュール位置・区切りの値・中身）を返す
    fn assert_valid_stream(file: &Path) -> Vec<(u64, Vec<u8>, Vec<u8>)> {
        let data = std::fs::read(file).unwrap();
        let pages = read_pages(&data);
        for (index, (page, bytes)) in pages.iter().enumerate() {
            assert_eq!(page.to_bytes(), *bytes, "ページ{index}のCRC");
            assert_eq!(page.sequence, index as u32);
            assert_eq!(page.flags & FIRST_PAGE != 0, index == 0, "ページ{index}");
            assert_eq!(page.flags & LAST_PAGE != 0, index + 1 == pages.len());
        }

        let header = read_header(&mut Cursor::new(&data)).unwrap();
        let declared = u16::from_be_bytes([
            header.first_packet[HEADER_COUNT_POSITION],
            header.first_packet[HEADER_COUNT_POSITION + 1],
        ]);
        assert_eq!(usize::from(declared), header.blocks.len());
        assert_eq!(block_type(&header.blocks[0]), BLOCK_VORBIS_COMMENT);
        assert!(!header.shares_page_with_audio);
        pages[header.page_count as usize..]
            .iter()
            .map(|(page, _)| (page.granule, page.lacing.clone(), page.body.clone()))
            .collect()
    }

    fn block(block_type: u8, body: &[u8]) -> Vec<u8> {
        let mut block = vec![block_type];
        block.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        block.extend_from_slice(body);
        block
    }

    #[test]
    fn test_detects_ogg_flac_from_content() {
        let detect = |path: PathBuf| is_ogg_flac(&mut File::open(path).unwrap()).unwrap();
        assert!(detect(fixture()));
        for name in ["vorbis.ogg", "opus.opus", "flac.flac", "lame.mp3"] {
            assert!(!detect(fixtures().join(name)), "{name}");
        }
        assert!(!is_ogg_flac(&mut Cursor::new(b"OggS")).unwrap());
    }

    /// ffmpegで作ったOgg FLACの、タグ・長さ・サンプルレートを読める（ffprobeの値と同じ）
    ///
    /// loftyだけで読むと、ADTSのAACと誤って判定し、タグがなく、長さ8秒・11,025Hzになっていた。
    #[test]
    fn test_reads_tags_and_properties_of_a_file_written_by_ffmpeg() {
        let file = read_file(&mut File::open(fixture()).unwrap(), ParseOptions::new()).unwrap();
        assert_eq!(file.file_type(), FileType::Flac);
        let properties = file.properties();
        assert_eq!(properties.duration(), Duration::from_secs(6));
        assert_eq!(properties.sample_rate(), Some(44_100));
        assert_eq!(properties.channels(), Some(2));
        assert_eq!(properties.bit_depth(), Some(16));

        let info = extract_all_file_info(&fixture()).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Ogg FLAC Title"));
        assert_eq!(info.metadata.artist.as_deref(), Some("Ogg FLAC Artist"));
        assert_eq!(info.duration, Some(6));
        assert_eq!(info.sample_rate, Some(44_100));
        // 音声のページ（4,180バイト）× 8 ÷ 6,000ミリ秒
        assert_eq!(info.bitrate, Some(5));
    }

    /// STREAMINFOにサンプル数があるファイル（flacのツールが書く形）は、その値から長さを求める
    #[test]
    fn test_duration_comes_from_stream_info_when_it_has_the_sample_count() {
        let mut data = std::fs::read(fixture()).unwrap();
        let mut first = Page::read(&mut &*data).unwrap();
        // STREAMINFOの本体の14〜17バイト目: サンプル数の下位32ビット
        let samples = STREAMINFO_POSITION + BLOCK_HEADER_SIZE + 14;
        first.body[samples..samples + 4].copy_from_slice(&(44_100u32 * 3).to_be_bytes());
        data.splice(..first.len(), first.to_bytes());

        let file = read_file(&mut Cursor::new(&data), ParseOptions::new()).unwrap();
        assert_eq!(file.properties().duration(), Duration::from_secs(3));
    }

    /// ページを読んで書き戻すと、同じバイト列になる（CRCの計算がffmpegと同じ）
    #[test]
    fn test_pages_are_written_back_unchanged() {
        let data = std::fs::read(fixture()).unwrap();
        let pages = read_pages(&data);
        assert_eq!(pages.len(), 8);
        for (page, bytes) in pages {
            assert_eq!(page.to_bytes(), bytes);
        }
    }

    /// タグ・大きな画像を書き込み・取り除いても、Oggのストリームとして正しく、音声のページは変わらない
    #[test]
    fn test_writing_tags_keeps_the_audio_pages() {
        let (dir, file) = copy_fixture();
        let original = assert_valid_stream(&file);

        let metadata = Metadata {
            title: Some("新しいタイトル".to_string()),
            lyrics: Some("歌詞\n".repeat(200)),
            ..Default::default()
        };
        update_file_metadata(&file, &metadata, false).unwrap();
        assert_eq!(assert_valid_stream(&file), original);

        // ページの最大の大きさより大きい画像（ヘッダーのページが増え、音声のページの番号が変わる）
        let mut image = jpeg(600, 600);
        image.resize(300_000, 0x55);
        let art = EmbeddedPicture {
            data: image.clone(),
            mime_type: "image/jpeg".to_string(),
        };
        set_file_album_art(&file, &art).unwrap();
        assert_eq!(assert_valid_stream(&file), original);
        assert_eq!(extract_album_art(&file).unwrap().unwrap().data, image);

        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("新しいタイトル"));
        assert_eq!(info.metadata.artist.as_deref(), Some("Ogg FLAC Artist"));
        assert_eq!(info.duration, Some(6));
        assert_eq!(info.sample_rate, Some(44_100));

        assert!(remove_file_album_art(&file).unwrap());
        assert_eq!(assert_valid_stream(&file), original);
        assert!(extract_album_art(&file).unwrap().is_none());
        assert_eq!(
            read_file_tags(&file).unwrap().title.as_deref(),
            Some("新しいタイトル")
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 画像のブロック（PICTURE）の画像も読む。書き込む時は、画像をコメントへ移し、詰め物の
    /// ブロックは取り除き、ほかのブロックは残す
    #[test]
    fn test_picture_blocks_are_read_and_moved_into_comments() {
        let (dir, file) = copy_fixture();
        let data = std::fs::read(&file).unwrap();
        let header = read_header(&mut Cursor::new(&data)).unwrap();
        let picture = Picture::unchecked(png(40, 40))
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build();
        let information = PictureInformation::from_picture(&picture).unwrap();
        let application = block(2, b"test-data");
        let blocks = vec![
            header.blocks[0].clone(),
            block(BLOCK_PICTURE, &picture.as_flac_bytes(information, false)),
            application.clone(),
            block(BLOCK_PADDING, &[0; 16]),
        ];
        std::fs::write(&file, replace_blocks(&data, &header, blocks).unwrap()).unwrap();
        assert_valid_stream(&file);

        assert_eq!(extract_album_art(&file).unwrap().unwrap().data, png(40, 40));
        assert_eq!(
            read_file_tags(&file).unwrap().title.as_deref(),
            Some("Ogg FLAC Title")
        );

        let art = EmbeddedPicture {
            data: jpeg(50, 50),
            mime_type: "image/jpeg".to_string(),
        };
        set_file_album_art(&file, &art).unwrap();
        assert_valid_stream(&file);
        let data = std::fs::read(&file).unwrap();
        let header = read_header(&mut Cursor::new(&data)).unwrap();
        assert_eq!(header.blocks.len(), 2);
        assert_eq!(block_type(&header.blocks[0]), BLOCK_VORBIS_COMMENT);
        assert_eq!(header.blocks[1][1..], application[1..]);
        let comments = read_comments(&mut Cursor::new(&data), ParseOptions::new())
            .unwrap()
            .unwrap();
        assert_eq!(comments.pictures().len(), 1);
        assert_eq!(comments.pictures()[0].0.data(), jpeg(50, 50));
        assert_eq!(
            extract_album_art(&file).unwrap().unwrap().data,
            jpeg(50, 50)
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ヘッダーにほかのストリームのページが混ざるファイルは、読めるが、書き換えない
    #[test]
    fn test_files_mixed_with_other_streams_are_not_rewritten() {
        let (dir, file) = copy_fixture();
        let data = std::fs::read(&file).unwrap();
        let pages = read_pages(&data);
        let other = Page {
            flags: FIRST_PAGE,
            granule: 0,
            serial: 99,
            sequence: 0,
            lacing: vec![3],
            body: b"abc".to_vec(),
        };
        let mut mixed = pages[0].1.to_vec();
        mixed.extend(other.to_bytes());
        for (_, bytes) in &pages[1..] {
            mixed.extend_from_slice(bytes);
        }
        std::fs::write(&file, &mixed).unwrap();

        let info = extract_all_file_info(&file).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Ogg FLAC Title"));
        assert_eq!(info.duration, Some(6));

        let metadata = Metadata {
            title: Some("書き換えない".to_string()),
            ..Default::default()
        };
        assert!(update_file_metadata(&file, &metadata, false).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), mixed);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
