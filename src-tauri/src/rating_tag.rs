//! 評価（半分の星）のタグの読み書き
//!
//! アプリは評価を、半分の星の単位（0〜10。7は星3.5。0は評価なし）で持つ。半分の星を決めた規格は
//! なく、MusicBee・MediaMonkeyと同じ値で、標準の評価のタグに書く（ADR-082）。ほかのプレーヤーは、
//! 整数の星に丸めて読む。
//!
//! - ID3v2の`POPM`（0〜255）: 星0.5〜5を、13・1・54・64・118・128・186・196・242・255で書く
//!   （整数の星は、Windows Media Playerと同じ値）。読む時は、Kodiと同じしきい値で半分の星にする。
//!   Picard（`users@musicbrainz.org`）が書いた評価だけは、Picardの付け方（星1つが51）で読み書きする
//! - MP4の`rate`、Vorbisコメント・APEv2タグの数値だけの`RATING`（0〜100）: 10刻み（星3.5は70）
//!
//! lofty（0.25）は、評価を整数の星（`StarRating`）にしてから汎用タグ（`Tag`）へ入れ、汎用タグを
//! 書く時も整数の星の値で書く。そのため、汎用タグだけで読み書きすると、半分の星を読めず、評価以外の
//! 項目を保存しただけでも、ファイルの半分の星が整数の星に書き換わる。ここでは、ID3v2・MP4について、
//! 形式ごとのタグから元の値を読み（`read_file`）、半分の星がある時は、形式ごとのタグを組み立てて
//! 書く（`save_tag`）。
//!
//! 汎用タグの中では、評価（`ItemKey::Popularimeter`）を次の文字列で持つ。
//!
//! - ID3v2・MP4・RIFF INFO: loftyと同じ`書き手|星の数|再生回数`。半分の星は`MusicBee|3.5|0`のように
//!   小数で書く（loftyは読めないため、そのまま汎用タグで保存させない）
//! - Vorbisコメント・APEv2タグ: ファイルに書かれていた数値のまま（`70`・`4`）。保存は`vorbis_tag`・
//!   `ape_tag`が行う

use lofty::aac::AacFile;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::error::{FileEncodingError, FileParseError};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, PopularimeterFrame};
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::WavFile;
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::tag::{ItemKey, ItemValue, Tag, TagExt, TagItem, TagType};
use std::borrow::Cow;
use std::io::{Read, Seek};
use std::path::Path;

/// 評価として受け付ける範囲（半分の星の単位。0は評価なし、10は星5つ）
pub const RATING_RANGE: std::ops::RangeInclusive<i32> = 0..=10;

/// 星5つの評価（半分の星の単位）
const MAX_RATING: i32 = 10;

/// 評価を書いた書き手がない時に使う書き手（loftyと同じ）
const DEFAULT_EMAIL: &str = "MusicBee";

/// Picardが`POPM`に書く書き手（星1つを51で書く）
const PICARD_EMAIL: &str = "users@musicbrainz.org";

/// `POPM`に書く値（半分の星の単位ごと。0は使わない）
///
/// MusicBee・MediaMonkey 4と同じ値。整数の星は、Windows Media Playerと同じ値（星1つが1になる）。
const POPM_VALUES: [u8; 11] = [0, 13, 1, 54, 64, 118, 128, 186, 196, 242, 255];

/// MP4の評価の項目
const MP4_RATE: AtomIdent<'static> = AtomIdent::Fourcc(*b"rate");

/// `POPM`のフレームのID
fn popm_id() -> FrameId<'static> {
    FrameId::Valid(Cow::Borrowed("POPM"))
}

/// `POPM`の値（0〜255）を、半分の星の単位にする（0は評価なし）
///
/// Kodiと同じしきい値（MediaMonkey 4・Windows Media Playerが書く値を、それぞれの星にする）。
/// 書き手がPicardの場合は、Picardの付け方（星1つが51）で読む。
pub fn popm_to_rating(email: &str, value: u8) -> i32 {
    if email == PICARD_EMAIL && value > 0 {
        return (f64::from(value) / 25.5).round().clamp(1.0, 10.0) as i32;
    }
    match value {
        0 => 0,
        1 => 2,
        2..=22 => 1,
        23..=31 => 2,
        32..=63 => 3,
        64..=95 => 4,
        96..=127 => 5,
        128..=159 => 6,
        160..=195 => 7,
        196..=223 => 8,
        224..=254 => 9,
        255 => 10,
    }
}

/// 半分の星の単位を、`POPM`の値にする（`rating`は1〜10）
pub fn rating_to_popm(email: &str, rating: i32) -> u8 {
    let rating = rating.clamp(1, MAX_RATING);
    if email == PICARD_EMAIL {
        return (f64::from(rating) * 25.5).round() as u8;
    }
    POPM_VALUES[rating as usize]
}

/// 0〜100の値（10刻み）を、半分の星の単位にする（0は評価なし。100を超える値は星5つ）
fn percent_to_rating(value: f64) -> i32 {
    if value <= 0.0 {
        return 0;
    }
    (value / 10.0).round().clamp(1.0, 10.0) as i32
}

/// 数値だけの`RATING`（Vorbisコメント・APEv2タグ）の値の付け方
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlainScale {
    /// 星の数をそのまま書く（1〜5）
    Stars,
    /// 0〜100で書く（MusicBee・MediaMonkeyなど。こちらが多い）
    Percent,
}

/// `RATING`の値（数値だけの文字列）を、半分の星の単位と付け方に解釈する
///
/// - 1〜5: 星の数（`3.5`のような半分の星も読む）
/// - 5を超え100まで: 0〜100（10刻み。75のような値は、近い方の半分の星にする）
/// - 0より大きく1未満の小数: 0〜1の割合（例: 0.7 → 星3.5）
///
/// 0・範囲外・数値でない値はNone（評価なし）。
pub fn parse_plain(value: &str) -> Option<(i32, PlainScale)> {
    let number: f64 = value.trim().parse().ok()?;
    if !number.is_finite() || number <= 0.0 || number > 100.0 {
        return None;
    }
    if number < 1.0 {
        return Some((percent_to_rating(number * 100.0), PlainScale::Percent));
    }
    if number <= 5.0 {
        let rating = (number * 2.0).round().clamp(1.0, 10.0) as i32;
        return Some((rating, PlainScale::Stars));
    }
    Some((percent_to_rating(number), PlainScale::Percent))
}

/// 半分の星の単位を、数値だけの`RATING`の値にする（`rating`は1〜10）
///
/// 星の数で書かれていたファイルは、整数の星ならその付け方を保つ。半分の星は、星の数で書くと
/// 読めないアプリがあるため、0〜100で書く（MusicBee・MediaMonkeyと同じ）。
fn format_plain(rating: i32, scale: Option<PlainScale>) -> String {
    match scale {
        Some(PlainScale::Stars) if rating % 2 == 0 => (rating / 2).to_string(),
        _ => (rating * 10).to_string(),
    }
}

/// 評価を、数値だけの`RATING`として読み書きするタグの種類か
///
/// Vorbisコメント（FLAC・Oggなど）とAPEv2タグ（WavPack・Monkey's Audio）。どちらも、多くのアプリが
/// 書き手を付けない`RATING=80`の形で書く。
fn uses_plain_rating(tag_type: TagType) -> bool {
    matches!(tag_type, TagType::VorbisComments | TagType::Ape)
}

/// 書き手の付いた評価（汎用タグの`書き手|星の数|再生回数`）
#[derive(Debug, Clone, PartialEq)]
struct WriterRating {
    email: String,
    /// 半分の星の単位（1〜10）
    rating: i32,
    play_counter: u64,
}

impl WriterRating {
    /// 汎用タグの文字列を読む（loftyの形と、半分の星を小数で書いた形）
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, '|');
        let email = parts.next()?;
        let stars: f64 = parts.next()?.parse().ok()?;
        let play_counter = parts.next()?.parse().ok()?;
        let rating = (stars * 2.0).round();
        if !(1.0..=10.0).contains(&rating) {
            return None;
        }
        Some(Self {
            email: email.to_string(),
            rating: rating as i32,
            play_counter,
        })
    }

    /// 汎用タグの文字列にする（整数の星はloftyと同じ形。半分の星は`3.5`のように書く）
    fn format(&self) -> String {
        let stars = if self.rating % 2 == 0 {
            (self.rating / 2).to_string()
        } else {
            format!("{}.5", self.rating / 2)
        };
        format!("{}|{}|{}", self.email, stars, self.play_counter)
    }

    /// loftyが読めない（半分の星の）評価か
    fn is_half_star(&self) -> bool {
        self.rating % 2 != 0
    }
}

/// 汎用タグにある、評価の項目の文字列
fn rating_texts(tag: &Tag) -> impl Iterator<Item = &str> {
    tag.get_strings(ItemKey::Popularimeter)
}

/// タグから評価（半分の星の単位）を読み取る。評価のタグがない場合は0
///
/// 書き手の付いた評価を先に見て、なければ数値だけの`RATING`（Vorbisコメント・APEv2タグ）を見る。
/// 複数ある場合は最初の評価を使う。
pub fn read_rating(tag: &Tag) -> i32 {
    rating_texts(tag)
        .find_map(WriterRating::parse)
        .map(|writer| writer.rating)
        .or_else(|| plain_rating(tag).map(|(rating, _)| rating))
        .unwrap_or(0)
}

/// 数値だけの`RATING`を読む（Vorbisコメント・APEv2タグだけ）
///
/// lofty（0.25）は、Vorbisコメントの`RATING`を汎用タグへ文字列のまま入れるだけで、
/// `Tag::ratings()`では読めない（読めるのは`RATING:書き手`の形だけ）。APEv2タグの`RATING`は、
/// `ape_tag`が同じ形で汎用タグへ入れる。
fn plain_rating(tag: &Tag) -> Option<(i32, PlainScale)> {
    if !uses_plain_rating(tag.tag_type()) {
        return None;
    }
    rating_texts(tag).find_map(parse_plain)
}

/// タグの評価を書き換える（`rating`は半分の星の単位。0は評価のタグを取り除く）
///
/// すでに評価がある場合は、その書き手と再生回数を引き継ぎ、星の数だけを変える
/// （書き込んだアプリが、自分の付け方のまま読めるようにする）。ない場合は、MusicBeeの書き手で書く。
///
/// Vorbisコメント（FLACなど）とAPEv2タグは、書き手を付けない`RATING`に数値だけを書く。
/// lofty（0.25）は汎用タグの評価をVorbisコメントへ変換せずに書き出すため、loftyの形の
/// まま入れると、他のアプリが読めない値（`RATING=MusicBee|4|0`）がファイルに書かれる。
/// APEv2タグは、loftyに対応する項目がないため、検査なしで入れる（保存時に`ape_tag`が書く）。
pub fn set_rating(tag: &mut Tag, rating: i32) {
    let has_rating = RATING_RANGE.contains(&rating) && rating > 0;
    if uses_plain_rating(tag.tag_type()) {
        // すでに星の数（1〜5）で書かれている場合はその付け方を保ち、それ以外は0〜100で書く
        let scale = plain_rating(tag).map(|(_, scale)| scale);
        tag.remove_key(ItemKey::Popularimeter);
        if has_rating {
            tag.insert_unchecked(TagItem::new(
                ItemKey::Popularimeter,
                ItemValue::Text(format_plain(rating, scale)),
            ));
        }
        return;
    }

    let existing = rating_texts(tag).find_map(WriterRating::parse);
    tag.remove_key(ItemKey::Popularimeter);
    if !has_rating {
        return;
    }
    let writer = match existing {
        Some(current) => WriterRating { rating, ..current },
        None => WriterRating {
            email: DEFAULT_EMAIL.to_string(),
            rating,
            play_counter: 0,
        },
    };
    // 半分の星はloftyが読めない形のため、項目の検査をしない`insert_unchecked`で入れる
    tag.insert_unchecked(TagItem::new(
        ItemKey::Popularimeter,
        ItemValue::Text(writer.format()),
    ));
}

/// 評価を、loftyが丸める前の値で読む形式か（ID3v2を既定のタグにする形式と、MP4）
pub fn reads_raw_rating(file_type: FileType) -> bool {
    matches!(
        file_type,
        FileType::Mpeg | FileType::Aac | FileType::Wav | FileType::Aiff | FileType::Mp4
    )
}

/// ID3v2の`POPM`の、元の値
struct RawPopm {
    email: String,
    value: u8,
    counter: u64,
}

/// ID3v2のタグにある`POPM`を、元の値のまま取り出す
fn raw_popms(tag: Option<&Id3v2Tag>) -> Vec<RawPopm> {
    let Some(tag) = tag else {
        return Vec::new();
    };
    tag.into_iter()
        .filter_map(|frame| match frame {
            Frame::Popularimeter(popm) => Some(RawPopm {
                email: popm.email.to_string(),
                value: popm.rating,
                counter: popm.counter,
            }),
            _ => None,
        })
        .collect()
}

/// MP4の`rate`を、元の値のまま読む（loftyが評価として読む、文字列で書かれた値だけ）
fn raw_mp4_rate(ilst: Option<&Ilst>) -> Option<u8> {
    ilst?.get(&MP4_RATE)?.data().find_map(|data| match data {
        AtomData::UTF8(text) | AtomData::UTF16(text) => text.trim().parse().ok(),
        _ => None,
    })
}

/// ID3v2・MP4のファイルを読み、汎用タグの評価を、loftyが丸める前の値で置き換えて返す
pub fn read_file<R: Read + Seek>(
    reader: &mut R,
    file_type: FileType,
    options: ParseOptions,
) -> Result<TaggedFile, FileParseError> {
    reader.rewind()?;
    let mut popms = Vec::new();
    let mut mp4_rate = None;
    let mut tagged_file: TaggedFile = match file_type {
        FileType::Mpeg => {
            let file = MpegFile::read_from(reader, options)?;
            popms = raw_popms(file.id3v2());
            file.into()
        }
        FileType::Aac => {
            let file = AacFile::read_from(reader, options)?;
            popms = raw_popms(file.id3v2());
            file.into()
        }
        FileType::Wav => {
            let file = WavFile::read_from(reader, options)?;
            popms = raw_popms(file.id3v2());
            file.into()
        }
        FileType::Aiff => {
            let file = AiffFile::read_from(reader, options)?;
            popms = raw_popms(file.id3v2());
            file.into()
        }
        FileType::Mp4 => {
            let file = Mp4File::read_from(reader, options)?;
            mp4_rate = raw_mp4_rate(file.ilst());
            file.into()
        }
        _ => lofty::probe::Probe::with_file_type(reader, file_type)
            .options(options)
            .read()?,
    };

    if let Some(tag) = tagged_file.tag_mut(TagType::Id3v2)
        && !popms.is_empty()
    {
        // loftyは`POPM`を1つずつ汎用タグの評価にする（書き手のない評価は、MusicBeeの書き手にする）
        tag.remove_key(ItemKey::Popularimeter);
        for popm in popms {
            let rating = popm_to_rating(&popm.email, popm.value);
            if rating == 0 {
                continue;
            }
            let email = if popm.email.is_empty() {
                DEFAULT_EMAIL.to_string()
            } else {
                popm.email
            };
            let writer = WriterRating {
                email,
                rating,
                play_counter: popm.counter,
            };
            tag.push_unchecked(TagItem::new(
                ItemKey::Popularimeter,
                ItemValue::Text(writer.format()),
            ));
        }
    }

    // loftyが`rate`を評価として読んだ時（文字列で書かれた値）だけ、元の値で置き換える
    if let (Some(tag), Some(rate)) = (tagged_file.tag_mut(TagType::Mp4Ilst), mp4_rate) {
        let current = rating_texts(tag).find_map(WriterRating::parse);
        if let Some(writer) = current {
            let rating = percent_to_rating(f64::from(rate));
            tag.remove_key(ItemKey::Popularimeter);
            if rating > 0 {
                tag.insert_unchecked(TagItem::new(
                    ItemKey::Popularimeter,
                    ItemValue::Text(WriterRating { rating, ..writer }.format()),
                ));
            }
        }
    }

    Ok(tagged_file)
}

/// 汎用タグにある、半分の星の評価（loftyが書けないもの）
fn half_star_ratings(tag: &Tag) -> Vec<WriterRating> {
    rating_texts(tag)
        .filter_map(WriterRating::parse)
        .filter(WriterRating::is_half_star)
        .collect()
}

/// 半分の星の評価がある汎用タグを、評価を整数の星（切り上げ）にして返す（なければNone）
///
/// 半分の星を書けないタグ（RIFF INFOの`IRTD`。loftyが20刻みで書く）へ書く時に使う。
/// ほかのプレーヤーが半分の星を整数の星に丸めて読むのと同じく、切り上げる。
fn with_whole_stars(tag: &Tag) -> Option<Tag> {
    let writers: Vec<WriterRating> = rating_texts(tag).filter_map(WriterRating::parse).collect();
    if !writers.iter().any(WriterRating::is_half_star) {
        return None;
    }
    let mut tag = tag.clone();
    tag.remove_key(ItemKey::Popularimeter);
    for writer in writers {
        let whole = WriterRating {
            rating: writer.rating + writer.rating % 2,
            ..writer
        };
        tag.push_unchecked(TagItem::new(
            ItemKey::Popularimeter,
            ItemValue::Text(whole.format()),
        ));
    }
    Some(tag)
}

/// 半分の星の評価があるID3v2のタグを、形式ごとのタグにして返す（なければNone）
///
/// 評価以外は、loftyが汎用タグから書く時と同じ変換（`Id3v2Tag::from`）にし、`POPM`だけを
/// 元の値で入れる。
fn id3v2_for_save(tag: &Tag) -> Option<Id3v2Tag> {
    let writers: Vec<WriterRating> = rating_texts(tag).filter_map(WriterRating::parse).collect();
    if !writers.iter().any(WriterRating::is_half_star) {
        return None;
    }
    let mut without_ratings = tag.clone();
    without_ratings.remove_key(ItemKey::Popularimeter);
    let mut id3v2 = Id3v2Tag::from(without_ratings);
    let _ = id3v2.remove(&popm_id());
    for writer in writers {
        let value = rating_to_popm(&writer.email, writer.rating);
        id3v2.insert(Frame::Popularimeter(PopularimeterFrame::new(
            writer.email,
            value,
            writer.play_counter,
        )));
    }
    Some(id3v2)
}

/// 半分の星の評価があるMP4のタグを、形式ごとのタグにして返す（なければNone）
///
/// 評価以外は、loftyが汎用タグから書く時と同じ変換（`Ilst::from`）にし、`rate`だけを
/// 0〜100の値（10刻み）で入れる。loftyと同じく、文字列で書く。
fn ilst_for_save(tag: &Tag) -> Option<Ilst> {
    let half = half_star_ratings(tag).into_iter().next()?;
    let mut without_ratings = tag.clone();
    without_ratings.remove_key(ItemKey::Popularimeter);
    let mut ilst = Ilst::from(without_ratings);
    ilst.replace_atom(Atom::new(
        MP4_RATE,
        AtomData::UTF8((half.rating * 10).to_string()),
    ));
    Some(ilst)
}

/// 汎用タグを、ファイルへ保存する（半分の星の評価を、そのタグの書き方で書く）
///
/// 半分の星がなければ、loftyの汎用タグの保存と同じ。ID3v2・MP4に半分の星がある時は、形式ごとの
/// タグにして保存する。半分の星を書けないタグ（RIFF INFO）は、整数の星に切り上げて保存する。
pub fn save_tag(tag: &Tag, path: &Path, options: WriteOptions) -> Result<(), FileEncodingError> {
    match tag.tag_type() {
        TagType::Id3v2 => match id3v2_for_save(tag) {
            Some(id3v2) => id3v2.save_to_path(path, options),
            None => tag.save_to_path(path, options),
        },
        TagType::Mp4Ilst => match ilst_for_save(tag) {
            Some(ilst) => ilst.save_to_path(path, options),
            None => tag.save_to_path(path, options),
        },
        _ => match with_whole_stars(tag) {
            Some(whole) => whole.save_to_path(path, options),
            None => tag.save_to_path(path, options),
        },
    }
}

/// 汎用タグから、評価の項目を除いたもの（loftyの変換で、読めない評価の警告を出させない）
pub fn without_ratings(tag: &Tag) -> Tag {
    let mut tag = tag.clone();
    tag.remove_key(ItemKey::Popularimeter);
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popm_values_round_trip() {
        // MusicBee・MediaMonkey 4が書く値（お持ちのライブラリで確かめた値）を、その星で読み、同じ値で書く
        for (value, rating) in [
            (13, 1),
            (1, 2),
            (54, 3),
            (64, 4),
            (118, 5),
            (128, 6),
            (186, 7),
            (196, 8),
            (242, 9),
            (255, 10),
        ] {
            assert_eq!(popm_to_rating("MusicBee", value), rating, "{value}");
            assert_eq!(rating_to_popm("MusicBee", rating), value, "{rating}");
            assert_eq!(
                popm_to_rating("Windows Media Player 9 Series", value),
                rating,
                "{value}"
            );
        }
        assert_eq!(popm_to_rating("MusicBee", 0), 0);
    }

    #[test]
    fn test_popm_thresholds_follow_kodi() {
        // ほかのアプリが書いた途中の値は、Kodiと同じしきい値で読む
        assert_eq!(popm_to_rating("no@email", 2), 1);
        assert_eq!(popm_to_rating("no@email", 22), 1);
        assert_eq!(popm_to_rating("no@email", 23), 2);
        assert_eq!(popm_to_rating("no@email", 32), 3);
        assert_eq!(popm_to_rating("no@email", 100), 5);
        assert_eq!(popm_to_rating("no@email", 160), 7);
        assert_eq!(popm_to_rating("no@email", 224), 9);
        assert_eq!(popm_to_rating("", 254), 9);
    }

    #[test]
    fn test_picard_popm_uses_its_own_scale() {
        for (value, rating) in [(51, 2), (102, 4), (153, 6), (204, 8), (255, 10)] {
            assert_eq!(popm_to_rating(PICARD_EMAIL, value), rating, "{value}");
            assert_eq!(rating_to_popm(PICARD_EMAIL, rating), value, "{rating}");
        }
        assert_eq!(rating_to_popm(PICARD_EMAIL, 7), 179);
        assert_eq!(popm_to_rating(PICARD_EMAIL, 179), 7);
    }

    #[test]
    fn test_parse_plain() {
        use PlainScale::{Percent, Stars};

        // 0〜100（10刻み。半分の星を含む）
        assert_eq!(parse_plain("20"), Some((2, Percent)));
        assert_eq!(parse_plain("70"), Some((7, Percent)));
        assert_eq!(parse_plain("80"), Some((8, Percent)));
        assert_eq!(parse_plain("90"), Some((9, Percent)));
        assert_eq!(parse_plain("100"), Some((10, Percent)));
        assert_eq!(parse_plain("10"), Some((1, Percent)));
        assert_eq!(parse_plain("75"), Some((8, Percent)));
        // 星の数（1〜5。小数の半分の星も読む）
        assert_eq!(parse_plain("1"), Some((2, Stars)));
        assert_eq!(parse_plain(" 4 "), Some((8, Stars)));
        assert_eq!(parse_plain("5"), Some((10, Stars)));
        assert_eq!(parse_plain("3.5"), Some((7, Stars)));
        // 0〜1の割合
        assert_eq!(parse_plain("0.8"), Some((8, Percent)));
        assert_eq!(parse_plain("0.7"), Some((7, Percent)));
        assert_eq!(parse_plain("0.01"), Some((1, Percent)));
        // 評価なし・読めない値
        assert_eq!(parse_plain("0"), None);
        assert_eq!(parse_plain("101"), None);
        assert_eq!(parse_plain("-1"), None);
        assert_eq!(parse_plain("MusicBee|4|0"), None);
        assert_eq!(parse_plain(""), None);
    }

    #[test]
    fn test_format_plain_keeps_star_scale_only_for_whole_stars() {
        assert_eq!(format_plain(8, Some(PlainScale::Stars)), "4");
        assert_eq!(format_plain(7, Some(PlainScale::Stars)), "70");
        assert_eq!(format_plain(7, Some(PlainScale::Percent)), "70");
        assert_eq!(format_plain(10, None), "100");
        assert_eq!(format_plain(1, None), "10");
    }

    #[test]
    fn test_writer_rating_text() {
        let half = WriterRating::parse("MusicBee|3.5|7").unwrap();
        assert_eq!(half.rating, 7);
        assert_eq!(half.play_counter, 7);
        assert!(half.is_half_star());
        assert_eq!(half.format(), "MusicBee|3.5|7");

        // 整数の星は、loftyと同じ形
        let whole = WriterRating::parse("Windows Media Player 9 Series|4|0").unwrap();
        assert_eq!(whole.rating, 8);
        assert_eq!(whole.format(), "Windows Media Player 9 Series|4|0");
        assert_eq!(WriterRating::parse("MusicBee|0.5|0").unwrap().rating, 1);

        assert_eq!(WriterRating::parse("MusicBee|0|0"), None);
        assert_eq!(WriterRating::parse("MusicBee|6|0"), None);
        assert_eq!(WriterRating::parse("80"), None);
    }

    #[test]
    fn test_set_and_read_rating_in_every_tag_type() {
        for tag_type in [
            TagType::Id3v2,
            TagType::Mp4Ilst,
            TagType::VorbisComments,
            TagType::Ape,
            TagType::RiffInfo,
        ] {
            let mut tag = Tag::new(tag_type);
            assert_eq!(read_rating(&tag), 0, "{tag_type:?}");
            for rating in 1..=10 {
                set_rating(&mut tag, rating);
                assert_eq!(read_rating(&tag), rating, "{tag_type:?} {rating}");
                // 書き直しても評価は1つだけ
                assert_eq!(rating_texts(&tag).count(), 1, "{tag_type:?}");
            }
            set_rating(&mut tag, 0);
            assert_eq!(read_rating(&tag), 0, "{tag_type:?}");
            assert_eq!(tag.get_string(ItemKey::Popularimeter), None, "{tag_type:?}");
        }
    }

    #[test]
    fn test_set_rating_keeps_writer_and_play_counter() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(
            ItemKey::Popularimeter,
            "Windows Media Player 9 Series|2|7".to_string(),
        );
        set_rating(&mut tag, 7);
        assert_eq!(
            tag.get_string(ItemKey::Popularimeter),
            Some("Windows Media Player 9 Series|3.5|7")
        );
    }

    /// Vorbisコメントには、他のアプリが読める数値だけの`RATING`を書く
    #[test]
    fn test_vorbis_rating_is_written_as_plain_number() {
        let dump = |tag: &Tag| {
            let mut bytes = Vec::new();
            tag.dump_to(&mut bytes, WriteOptions::default()).unwrap();
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let mut tag = Tag::new(TagType::VorbisComments);
        set_rating(&mut tag, 8);
        let written = dump(&tag);
        assert!(written.contains("RATING=80"), "{written:?}");
        assert!(!written.contains("MusicBee"), "{written:?}");
        assert_eq!(read_rating(&tag), 8);

        set_rating(&mut tag, 7);
        assert!(dump(&tag).contains("RATING=70"));

        // 評価を外すと、RATINGそのものがなくなる
        set_rating(&mut tag, 0);
        assert!(!dump(&tag).contains("RATING"));
    }

    /// 他のアプリが書いた数値だけの`RATING`を読み、星の数で書かれていればその付け方を保つ
    #[test]
    fn test_vorbis_rating_reads_plain_values_and_keeps_star_scale() {
        // 0〜100（MusicBeeなど）
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "60".to_string());
        assert_eq!(read_rating(&tag), 6);
        set_rating(&mut tag, 10);
        assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("100"));

        // 星の数（整数の星は星の数のまま、半分の星は0〜100で書く）
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "4".to_string());
        assert_eq!(read_rating(&tag), 8);
        set_rating(&mut tag, 4);
        assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("2"));
        set_rating(&mut tag, 5);
        assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("50"));

        // 以前のバージョンが書いたloftyの形も読め、書き直すと数値だけになる
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::Popularimeter, "MusicBee|3|0".to_string());
        assert_eq!(read_rating(&tag), 6);
        set_rating(&mut tag, 6);
        assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("60"));

        // 数値だけの値の解釈は、VorbisコメントとAPEv2タグだけに使う
        let mut id3 = Tag::new(TagType::Id3v2);
        id3.insert_text(ItemKey::Popularimeter, "80".to_string());
        assert_eq!(read_rating(&id3), 0);
    }

    #[test]
    fn test_with_whole_stars_rounds_half_stars_up() {
        let mut tag = Tag::new(TagType::RiffInfo);
        set_rating(&mut tag, 7);
        let whole = with_whole_stars(&tag).unwrap();
        assert_eq!(
            whole.get_string(ItemKey::Popularimeter),
            Some("MusicBee|4|0")
        );
        assert_eq!(read_rating(&whole), 8);

        // 半分の星がなければ、写さない
        set_rating(&mut tag, 8);
        assert!(with_whole_stars(&tag).is_none());
    }
}
