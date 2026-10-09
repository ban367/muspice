//! 移動・改名されたファイルの対応付け
//!
//! ライブラリフォルダの中でファイルを移動・改名すると、再スキャンでは「見つからなくなった曲」と
//! 「新しく見つかったファイル」になる。新しいファイルを別の曲として登録すると、お気に入り・
//! 再生回数・再生履歴・プレイリストへの登録が引き継がれないため、同じ曲と判定できた場合は、
//! 見つからない曲のパスを新しいファイルに付け替える（トラックのIDは変えない）。
//!
//! 同じ曲とみなす条件（誤った対応付けを避けるため、どちらもファイルサイズの一致を前提にする）:
//!
//! - ファイルの更新日時が同じ（同じボリュームの中での移動・改名では変わらない）
//! - または、タグの内容（タイトル・アーティスト・アルバム・トラック番号・ディスク番号）と長さが同じ
//!   （コピーし直した場合など、更新日時が変わっても対応付けられる）
//!
//! 候補が1つに決まらない場合（同じ内容のファイルが複数あるなど）は対応付けず、新しい曲として登録する。

use crate::models::Track;
use std::collections::HashMap;

/// ファイルが見つからないトラック（対応付けの候補）
#[derive(Debug, Clone, PartialEq)]
pub struct MissingTrack {
    pub id: String,
    pub file_name: String,
    pub file_size: i64,
    /// ファイルの更新日時（UNIX時間の秒）。記録前に登録したトラックはNone
    pub file_modified_at: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration: Option<i32>,
}

/// 見つからないトラックを、ファイルサイズで引けるようにしたもの
#[derive(Debug, Default)]
pub struct MissingTrackIndex {
    by_size: HashMap<i64, Vec<MissingTrack>>,
}

impl MissingTrackIndex {
    pub fn new(tracks: Vec<MissingTrack>) -> Self {
        let mut by_size: HashMap<i64, Vec<MissingTrack>> = HashMap::new();
        for track in tracks {
            by_size.entry(track.file_size).or_default().push(track);
        }
        Self { by_size }
    }

    /// 新しく見つかったファイルと同じ曲の「見つからないトラック」を取り出す
    ///
    /// 取り出したトラックは候補から外す（1つのトラックを、複数のファイルに対応付けない）。
    /// 同じ曲とみなせる候補が複数ある場合は、ファイル名が同じものに絞り、それでも1つに
    /// 決まらなければ対応付けない。
    /// @param file - 新しく見つかったファイルから読んだ内容
    /// @param modified_at - そのファイルの更新日時（UNIX時間の秒）
    pub fn take_match(&mut self, file: &Track, modified_at: Option<i64>) -> Option<MissingTrack> {
        let candidates = self.by_size.get_mut(&file.file_size)?;
        let matches: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| is_same_file(candidate, file, modified_at))
            .map(|(index, _)| index)
            .collect();

        let index = match matches.as_slice() {
            [index] => *index,
            [] => return None,
            _ => {
                let same_name: Vec<usize> = matches
                    .into_iter()
                    .filter(|&index| candidates[index].file_name == file.file_name)
                    .collect();
                match same_name.as_slice() {
                    [index] => *index,
                    _ => return None,
                }
            }
        };
        Some(candidates.swap_remove(index))
    }
}

/// サイズが同じ候補とファイルが、同じ曲のファイルか
fn is_same_file(candidate: &MissingTrack, file: &Track, modified_at: Option<i64>) -> bool {
    let same_modified_at =
        candidate.file_modified_at.is_some() && candidate.file_modified_at == modified_at;
    // タイトルのタグがない曲のタイトルはファイル名から付けるため、改名すると一致しなくなる
    // （長さで大きさが決まる形式では、タグのない別の曲と区別できないため、それでよい）
    let same_tags = candidate.duration == file.duration
        && candidate.title == file.title
        && candidate.artist == file.artist
        && candidate.album == file.album
        && candidate.track_number == file.track_number
        && candidate.disc_number == file.disc_number;
    same_modified_at || same_tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ReplayGain;

    fn missing(id: &str, file_name: &str, size: i64, modified_at: Option<i64>) -> MissingTrack {
        MissingTrack {
            id: id.to_string(),
            file_name: file_name.to_string(),
            file_size: size,
            file_modified_at: modified_at,
            title: Some("曲".to_string()),
            artist: Some("アーティスト".to_string()),
            album: Some("アルバム".to_string()),
            track_number: Some(1),
            disc_number: Some(1),
            duration: Some(200),
        }
    }

    /// 新しく見つかったファイル（`missing`と同じタグ）
    fn file(file_name: &str, size: i64) -> Track {
        Track {
            id: "new".to_string(),
            file_path: format!("/music/new/{file_name}"),
            file_name: file_name.to_string(),
            title: Some("曲".to_string()),
            artist: Some("アーティスト".to_string()),
            album: Some("アルバム".to_string()),
            album_artist: None,
            genre: None,
            year: None,
            track_number: Some(1),
            disc_number: Some(1),
            duration: Some(200),
            file_size: size,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            replay_gain: ReplayGain::default(),
            is_missing: false,
        }
    }

    #[test]
    fn test_matches_by_size_and_modified_at_even_if_tags_differ() {
        let mut index = MissingTrackIndex::new(vec![missing("t1", "old.mp3", 1000, Some(50))]);
        // DBだけで編集していた曲など、タグの内容が違っても、サイズと更新日時が同じなら同じファイル
        let mut renamed = file("renamed.mp3", 1000);
        renamed.title = Some("ファイルのタグのタイトル".to_string());

        let matched = index.take_match(&renamed, Some(50));

        assert_eq!(matched.map(|t| t.id), Some("t1".to_string()));
        // 1つのトラックを、複数のファイルに対応付けない
        assert_eq!(index.take_match(&renamed, Some(50)), None);
    }

    #[test]
    fn test_matches_by_size_and_tags_when_modified_at_changed() {
        let mut index = MissingTrackIndex::new(vec![
            missing("t1", "a.mp3", 1000, Some(50)),
            missing("legacy", "b.mp3", 2000, None),
        ]);

        // コピーし直して更新日時が変わった場合
        assert_eq!(
            index
                .take_match(&file("a.mp3", 1000), Some(99))
                .map(|t| t.id),
            Some("t1".to_string())
        );
        // 更新日時を記録していないトラックも、タグが同じなら対応付ける
        assert_eq!(
            index.take_match(&file("b.mp3", 2000), None).map(|t| t.id),
            Some("legacy".to_string())
        );
    }

    #[test]
    fn test_does_not_match_different_size_or_different_file() {
        let mut index = MissingTrackIndex::new(vec![missing("t1", "a.mp3", 1000, Some(50))]);

        // サイズが違う（タグを書き換えたファイルなど）
        assert_eq!(index.take_match(&file("a.mp3", 1001), Some(50)), None);

        // サイズは同じだが、更新日時もタグも違う（長さで大きさが決まる形式の別の曲など）
        let mut other = file("a.mp3", 1000);
        other.title = Some("別の曲".to_string());
        assert_eq!(index.take_match(&other, Some(51)), None);
        let mut other_length = file("a.mp3", 1000);
        other_length.duration = Some(201);
        assert_eq!(index.take_match(&other_length, Some(51)), None);

        // 更新日時を記録していない同士は、更新日時の一致とはみなさない
        let mut legacy = MissingTrackIndex::new(vec![missing("t1", "a.mp3", 1000, None)]);
        assert_eq!(legacy.take_match(&other, None), None);
    }

    #[test]
    fn test_ambiguous_candidates_are_narrowed_by_file_name() {
        // 同じ内容のファイルが2つ見つからなくなった場合
        let duplicates = || {
            MissingTrackIndex::new(vec![
                missing("copy1", "song.mp3", 1000, Some(50)),
                missing("copy2", "song (1).mp3", 1000, Some(50)),
            ])
        };

        // ファイル名が同じ候補が1つなら、それに対応付ける
        assert_eq!(
            duplicates()
                .take_match(&file("song (1).mp3", 1000), Some(50))
                .map(|t| t.id),
            Some("copy2".to_string())
        );
        // 1つに決まらなければ、対応付けない
        assert_eq!(
            duplicates().take_match(&file("renamed.mp3", 1000), Some(50)),
            None
        );
    }
}
