//! Vorbisコメント（FLAC・Ogg Vorbis・Opus）の保存
//!
//! lofty（0.25）の汎用タグ（`Tag`）は、Vorbisコメントのうち、対応する項目（`ItemKey`）があるもの
//! だけを持ち、保存するとコメント全体を汎用タグの内容で置き換える。そのため、汎用タグだけで
//! 保存すると、ほかのアプリが書いた独自の項目がファイルから消える。
//!
//! ここでは、保存する時にファイルのコメントを読み直し、汎用タグが持つ項目だけを差し替える
//! （ほかの項目は残す。APEv2タグの`ape_tag`と同じ考え方。ADR-047）。読み取りは、汎用タグで
//! 足りるため扱わない。

use lofty::config::{ParseOptions, WriteOptions};
use lofty::error::{FileEncodingError, FileParseError};
use lofty::file::{AudioFile, FileType};
use lofty::flac::FlacFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, SpeexFile, VorbisFile};
use lofty::picture::PictureInformation;
use lofty::tag::{ItemKey, ItemValue, MergeTag, SplitTag, Tag, TagExt, TagType};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Vorbisコメントを既定のタグとする形式か
pub fn uses_vorbis_comments(file_type: FileType) -> bool {
    matches!(
        file_type,
        FileType::Flac | FileType::Vorbis | FileType::Opus | FileType::Speex
    )
}

/// ファイルのVorbisコメントを読む（コメントのないFLACはNone）
fn read_comments<R: Read + Seek>(
    reader: &mut R,
    file_type: FileType,
    options: ParseOptions,
) -> Result<Option<VorbisComments>, FileParseError> {
    reader.rewind()?;
    Ok(match file_type {
        FileType::Flac => FlacFile::read_from(reader, options)?.remove_vorbis_comments(),
        FileType::Vorbis => Some(VorbisFile::read_from(reader, options)?.remove_vorbis_comments()),
        FileType::Opus => Some(OpusFile::read_from(reader, options)?.remove_vorbis_comments()),
        FileType::Speex => Some(SpeexFile::read_from(reader, options)?.remove_vorbis_comments()),
        _ => None,
    })
}

/// 数値だけの値か（アプリが書く形の評価。`metadata`の`plain_rating`を参照）
fn is_plain_number(value: &str) -> bool {
    value.trim().parse::<f64>().is_ok()
}

/// 汎用タグの内容を、ファイルのVorbisコメントへ書き込む
///
/// ファイルのコメントを読み直し、汎用タグが持つ項目（loftyに対応がある項目）と画像を差し替える。
/// それ以外の項目（ほかのアプリが書いた独自の項目）と、ベンダーの文字列は残す。
pub fn save_tag(file_path: &Path, file_type: FileType, tag: &Tag) -> Result<(), FileEncodingError> {
    let mut reader = BufReader::new(File::open(file_path)?);
    // 画像は汎用タグが持っているため、読み直さない
    let options = ParseOptions::new()
        .read_properties(false)
        .read_cover_art(false);
    let existing = read_comments(&mut reader, file_type, options)?.unwrap_or_default();
    drop(reader);

    // 汎用タグは、エンコーダーの項目（`ENCODER`）がないファイルでは、ベンダーの文字列をその項目
    // として持つ。そのまま書くと、ファイルになかった`ENCODER`が増えるため、あった場合だけ書く
    let had_encoder_item = existing.items().any(|(key, _)| {
        ItemKey::from_key(TagType::VorbisComments, key) == Some(ItemKey::EncoderSoftware)
    });
    // 汎用タグが持つ項目（今の値）を取り除く。残りが、アプリが扱わない項目
    let (remainder, _) = existing.split_tag();

    // 書き手の付いた評価（`RATING:書き手`。汎用タグではloftyの形の文字列）は、loftyの変換で
    // 元の形へ書き戻す。それ以外の項目は、値をそのまま書く
    let mut writer_ratings = Tag::new(TagType::VorbisComments);
    let mut items = Vec::new();
    for item in tag.items() {
        let (ItemValue::Text(value) | ItemValue::Locator(value)) = item.value() else {
            continue;
        };
        let key = item.key();
        if key == ItemKey::EncoderSoftware && !had_encoder_item {
            continue;
        }
        if key == ItemKey::Popularimeter && !is_plain_number(value) {
            writer_ratings.push_unchecked(item.clone());
            continue;
        }
        if let Some(mapped) = key.map_key(TagType::VorbisComments) {
            items.push((mapped.to_string(), value.clone()));
        }
    }

    let mut comments = remainder.merge_tag(writer_ratings);
    for (key, value) in items {
        comments.push(key, value);
    }
    for picture in tag.pictures() {
        let information = PictureInformation::from_picture(picture).unwrap_or_default();
        // 大きさなどの情報を渡しているため、失敗しない
        let _ = comments.insert_picture(picture.clone(), Some(information));
    }

    comments.save_to_path(file_path, WriteOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::{jpeg, png};
    use crate::metadata::{
        EmbeddedPicture, extract_album_art, extract_all_file_info, read_file_tags,
        remove_file_album_art, set_file_album_art, update_file_metadata, update_file_rating,
    };
    use crate::models::Metadata;
    use std::path::PathBuf;

    /// FLAC・Ogg Vorbis・Opusのファイル（再生エンジンのテスト用のファイルの写し）
    fn test_files(dir: &Path) -> Vec<(PathBuf, FileType)> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/playback");
        [
            ("flac.flac", FileType::Flac),
            ("vorbis.ogg", FileType::Vorbis),
            ("opus.opus", FileType::Opus),
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

    /// ファイルのVorbisコメントを、そのまま読む
    fn read_file_comments(file: &Path, file_type: FileType) -> VorbisComments {
        let mut reader = BufReader::new(File::open(file).unwrap());
        let options = ParseOptions::new().read_properties(false);
        read_comments(&mut reader, file_type, options)
            .unwrap()
            .unwrap_or_default()
    }

    /// ファイルの画像の数（FLACは、コメントとは別のブロックに持つ）
    fn picture_count(file: &Path, file_type: FileType) -> usize {
        if file_type != FileType::Flac {
            return read_file_comments(file, file_type).pictures().len();
        }
        let mut reader = BufReader::new(File::open(file).unwrap());
        let options = ParseOptions::new().read_properties(false);
        FlacFile::read_from(&mut reader, options)
            .unwrap()
            .pictures()
            .len()
    }

    /// ほかのアプリが書いた形のコメントを、ファイルへ足す（独自の項目・複数の値・書き手の付いた評価）
    fn write_foreign_comments(file: &Path, file_type: FileType) -> String {
        let mut comments = read_file_comments(file, file_type);
        let vendor = comments.vendor().to_string();
        comments.push("TITLE".to_string(), "Foreign Title".to_string());
        comments.push("ARTIST".to_string(), "First".to_string());
        comments.push("ARTIST".to_string(), "Second".to_string());
        comments.push("MY_CUSTOM_ITEM".to_string(), "keep me".to_string());
        comments.push("my_other_item".to_string(), "one".to_string());
        comments.push("my_other_item".to_string(), "two".to_string());
        comments.push("RATING:someone@example.com".to_string(), "80".to_string());
        comments
            .save_to_path(file, WriteOptions::default())
            .unwrap();
        vendor
    }

    /// タグを書き換えても、アプリが扱わない項目とベンダーの文字列は残る
    #[test]
    fn test_saving_keeps_items_the_app_does_not_handle() {
        let dir = temp_dir();
        for (file, file_type) in test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let encoder_items = |comments: &VorbisComments| {
                comments
                    .get_all("ENCODER")
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            let original_encoder = encoder_items(&read_file_comments(&file, file_type));
            let vendor = write_foreign_comments(&file, file_type);
            let custom = |comments: &VorbisComments| {
                (
                    comments.get("MY_CUSTOM_ITEM").map(str::to_string),
                    comments
                        .get_all("my_other_item")
                        .map(str::to_string)
                        .collect::<Vec<_>>(),
                )
            };
            let expected_custom = (
                Some("keep me".to_string()),
                vec!["one".to_string(), "two".to_string()],
            );

            // 項目の編集（値のある項目だけを変える）
            let metadata = Metadata {
                title: Some("New Title".to_string()),
                bpm: Some(120),
                ..Default::default()
            };
            update_file_metadata(&file, &metadata, false).unwrap();
            let comments = read_file_comments(&file, file_type);
            assert_eq!(comments.get("TITLE"), Some("New Title"), "{name}");
            assert_eq!(comments.get("BPM"), Some("120"), "{name}");
            assert_eq!(custom(&comments), expected_custom, "{name}");
            assert_eq!(comments.vendor(), vendor, "{name}");
            // 複数の値を持つ項目も、そのまま残る
            assert_eq!(
                comments.get_all("ARTIST").collect::<Vec<_>>(),
                ["First", "Second"],
                "{name}"
            );
            // `ENCODER`は、ファイルにあったまま（なかったファイルに、ベンダーの文字列から足さない）
            assert_eq!(encoder_items(&comments), original_encoder, "{name}");
            // 書き手の付いた評価は、元の形のまま残る
            assert_eq!(
                comments.get("RATING:someone@example.com"),
                Some("80"),
                "{name}"
            );
            assert_eq!(comments.get("RATING"), None, "{name}");
            assert_eq!(extract_all_file_info(&file).unwrap().rating, 4, "{name}");

            // 画像の埋め込みと取り除き
            let art = EmbeddedPicture {
                data: jpeg(40, 40),
                mime_type: "image/jpeg".to_string(),
            };
            set_file_album_art(&file, &art).unwrap();
            let comments = read_file_comments(&file, file_type);
            assert_eq!(picture_count(&file, file_type), 1, "{name}");
            assert_eq!(
                extract_album_art(&file).unwrap().unwrap().data,
                jpeg(40, 40),
                "{name}"
            );
            assert_eq!(custom(&comments), expected_custom, "{name}");
            let art = EmbeddedPicture {
                data: png(50, 50),
                mime_type: "image/png".to_string(),
            };
            set_file_album_art(&file, &art).unwrap();
            assert_eq!(picture_count(&file, file_type), 1, "{name}");
            assert_eq!(
                extract_album_art(&file).unwrap().unwrap().data,
                png(50, 50),
                "{name}"
            );
            assert!(remove_file_album_art(&file).unwrap(), "{name}");
            let comments = read_file_comments(&file, file_type);
            assert_eq!(picture_count(&file, file_type), 0, "{name}");
            assert_eq!(custom(&comments), expected_custom, "{name}");

            // 評価を付けると、数値だけの`RATING`になる（書き手の付いた評価は置き換える）
            update_file_rating(&file, 5).unwrap();
            let comments = read_file_comments(&file, file_type);
            assert_eq!(comments.get("RATING"), Some("100"), "{name}");
            assert_eq!(comments.get("RATING:someone@example.com"), None, "{name}");
            assert_eq!(custom(&comments), expected_custom, "{name}");
            assert_eq!(comments.vendor(), vendor, "{name}");

            // 1曲の編集（空にした項目を取り除く）でも、アプリが扱わない項目は残る
            let cleared = Metadata {
                title: Some("Only Title".to_string()),
                ..Default::default()
            };
            update_file_metadata(&file, &cleared, true).unwrap();
            let comments = read_file_comments(&file, file_type);
            assert_eq!(read_file_tags(&file).unwrap(), cleared, "{name}");
            assert_eq!(comments.get("ARTIST"), None, "{name}");
            assert_eq!(comments.get("BPM"), None, "{name}");
            assert_eq!(comments.get("RATING"), Some("100"), "{name}");
            assert_eq!(custom(&comments), expected_custom, "{name}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// エンコーダーの項目（`ENCODER`）があるファイルでは、その項目を残す
    #[test]
    fn test_encoder_item_is_kept_when_the_file_has_one() {
        let dir = temp_dir();
        for (file, file_type) in test_files(&dir) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let mut comments = read_file_comments(&file, file_type);
            let vendor = comments.vendor().to_string();
            comments.insert("encoder".to_string(), "Some Encoder 1.0".to_string());
            comments
                .save_to_path(&file, WriteOptions::default())
                .unwrap();

            update_file_rating(&file, 3).unwrap();
            let comments = read_file_comments(&file, file_type);
            assert_eq!(comments.get("ENCODER"), Some("Some Encoder 1.0"), "{name}");
            assert_eq!(comments.vendor(), vendor, "{name}");
            assert_eq!(comments.get("RATING"), Some("60"), "{name}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
