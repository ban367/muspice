//! 曲の歌詞（表示用）
//!
//! 歌詞は、曲と同じフォルダにある同じ名前の`.lrc`ファイルを優先し、なければ埋め込みの歌詞
//! （タグ）を使う。時刻付きの歌詞（LRC形式）の解釈は、フロントエンドが行う（ここでは、文字列を
//! そのまま返す）。一覧の取得には含めず、表示する時にファイルから読む。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};

/// 読み込む`.lrc`ファイルの大きさの上限（歌詞としては十分に大きい。これを超えるファイルは読まない）
const MAX_LRC_FILE_BYTES: u64 = 1024 * 1024;

/// 歌詞の出どころ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LyricsSource {
    /// 曲と同じフォルダの`.lrc`ファイル
    LrcFile,
    /// 曲のファイルのタグ
    Embedded,
}

/// 曲の歌詞
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackLyrics {
    /// 歌詞（時刻付きの場合は、LRC形式のまま）
    pub text: String,
    pub source: LyricsSource,
}

/// 文字列に、LRC形式の時刻（行の先頭の`[mm:ss`）があるか
pub fn has_timestamps(text: &str) -> bool {
    text.lines().any(|line| {
        let Some(rest) = line.trim_start().strip_prefix('[') else {
            return false;
        };
        let Some((minutes, seconds)) = rest.split_once(':') else {
            return false;
        };
        !minutes.is_empty()
            && minutes.chars().all(|c| c.is_ascii_digit())
            && seconds.chars().next().is_some_and(|c| c.is_ascii_digit())
    })
}

/// 曲と同じフォルダにある、同じ名前の`.lrc`ファイルを探す（拡張子は、大文字と小文字を区別しない）
fn find_lrc_file(track_path: &Path) -> Option<PathBuf> {
    let stem = track_path.file_stem()?;
    let entries = fs::read_dir(track_path.parent()?).ok()?;
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_stem() == Some(stem)
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lrc"))
                && path.is_file()
        })
}

/// `.lrc`ファイルの歌詞を読む（ない・大きすぎる・空の場合は値なし）
///
/// 文字コードは自動で判定する（BOM付き・UTF-8・Shift_JIS。`m3u::decode`と同じ）。
fn read_lrc_file(track_path: &Path) -> AppResult<Option<String>> {
    let Some(path) = find_lrc_file(track_path) else {
        return Ok(None);
    };
    let size = fs::metadata(&path)
        .map_err(|e| AppError::Io(format!("歌詞のファイルを読み込めませんでした: {}", e)))?
        .len();
    if size > MAX_LRC_FILE_BYTES {
        log::warn!(
            "歌詞のファイルが大きすぎるため読みません: {}",
            path.display()
        );
        return Ok(None);
    }

    let bytes = fs::read(&path)
        .map_err(|e| AppError::Io(format!("歌詞のファイルを読み込めませんでした: {}", e)))?;
    let text = crate::m3u::decode(&bytes);
    Ok((!text.trim().is_empty()).then_some(text))
}

/// 曲の歌詞を読む（`.lrc`ファイルを優先し、なければ埋め込みの歌詞。どちらもなければ値なし）
///
/// 曲のファイルが見つからない・タグを読めない場合は、埋め込みの歌詞がないものとして扱う。
pub fn read_track_lyrics(track_path: &Path) -> AppResult<Option<TrackLyrics>> {
    if let Some(text) = read_lrc_file(track_path)? {
        return Ok(Some(TrackLyrics {
            text,
            source: LyricsSource::LrcFile,
        }));
    }

    let embedded = crate::metadata::read_file_lyrics(track_path).unwrap_or_else(|e| {
        log::warn!("埋め込みの歌詞を読めませんでした: {}", e);
        None
    });
    Ok(embedded.map(|text| TrackLyrics {
        text,
        source: LyricsSource::Embedded,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_images::write_wav;
    use crate::models::Metadata;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muspice-lyrics-test-{}-{}",
            name,
            std::process::id()
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// タグに歌詞を入れた音楽ファイルを作る
    fn write_track(dir: &Path, name: &str, lyrics: Option<&str>) -> PathBuf {
        let path = dir.join(name);
        write_wav(&path);
        if let Some(lyrics) = lyrics {
            let metadata = Metadata {
                title: Some("曲".to_string()),
                lyrics: Some(lyrics.to_string()),
                ..Metadata::default()
            };
            crate::metadata::update_file_metadata(&path, &metadata, true).unwrap();
        }
        path
    }

    #[test]
    fn test_has_timestamps() {
        assert!(has_timestamps("[00:12.34]歌詞"));
        assert!(has_timestamps("[ar:歌手]\n  [01:02]歌詞"));
        assert!(has_timestamps("[123:45.6]長い曲"));
        assert!(!has_timestamps("歌詞の1行目\n歌詞の2行目"));
        // 曲の情報だけ・括弧で始まる歌詞は、時刻ではない
        assert!(!has_timestamps("[ar:歌手]\n[ti:曲]"));
        assert!(!has_timestamps("[Chorus]\n[x:1]"));
        assert!(!has_timestamps(""));
    }

    #[test]
    fn test_reads_the_lrc_file_next_to_the_track_first() {
        let dir = temp_dir("lrc-first");
        let track = write_track(&dir, "song.wav", Some("埋め込みの歌詞"));
        fs::write(dir.join("song.lrc"), "[00:01.00]ファイルの歌詞\n").unwrap();
        // 名前の違うファイルは使わない
        fs::write(dir.join("other.lrc"), "[00:01.00]別の曲\n").unwrap();

        let lyrics = read_track_lyrics(&track).unwrap().unwrap();

        assert_eq!(lyrics.source, LyricsSource::LrcFile);
        assert_eq!(lyrics.text, "[00:01.00]ファイルの歌詞\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_lrc_extension_is_case_insensitive_and_encoding_is_detected() {
        let dir = temp_dir("lrc-encoding");
        let track = write_track(&dir, "曲.wav", None);
        let text = "[00:01.00]日本語の歌詞";
        fs::write(dir.join("曲.LRC"), encoding_rs::SHIFT_JIS.encode(text).0).unwrap();

        let lyrics = read_track_lyrics(&track).unwrap().unwrap();

        assert_eq!(lyrics.source, LyricsSource::LrcFile);
        assert_eq!(lyrics.text, text);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_falls_back_to_embedded_lyrics() {
        let dir = temp_dir("embedded");
        let track = write_track(&dir, "song.wav", Some("1行目\n2行目"));
        // 空の.lrcファイルは、ないものとして扱う
        fs::write(dir.join("song.lrc"), "  \n").unwrap();

        let lyrics = read_track_lyrics(&track).unwrap().unwrap();

        assert_eq!(lyrics.source, LyricsSource::Embedded);
        assert_eq!(lyrics.text, "1行目\n2行目");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_no_lyrics() {
        let dir = temp_dir("none");
        let track = write_track(&dir, "song.wav", None);

        assert_eq!(read_track_lyrics(&track).unwrap(), None);
        // 曲のファイルが見つからなくても、エラーにしない
        assert_eq!(read_track_lyrics(&dir.join("missing.wav")).unwrap(), None);
        assert_eq!(
            read_track_lyrics(Path::new("/no/such/folder/song.wav")).unwrap(),
            None
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_too_large_lrc_file_is_ignored() {
        let dir = temp_dir("large");
        let track = write_track(&dir, "song.wav", Some("埋め込み"));
        let large = "[00:01.00]a\n".repeat((MAX_LRC_FILE_BYTES as usize / 12) + 10);
        fs::write(dir.join("song.lrc"), large).unwrap();

        let lyrics = read_track_lyrics(&track).unwrap().unwrap();

        assert_eq!(lyrics.source, LyricsSource::Embedded);
        fs::remove_dir_all(&dir).ok();
    }
}
