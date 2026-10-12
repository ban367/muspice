//! APEv2タグ（WavPack・Monkey's Audio）の読み書き
//!
//! lofty（0.25）の汎用タグ（`Tag`）は、APEv2タグのうち、対応する項目（`ItemKey`）があるもの
//! だけを持つ。そのため、汎用タグだけで読み書きすると、次のようになる:
//!
//! - BPM（`BPM`）・評価（`RATING`）・画像（`Cover Art (Front)`など）を読めず、書けない
//! - 保存すると、対応する項目がないもの（上の3つと、ほかのアプリが書いた独自の項目）が
//!   ファイルから消える（汎用タグの内容で、タグ全体を置き換えるため）
//!
//! ここでは、形式ごとのタグ（`ApeTag`）を直接読み書きする。読む時は、上の3つを汎用タグへ
//! 足して返す（`metadata`の読み取りを、ほかの形式と同じに保つ）。保存する時は、ファイルの
//! タグを読み直し、アプリが扱う項目だけを差し替える（ほかの項目は残す。ADR-047）。

use lofty::ape::{APE_PICTURE_TYPES, ApeFile, ApeItem, ApeTag};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::error::{FileEncodingError, FileParseError};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::tag::{ItemKey, ItemValue, MergeTag, SplitTag, Tag, TagExt, TagItem};
use lofty::wavpack::WavPackFile;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// BPMの項目のキー
const BPM_KEY: &str = "BPM";

/// 評価の項目のキー（値は数値だけ。`metadata`の`plain_rating`を参照）
const RATING_KEY: &str = "RATING";

/// APEv2タグを既定のタグとする形式か
pub fn uses_ape_tag(file_type: FileType) -> bool {
    matches!(file_type, FileType::WavPack | FileType::Ape)
}

/// 汎用タグに対応する項目がなく、ここで読み書きする項目
struct Extras {
    bpm: Option<String>,
    rating: Option<String>,
    pictures: Vec<Picture>,
}

/// ファイルを読み、APEv2タグと、それ以外（長さなどの情報・ほかの種類のタグ）に分ける
fn read_parts<R: Read + Seek>(
    reader: &mut R,
    file_type: FileType,
    options: ParseOptions,
) -> Result<(Option<ApeTag>, TaggedFile), FileParseError> {
    reader.rewind()?;
    if file_type == FileType::WavPack {
        let mut file = WavPackFile::read_from(reader, options)?;
        Ok((file.remove_ape(), file.into()))
    } else {
        let mut file = ApeFile::read_from(reader, options)?;
        Ok((file.remove_ape(), file.into()))
    }
}

/// 文字列の項目の値を取り出し、タグから取り除く（複数の値がある場合は最初の値）
fn take_text(ape: &mut ApeTag, key: &str) -> Option<String> {
    let ItemValue::Text(text) = ape.get(key)?.value() else {
        return None;
    };
    let value = text.split('\0').next().unwrap_or_default().to_string();
    ape.remove(key);
    Some(value)
}

/// 画像の項目のキー（`Cover Art (Front)`など）を、大文字・小文字を区別せずに探す
fn picture_key(key: &str) -> Option<&'static str> {
    APE_PICTURE_TYPES
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(key))
}

/// 画像の項目の値（「ファイル名・区切りの0・画像データ」）を、画像として読む
///
/// 画像の種類は、画像データの先頭から判定する（判定できない画像は読まない）。
fn parse_picture(key: &'static str, value: &[u8]) -> Option<Picture> {
    let separator = value.iter().position(|&byte| byte == 0)?;
    let mut picture = Picture::from_reader(&mut &value[separator + 1..]).ok()?;
    picture.set_pic_type(PictureType::from_ape_key(key));
    let name = String::from_utf8_lossy(&value[..separator]);
    picture.set_description((!name.is_empty()).then(|| name.into_owned()));
    Some(picture)
}

/// 画像の項目を取り出し、タグから取り除く（読めなかった画像の項目は、タグに残す）
fn take_pictures(ape: &mut ApeTag) -> Vec<Picture> {
    let mut pictures = Vec::new();
    let mut taken_keys = Vec::new();
    for item in &*ape {
        let (Some(key), ItemValue::Binary(value)) = (picture_key(item.key()), item.value()) else {
            continue;
        };
        if let Some(picture) = parse_picture(key, value) {
            pictures.push(picture);
            taken_keys.push(item.key().to_string());
        }
    }
    for key in taken_keys {
        ape.remove(&key);
    }
    pictures
}

/// ここで読み書きする項目を取り出し、タグから取り除く
fn take_extras(ape: &mut ApeTag) -> Extras {
    Extras {
        bpm: take_text(ape, BPM_KEY),
        rating: take_text(ape, RATING_KEY),
        pictures: take_pictures(ape),
    }
}

/// APEv2タグを持つ形式のファイルを読む
///
/// 返す`TaggedFile`のAPEv2タグ（汎用タグ）には、BPM（`ItemKey::Bpm`）・評価
/// （`ItemKey::Popularimeter`。ファイルに書かれていた値のまま）・画像を足してある。
pub fn read_file<R: Read + Seek>(
    reader: &mut R,
    file_type: FileType,
    options: ParseOptions,
) -> Result<TaggedFile, FileParseError> {
    let (ape, mut tagged_file) = read_parts(reader, file_type, options)?;
    let Some(mut ape) = ape else {
        return Ok(tagged_file);
    };

    let extras = take_extras(&mut ape);
    let (_, mut tag) = ape.split_tag();
    // 対応する項目がないため、検査なしで入れる（`Tag::insert`は受け付けない）
    if let Some(bpm) = extras.bpm {
        tag.insert_unchecked(TagItem::new(ItemKey::Bpm, ItemValue::Text(bpm)));
    }
    if let Some(rating) = extras.rating {
        tag.insert_unchecked(TagItem::new(
            ItemKey::Popularimeter,
            ItemValue::Text(rating),
        ));
    }
    for picture in extras.pictures {
        tag.push_picture(picture);
    }
    tagged_file.insert_tag(tag);
    Ok(tagged_file)
}

/// 汎用タグから、最初の文字列の値を取り出す（その項目は、汎用タグから取り除く）
fn take_tag_text(tag: &mut Tag, key: ItemKey) -> Option<String> {
    let values: Vec<String> = tag.take_strings(key).collect();
    values.into_iter().next()
}

/// 文字列の項目を書き換える（値がなければ取り除く）
fn set_text(ape: &mut ApeTag, key: &str, value: Option<String>) {
    ape.remove(key);
    if let Some(value) = value
        && let Ok(item) = ApeItem::new(key.to_string(), ItemValue::Text(value))
    {
        ape.insert(item);
    }
}

/// 画像を、APEv2タグの項目にする（値は「ファイル名・区切りの0・画像データ」）
///
/// ファイル名のない画像（アプリで埋め込んだ画像）には、キーと拡張子からファイル名を付ける
/// （ファイル名の拡張子から画像の種類を決めるアプリがある。ffmpegなど）。
fn picture_item(picture: &Picture) -> Option<ApeItem> {
    let picture_type = picture.pic_type();
    let key = picture_type.as_ape_key()?;
    let name = match picture.description() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => {
            let extension = picture
                .mime_type()
                .and_then(|mime_type| mime_type.ext())
                .unwrap_or("jpg");
            format!("{key}.{extension}")
        }
    };

    let mut value = name.into_bytes();
    value.push(0);
    value.extend_from_slice(picture.data());
    ApeItem::new(key.to_string(), ItemValue::Binary(value)).ok()
}

/// 汎用タグの内容を、ファイルのAPEv2タグへ書き込む
///
/// ファイルのタグを読み直し、汎用タグが持つ項目（loftyに対応がある項目）と、BPM・評価・画像を
/// 差し替える。それ以外の項目（ほかのアプリが書いた独自の項目・読めなかった画像）は残す。
pub fn save_tag(file_path: &Path, file_type: FileType, tag: &Tag) -> Result<(), FileEncodingError> {
    let mut reader = BufReader::new(File::open(file_path)?);
    let options = ParseOptions::new().read_properties(false);
    let (ape, _) = read_parts(&mut reader, file_type, options)?;
    drop(reader);

    // ファイルのタグから、これから書く項目（今の値）を取り除く。残りが、アプリが扱わない項目
    let mut ape = ape.unwrap_or_default();
    take_extras(&mut ape);
    let (remainder, _) = ape.split_tag();

    let mut tag = tag.clone();
    let bpm =
        take_tag_text(&mut tag, ItemKey::IntegerBpm).or(take_tag_text(&mut tag, ItemKey::Bpm));
    let rating = take_tag_text(&mut tag, ItemKey::Popularimeter);
    let mut pictures = Vec::new();
    while !tag.pictures().is_empty() {
        pictures.push(tag.remove_picture(0));
    }

    let mut merged = remainder.merge_tag(tag);
    set_text(&mut merged, BPM_KEY, bpm);
    set_text(&mut merged, RATING_KEY, rating);
    for picture in &pictures {
        if let Some(item) = picture_item(picture) {
            merged.insert(item);
        }
    }

    merged.save_to_path(file_path, WriteOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::{jpeg, png};
    use crate::metadata::{
        EmbeddedPicture, extract_album_art, extract_all_file_info, read_file_tags,
        set_file_album_art, update_file_metadata, update_file_rating,
    };
    use crate::models::Metadata;
    use lofty::tag::Accessor;
    use std::path::PathBuf;

    /// WavPackとMonkey's Audioのファイル（再生エンジンのテスト用のファイルの写し）
    fn test_files(dir: &Path) -> Vec<(PathBuf, FileType)> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        [
            ("wavpack.wv", FileType::WavPack),
            ("monkey.ape", FileType::Ape),
        ]
        .into_iter()
        .map(|(name, file_type)| {
            let file = dir.join(name);
            std::fs::copy(fixtures.join(name), &file).unwrap();
            (file, file_type)
        })
        .collect()
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muspice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn text_item(key: &str, value: &str) -> ApeItem {
        ApeItem::new(key.to_string(), ItemValue::Text(value.to_string())).unwrap()
    }

    fn binary_item(key: &str, name: &str, data: &[u8]) -> ApeItem {
        let mut value = name.as_bytes().to_vec();
        value.push(0);
        value.extend_from_slice(data);
        ApeItem::new(key.to_string(), ItemValue::Binary(value)).unwrap()
    }

    /// ファイルのAPEv2タグを、そのまま読む
    fn read_ape_tag(file: &Path, file_type: FileType) -> ApeTag {
        let mut reader = BufReader::new(File::open(file).unwrap());
        let options = ParseOptions::new().read_properties(false);
        read_parts(&mut reader, file_type, options)
            .unwrap()
            .0
            .unwrap_or_default()
    }

    fn item_text(ape: &ApeTag, key: &str) -> Option<String> {
        match ape.get(key)?.value() {
            ItemValue::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn item_binary(ape: &ApeTag, key: &str) -> Option<Vec<u8>> {
        match ape.get(key)?.value() {
            ItemValue::Binary(value) => Some(value.clone()),
            _ => None,
        }
    }

    /// WebPの画像（アプリが読まない種類の画像）の先頭
    const WEBP: &[u8] = b"RIFF\x10\x00\x00\x00WEBPVP8 ";

    /// ほかのアプリが書いた形のタグ（キーの大文字・小文字が違う項目・独自の項目を含む）
    fn foreign_tag() -> ApeTag {
        let mut ape = ApeTag::new();
        ape.insert(text_item("Title", "Foreign Title"));
        ape.insert(text_item("Bpm", "127.6"));
        ape.insert(text_item("Rating", "4"));
        ape.insert(text_item("MY CUSTOM ITEM", "keep me"));
        ape.insert(text_item("replaygain_track_gain", "-3.50 dB"));
        ape.insert(binary_item("Cover Art (Front)", "cover.jpg", &jpeg(20, 20)));
        ape.insert(binary_item("COVER ART (BACK)", "裏.png", &png(10, 10)));
        ape.insert(binary_item("Cover Art (Media)", "disc.webp", WEBP));
        ape
    }

    /// ほかのアプリが書いたBPM・評価・画像・ReplayGainを読める
    #[test]
    fn test_reads_items_written_by_other_apps() {
        let dir = temp_dir();
        for (file, file_type) in test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            foreign_tag()
                .save_to_path(&file, WriteOptions::default())
                .unwrap();

            let mut reader = BufReader::new(File::open(&file).unwrap());
            let tagged_file = read_file(&mut reader, file_type, ParseOptions::new()).unwrap();
            let tag = tagged_file.primary_tag().unwrap();
            assert_eq!(tag.title().as_deref(), Some("Foreign Title"), "{name}");
            assert_eq!(tag.get_string(ItemKey::Bpm), Some("127.6"), "{name}");
            assert_eq!(tag.get_string(ItemKey::Popularimeter), Some("4"), "{name}");
            // 画像: 読める種類のものだけ（キーの大文字・小文字は区別しない）
            let pictures = tag.pictures();
            assert_eq!(pictures.len(), 2, "{name}");
            assert_eq!(pictures[0].pic_type(), PictureType::CoverFront, "{name}");
            assert_eq!(pictures[0].data(), jpeg(20, 20), "{name}");
            assert_eq!(pictures[0].description(), Some("cover.jpg"), "{name}");
            assert_eq!(pictures[1].pic_type(), PictureType::CoverBack, "{name}");
            assert_eq!(pictures[1].description(), Some("裏.png"), "{name}");

            // アプリの読み取りでも、同じ値を読む（BPMは整数に丸める）
            assert_eq!(read_file_tags(&file).unwrap().bpm, Some(128), "{name}");
            let info = extract_all_file_info(&file).unwrap();
            assert_eq!(info.rating, 4, "{name}");
            assert_eq!(info.replay_gain.track_gain, Some(-3.5), "{name}");
            let art = extract_album_art(&file).unwrap().unwrap();
            assert_eq!(art.data, jpeg(20, 20), "{name}");
            assert_eq!(art.mime_type, "image/jpeg", "{name}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// タグを書き換えても、アプリが扱わない項目（独自の項目・読めない画像）は残る
    #[test]
    fn test_saving_keeps_items_the_app_does_not_handle() {
        let dir = temp_dir();
        for (file, file_type) in test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            foreign_tag()
                .save_to_path(&file, WriteOptions::default())
                .unwrap();

            // 評価: 星の数で書かれていたので、その付け方のまま書く（項目は増やさない）
            update_file_rating(&file, 5).unwrap();
            let ape = read_ape_tag(&file, file_type);
            assert_eq!(item_text(&ape, "RATING").as_deref(), Some("5"), "{name}");
            let rating_items = (&ape)
                .into_iter()
                .filter(|item| item.key().eq_ignore_ascii_case("RATING"))
                .count();
            assert_eq!(rating_items, 1, "{name}");
            assert_eq!(item_text(&ape, "BPM").as_deref(), Some("127.6"), "{name}");

            // 画像: フロントカバーだけを置き換える（ファイル名は、新しい画像の種類に合わせる）
            let art = EmbeddedPicture {
                data: png(30, 30),
                mime_type: "image/png".to_string(),
            };
            set_file_album_art(&file, &art).unwrap();
            let ape = read_ape_tag(&file, file_type);
            let mut front = b"Cover Art (Front).png\0".to_vec();
            front.extend_from_slice(&png(30, 30));
            assert_eq!(
                item_binary(&ape, "Cover Art (Front)"),
                Some(front),
                "{name}"
            );
            let mut back = "裏.png\0".as_bytes().to_vec();
            back.extend_from_slice(&png(10, 10));
            assert_eq!(item_binary(&ape, "Cover Art (Back)"), Some(back), "{name}");

            // 項目: 値を変え、BPMを取り除く
            let metadata = Metadata {
                title: Some("New Title".to_string()),
                ..Default::default()
            };
            update_file_metadata(&file, &metadata, true).unwrap();
            let ape = read_ape_tag(&file, file_type);
            assert_eq!(item_text(&ape, "Title").as_deref(), Some("New Title"));
            assert!(ape.get("BPM").is_none(), "{name}");
            assert_eq!(item_text(&ape, "RATING").as_deref(), Some("5"), "{name}");

            // アプリが扱わない項目は、どの書き換えの後も残る
            assert_eq!(
                item_text(&ape, "MY CUSTOM ITEM").as_deref(),
                Some("keep me"),
                "{name}"
            );
            let mut media = b"disc.webp\0".to_vec();
            media.extend_from_slice(WEBP);
            assert_eq!(
                item_binary(&ape, "Cover Art (Media)"),
                Some(media),
                "{name}"
            );
            // loftyに対応がある項目（ReplayGainなど）も、そのまま残る
            assert_eq!(
                extract_all_file_info(&file).unwrap().replay_gain.track_gain,
                Some(-3.5),
                "{name}"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// タグのないファイルへ書くと、ほかのアプリが読める項目になる
    #[test]
    fn test_writes_items_other_apps_can_read() {
        let dir = temp_dir();
        for (file, file_type) in test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let metadata = Metadata {
                title: Some("Title".to_string()),
                bpm: Some(128),
                ..Default::default()
            };
            update_file_metadata(&file, &metadata, true).unwrap();
            update_file_rating(&file, 4).unwrap();
            let art = EmbeddedPicture {
                data: jpeg(40, 40),
                mime_type: "image/jpeg".to_string(),
            };
            set_file_album_art(&file, &art).unwrap();

            let ape = read_ape_tag(&file, file_type);
            assert_eq!(item_text(&ape, "BPM").as_deref(), Some("128"), "{name}");
            // 評価は20刻み（MusicBeeなど）
            assert_eq!(item_text(&ape, "RATING").as_deref(), Some("80"), "{name}");
            let mut front = b"Cover Art (Front).jpg\0".to_vec();
            front.extend_from_slice(&jpeg(40, 40));
            assert_eq!(
                item_binary(&ape, "Cover Art (Front)"),
                Some(front),
                "{name}"
            );

            // 評価を外すと、項目ごとなくなる
            update_file_rating(&file, 0).unwrap();
            assert!(read_ape_tag(&file, file_type).get("RATING").is_none());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
