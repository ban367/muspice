//! MP4（M4A）のAACの、ギャップレス再生のための情報の読み取り
//!
//! AACのエンコーダーは、曲の頭に遅延（プライミング）、終わりにパディングを足す。そのまま
//! 再生すると曲間に短い無音が入るため、取り除く範囲をファイルから読む。情報は2か所にある。
//!
//! - iTunesのタグ`iTunSMPB`（Appleのエンコーダー）: 遅延・パディング・元のサンプル数
//! - 編集リスト（`edts`/`elst`。FFmpegなど）: 再生を始める位置と、再生する長さ
//!
//! `symphonia`のMP4のリーダーはどちらも反映しないため、`moov`ボックスを自前でたどって読む。
//! どちらもないファイルでは何も取り除かない（従来のプレーヤーと同じく、曲間に短い無音が残る）。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// `moov`ボックスとして読み込む大きさの上限（壊れたファイルで大量のメモリを確保しない）
const MAX_MOOV_SIZE: u64 = 64 * 1024 * 1024;

/// 曲の頭の遅延として受け入れる上限（秒）。これを超える値は、遅延ではなく編集とみなして使わない
const MAX_DELAY_SECONDS: u64 = 1;

/// 曲の頭・終わりの余分を取り除くための情報（フレームは、音声のサンプルレートでの数）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaplessInfo {
    /// 曲の頭で捨てるフレーム数
    pub delay: u64,
    /// 遅延を除いた、曲の長さ（分からなければNone）
    pub total_frames: Option<u64>,
}

/// ファイルから情報を読む（情報がない・読めない場合はNone）
pub fn read(path: &Path, sample_rate: u32) -> Option<GaplessInfo> {
    let moov = read_moov(path)?;
    parse_moov(&moov, sample_rate)
}

/// ファイルのトップレベルのボックスをたどり、`moov`の中身を読む
fn read_moov(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut offset = 0u64;
    while offset + 8 <= file_len {
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut header = [0u8; 16];
        file.read_exact(&mut header[..8]).ok()?;
        let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().ok()?));
        let mut header_len = 8u64;
        if size == 1 {
            // 64bitの大きさ
            file.read_exact(&mut header[8..16]).ok()?;
            size = u64::from_be_bytes(header[8..16].try_into().ok()?);
            header_len = 16;
        } else if size == 0 {
            // ファイルの終わりまで
            size = file_len - offset;
        }
        if size < header_len || offset + size > file_len {
            return None;
        }
        if &header[4..8] == b"moov" {
            let body_len = size - header_len;
            if body_len > MAX_MOOV_SIZE {
                return None;
            }
            let mut body = vec![0u8; body_len as usize];
            file.read_exact(&mut body).ok()?;
            return Some(body);
        }
        offset += size;
    }
    None
}

/// ボックスの並びを順に返す（種類と中身）。壊れた大きさのボックスに当たったら、そこで終わる
fn boxes(data: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut rest = data;
    std::iter::from_fn(move || {
        if rest.len() < 8 {
            return None;
        }
        let mut size = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        let mut header_len = 8;
        if size == 1 {
            if rest.len() < 16 {
                return None;
            }
            size = usize::try_from(u64::from_be_bytes(rest[8..16].try_into().ok()?)).ok()?;
            header_len = 16;
        } else if size == 0 {
            size = rest.len();
        }
        if size < header_len || size > rest.len() {
            return None;
        }
        let (current, next) = rest.split_at(size);
        rest = next;
        Some((&current[4..8], &current[header_len..]))
    })
}

/// 指定した種類の最初のボックスの中身を返す
fn find<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .find(|(k, _)| *k == kind.as_slice())
        .map(|(_, body)| body)
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn be_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// `mvhd` / `mdhd`から、タイムスケール（1秒あたりの単位数）と長さを読む
fn read_timescale_and_duration(header: &[u8]) -> Option<(u32, u64)> {
    match header.first()? {
        0 => Some((be_u32(header, 12)?, u64::from(be_u32(header, 16)?))),
        1 => Some((be_u32(header, 20)?, be_u64(header, 24)?)),
        _ => None,
    }
}

/// 編集リストの、最初の（空でない）編集
struct Edit {
    /// 再生する長さ（ムービーのタイムスケール）
    segment_duration: u64,
    /// 再生を始める位置（メディアのタイムスケール）
    media_time: u64,
}

fn read_first_edit(elst: &[u8]) -> Option<Edit> {
    let version = *elst.first()?;
    let count = be_u32(elst, 4)? as usize;
    let entry_len = if version == 1 { 20 } else { 12 };
    let mut edits = (0..count).filter_map(|index| {
        let offset = 8 + index * entry_len;
        let (segment_duration, media_time) = if version == 1 {
            (be_u64(elst, offset)?, be_u64(elst, offset + 8)? as i64)
        } else {
            (
                u64::from(be_u32(elst, offset)?),
                i64::from(be_u32(elst, offset + 4)? as i32),
            )
        };
        // 再生を始める位置が-1の編集は「空の編集」（再生の開始を遅らせるだけ）のため飛ばす
        (media_time >= 0).then_some(Edit {
            segment_duration,
            media_time: media_time as u64,
        })
    });
    let first = edits.next()?;
    // 編集が複数あるファイルは、単純な「頭と終わりを切る」ではないため使わない
    edits.next().is_none().then_some(first)
}

/// `moov`の中身から情報を読む
fn parse_moov(moov: &[u8], sample_rate: u32) -> Option<GaplessInfo> {
    let max_delay = MAX_DELAY_SECONDS * u64::from(sample_rate);

    // iTunSMPBがあれば、サンプル単位で正確なためそちらを使う
    if let Some(info) = read_itunsmpb(moov)
        && info.delay <= max_delay
    {
        return Some(info);
    }

    let (movie_timescale, _) = read_timescale_and_duration(find(moov, b"mvhd")?)?;
    // 最初の音声のトラック
    let (track, media) = boxes(moov)
        .filter(|(kind, _)| *kind == b"trak".as_slice())
        .find_map(|(_, track)| {
            let media = find(track, b"mdia")?;
            (find(media, b"hdlr")?.get(8..12)? == b"soun").then_some((track, media))
        })?;
    let (media_timescale, media_duration) = read_timescale_and_duration(find(media, b"mdhd")?)?;
    let edit = read_first_edit(find(find(track, b"edts")?, b"elst")?)?;
    if movie_timescale == 0 || media_timescale == 0 {
        return None;
    }

    let to_frames = |value: u64, timescale: u32| {
        (u128::from(value) * u128::from(sample_rate) / u128::from(timescale)) as u64
    };
    let delay = to_frames(edit.media_time, media_timescale);
    if delay > max_delay {
        return None;
    }

    // 長さは2通りに求められる。ムービーのタイムスケールは粗い（ミリ秒など）ことが多いため、
    // メディアの長さから求めた値が1単位以内で一致すれば、サンプル単位で正確なそちらを使う
    let from_edit = to_frames(edit.segment_duration, movie_timescale);
    let from_media = to_frames(
        media_duration.saturating_sub(edit.media_time),
        media_timescale,
    );
    let tolerance = u64::from(sample_rate) / u64::from(movie_timescale) + 1;
    let total_frames = if from_media.abs_diff(from_edit) <= tolerance {
        from_media
    } else {
        from_edit
    };
    (total_frames > 0).then_some(GaplessInfo {
        delay,
        total_frames: Some(total_frames),
    })
}

/// iTunesのタグ`iTunSMPB`（`moov/udta/meta/ilst/----`）を読む
fn read_itunsmpb(moov: &[u8]) -> Option<GaplessInfo> {
    let meta = find(find(moov, b"udta")?, b"meta")?;
    // `meta`は、バージョンとフラグ（4バイト）の後に子のボックスが並ぶ
    let ilst = find(meta.get(4..)?, b"ilst")?;
    let value = boxes(ilst)
        .filter(|(kind, _)| *kind == b"----".as_slice())
        .find_map(|(_, item)| {
            let name = find(item, b"name")?.get(4..)?;
            (name == b"iTunSMPB").then(|| find(item, b"data"))?
        })?;
    // `data`は、種類（4バイト）とロケール（4バイト）の後に値が入る
    let text = std::str::from_utf8(value.get(8..)?).ok()?;
    // 値は16進数を空白で区切った並び: 0 遅延 パディング 元のサンプル数 ...
    let mut fields = text.split_whitespace().skip(1);
    let delay = u64::from_str_radix(fields.next()?, 16).ok()?;
    let _padding = fields.next()?;
    let total = u64::from_str_radix(fields.next()?, 16).ok()?;
    (total > 0).then_some(GaplessInfo {
        delay,
        total_frames: Some(total),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }

    /// バージョン0の`mvhd` / `mdhd`
    fn header(timescale: u32, duration: u32) -> Vec<u8> {
        let mut body = vec![0u8; 12];
        body.extend_from_slice(&timescale.to_be_bytes());
        body.extend_from_slice(&duration.to_be_bytes());
        body
    }

    fn hdlr(kind: &[u8; 4]) -> Vec<u8> {
        let mut body = vec![0u8; 8];
        body.extend_from_slice(kind);
        body.extend_from_slice(&[0u8; 12]);
        body
    }

    fn elst(edits: &[(u32, i32)]) -> Vec<u8> {
        let mut body = vec![0u8; 4];
        body.extend_from_slice(&(edits.len() as u32).to_be_bytes());
        for (segment_duration, media_time) in edits {
            body.extend_from_slice(&segment_duration.to_be_bytes());
            body.extend_from_slice(&media_time.to_be_bytes());
            body.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        }
        body
    }

    fn track(kind: &[u8; 4], media: (u32, u32), edits: Option<&[(u32, i32)]>) -> Vec<u8> {
        let mut body = Vec::new();
        if let Some(edits) = edits {
            body.extend(make_box(b"edts", &make_box(b"elst", &elst(edits))));
        }
        let mut mdia = make_box(b"mdhd", &header(media.0, media.1));
        mdia.extend(make_box(b"hdlr", &hdlr(kind)));
        body.extend(make_box(b"mdia", &mdia));
        make_box(b"trak", &body)
    }

    fn itunsmpb(text: &str) -> Vec<u8> {
        let mut item = make_box(b"mean", b"\0\0\0\0com.apple.iTunes");
        item.extend(make_box(b"name", b"\0\0\0\0iTunSMPB"));
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(text.as_bytes());
        item.extend(make_box(b"data", &data));
        let ilst = make_box(b"ilst", &make_box(b"----", &item));
        let mut meta = vec![0u8; 4];
        meta.extend(ilst);
        make_box(b"udta", &make_box(b"meta", &meta))
    }

    #[test]
    fn test_reads_edit_list_written_by_ffmpeg() {
        // FFmpeg: ムービーはミリ秒、メディアはサンプルレート。メディアの長さは遅延＋曲の長さ
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"soun", (44_100, 133_324), Some(&[(3000, 1024)])));

        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(GaplessInfo {
                delay: 1024,
                total_frames: Some(132_300)
            })
        );
    }

    #[test]
    fn test_prefers_media_duration_when_movie_timescale_is_coarse() {
        // 曲の長さ132,345フレームは、ミリ秒では3001（切り捨て）としか書けない
        let mut moov = make_box(b"mvhd", &header(1000, 3001));
        moov.extend(track(b"soun", (44_100, 133_369), Some(&[(3001, 1024)])));

        assert_eq!(
            parse_moov(&moov, 44_100).unwrap().total_frames,
            Some(132_345)
        );
    }

    #[test]
    fn test_uses_edit_duration_when_media_includes_padding() {
        // メディアの長さにパディングが含まれる場合は、編集の長さを使う
        let mut moov = make_box(b"mvhd", &header(44_100, 132_300));
        moov.extend(track(b"soun", (44_100, 135_168), Some(&[(132_300, 2112)])));

        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(GaplessInfo {
                delay: 2112,
                total_frames: Some(132_300)
            })
        );
    }

    #[test]
    fn test_skips_video_track_and_empty_edit() {
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"vide", (30_000, 90_000), Some(&[(3000, 0)])));
        moov.extend(track(
            b"soun",
            (48_000, 145_024),
            Some(&[(10, -1), (3000, 1024)]),
        ));

        assert_eq!(
            parse_moov(&moov, 48_000),
            Some(GaplessInfo {
                delay: 1024,
                total_frames: Some(144_000)
            })
        );
    }

    #[test]
    fn test_prefers_itunsmpb() {
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"soun", (44_100, 135_168), Some(&[(3000, 2112)])));
        moov.extend(itunsmpb(
            " 00000000 00000840 000002F4 00000000000204CC 00000000 00000000",
        ));

        assert_eq!(
            parse_moov(&moov, 44_100),
            Some(GaplessInfo {
                delay: 0x840,
                total_frames: Some(0x204CC)
            })
        );
    }

    #[test]
    fn test_returns_none_without_gapless_info() {
        // 編集リストがない
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"soun", (44_100, 132_300), None));
        assert_eq!(parse_moov(&moov, 44_100), None);

        // 編集が複数ある（頭と終わりを切るだけではない）
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(
            b"soun",
            (44_100, 132_300),
            Some(&[(1000, 0), (1000, 88_200)]),
        ));
        assert_eq!(parse_moov(&moov, 44_100), None);

        // 遅延としては長すぎる（曲の途中から再生する編集）
        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"soun", (44_100, 441_000), Some(&[(3000, 220_500)])));
        assert_eq!(parse_moov(&moov, 44_100), None);

        // 壊れたデータでもパニックしない
        assert_eq!(parse_moov(&[0xff; 64], 44_100), None);
        assert_eq!(parse_moov(&[], 44_100), None);
    }

    #[test]
    fn test_read_finds_moov_after_other_boxes() {
        let dir = std::env::temp_dir().join(format!("muspice-mp4-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.m4a");

        let mut moov = make_box(b"mvhd", &header(1000, 3000));
        moov.extend(track(b"soun", (44_100, 133_324), Some(&[(3000, 1024)])));
        let mut file = make_box(b"ftyp", b"M4A \0\0\0\0");
        file.extend(make_box(b"mdat", &[0u8; 256]));
        file.extend(make_box(b"moov", &moov));
        std::fs::write(&path, &file).unwrap();

        assert_eq!(read(&path, 44_100).unwrap().delay, 1024);
        assert_eq!(read(&dir.join("missing.m4a"), 44_100), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
