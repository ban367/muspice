//! iTunes形式のライブラリXMLの読み込み（ほかのプレーヤーからの移行）
//!
//! MusicBee（設定の「ライブラリをiTunes形式のXMLで書き出す」）や、iTunes / ミュージックが書き出す
//! ライブラリのXML（プロパティリスト）を読み、ファイルのタグには入っていない項目
//! （再生回数・最終再生日・追加日・スキップ回数・お気に入り・プレイリスト）を取り出す。
//!
//! 曲は、XMLの場所（`Location`。`file://localhost/C:/Music/…`）とライブラリの曲のパスを、
//! M3Uの読み込みと同じ方法（`m3u::TrackMatcher`。パスの末尾の一致）で突き合わせる。
//! 取り込む値の決め方は`merge_stats`（ADR-033）。

use crate::error::{AppError, AppResult};
use crate::m3u::TrackMatcher;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// 読み込むXMLの大きさの上限（ライブラリのXMLではないファイルを読み続けないようにする）
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

/// XMLの中の1曲
#[derive(Debug, Clone, PartialEq)]
pub struct XmlTrack {
    /// XMLの中での曲の番号（プレイリストが、この番号で曲を指す）
    pub track_id: i64,
    /// 曲のファイルの場所（`file://`のURL）
    pub location: String,
    pub stats: ImportedStats,
}

/// XMLから取り込む、曲ごとの値（XMLにない項目は0・`None`・false）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImportedStats {
    pub play_count: i64,
    pub skip_count: i64,
    pub last_played: Option<DateTime<Utc>>,
    pub date_added: Option<DateTime<Utc>>,
    pub loved: bool,
}

/// XMLの中のプレイリスト
#[derive(Debug, Clone, PartialEq)]
pub struct XmlPlaylist {
    pub name: String,
    /// 入っている曲（XMLの中での曲の番号。並び順）
    pub track_ids: Vec<i64>,
}

/// ライブラリのXMLの内容
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XmlLibrary {
    pub tracks: Vec<XmlTrack>,
    pub playlists: Vec<XmlPlaylist>,
}

// ---- XML（プロパティリスト）の形。使う項目だけを読み、値の型の違いは読み飛ばす ----

#[derive(Deserialize)]
struct RawLibrary {
    #[serde(rename = "Tracks", default)]
    tracks: HashMap<String, RawTrack>,
    #[serde(rename = "Playlists", default)]
    playlists: Vec<RawPlaylist>,
}

#[derive(Deserialize)]
struct RawTrack {
    #[serde(rename = "Track ID")]
    track_id: Option<plist::Value>,
    #[serde(rename = "Location")]
    location: Option<plist::Value>,
    #[serde(rename = "Play Count")]
    play_count: Option<plist::Value>,
    #[serde(rename = "Skip Count")]
    skip_count: Option<plist::Value>,
    #[serde(rename = "Play Date UTC")]
    play_date: Option<plist::Value>,
    #[serde(rename = "Date Added")]
    date_added: Option<plist::Value>,
    #[serde(rename = "Loved")]
    loved: Option<plist::Value>,
    #[serde(rename = "Favorited")]
    favorited: Option<plist::Value>,
}

#[derive(Deserialize)]
struct RawPlaylist {
    #[serde(rename = "Name")]
    name: Option<plist::Value>,
    #[serde(rename = "Playlist Items", default)]
    items: Vec<RawPlaylistItem>,
    /// ライブラリ全体を表すプレイリスト
    #[serde(rename = "Master")]
    master: Option<plist::Value>,
    /// 「ミュージック」「ダウンロード済み」などの、決まったプレイリスト
    #[serde(rename = "Distinguished Kind")]
    distinguished_kind: Option<plist::Value>,
    /// プレイリストをまとめるフォルダ
    #[serde(rename = "Folder")]
    folder: Option<plist::Value>,
}

#[derive(Deserialize)]
struct RawPlaylistItem {
    #[serde(rename = "Track ID")]
    track_id: Option<plist::Value>,
}

fn integer(value: &Option<plist::Value>) -> Option<i64> {
    let value = value.as_ref()?;
    value
        .as_signed_integer()
        .or_else(|| value.as_real().map(|real| real as i64))
        .or_else(|| value.as_string().and_then(|text| text.trim().parse().ok()))
}

fn date(value: &Option<plist::Value>) -> Option<DateTime<Utc>> {
    let date = value.as_ref()?.as_date()?;
    Some(DateTime::<Utc>::from(std::time::SystemTime::from(date)))
}

fn is_true(value: &Option<plist::Value>) -> bool {
    value.as_ref().and_then(plist::Value::as_boolean) == Some(true)
}

/// ライブラリのXMLを読む
///
/// 場所（`Location`）のない曲（ファイルではない曲）と、ライブラリ全体・フォルダなどの
/// 特別なプレイリストは除く。
pub fn parse(path: &Path) -> AppResult<XmlLibrary> {
    let size = std::fs::metadata(path)
        .map_err(|e| AppError::Io(format!("ファイルを読めません: {}", e)))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(AppError::Validation(
            "ファイルが大きすぎます（ライブラリのXMLではない可能性があります）".to_string(),
        ));
    }
    let raw: RawLibrary = plist::from_file(path).map_err(|e| {
        AppError::Validation(format!(
            "ライブラリのXML（iTunes形式）として読めません: {}",
            e
        ))
    })?;

    let mut tracks: Vec<XmlTrack> = raw
        .tracks
        .values()
        .filter_map(|track| {
            let location = track.location.as_ref()?.as_string()?;
            Some(XmlTrack {
                track_id: integer(&track.track_id)?,
                location: location.to_string(),
                stats: ImportedStats {
                    play_count: integer(&track.play_count).unwrap_or(0).max(0),
                    skip_count: integer(&track.skip_count).unwrap_or(0).max(0),
                    last_played: date(&track.play_date),
                    date_added: date(&track.date_added),
                    loved: is_true(&track.loved) || is_true(&track.favorited),
                },
            })
        })
        .collect();
    // XMLの中の順（番号の順）にそろえる（結果の表示を、読み込むたびに同じ順にする）
    tracks.sort_by_key(|track| track.track_id);

    let playlists = raw
        .playlists
        .iter()
        .filter(|playlist| {
            !is_true(&playlist.master)
                && !is_true(&playlist.folder)
                && playlist.distinguished_kind.is_none()
        })
        .filter_map(|playlist| {
            Some(XmlPlaylist {
                name: playlist.name.as_ref()?.as_string()?.to_string(),
                track_ids: playlist
                    .items
                    .iter()
                    .filter_map(|item| integer(&item.track_id))
                    .collect(),
            })
        })
        .collect();

    Ok(XmlLibrary { tracks, playlists })
}

/// XMLの曲を、ライブラリの曲と突き合わせた結果
#[derive(Debug, PartialEq, Default)]
pub struct MatchedLibrary {
    /// ライブラリの曲（トラックID）ごとの、取り込む値
    pub stats: HashMap<String, ImportedStats>,
    /// XMLの中での曲の番号から、ライブラリの曲（トラックID）を引く
    pub track_ids: HashMap<i64, String>,
    /// ライブラリの曲と対応が付かなかった曲の場所
    pub unmatched: Vec<String>,
}

/// XMLの曲を、場所（`Location`）でライブラリの曲と突き合わせる
///
/// XMLの複数の曲が同じライブラリの曲に対応した場合（重複して登録されていた曲）は、
/// 値を合わせる（`ImportedStats::merge`）。
pub fn match_tracks(library: &XmlLibrary, matcher: &TrackMatcher) -> MatchedLibrary {
    let mut matched = MatchedLibrary::default();
    for track in &library.tracks {
        match matcher.find(&track.location, Path::new("/")) {
            Some(track_id) => {
                matched
                    .stats
                    .entry(track_id.to_string())
                    .and_modify(|stats| *stats = stats.merge(&track.stats))
                    .or_insert(track.stats);
                matched
                    .track_ids
                    .insert(track.track_id, track_id.to_string());
            }
            None => matched
                .unmatched
                .push(crate::m3u::display_location(&track.location)),
        }
    }
    matched
}

impl ImportedStats {
    /// 同じ曲についての2つの値を合わせる（回数は多い方・最終再生日は新しい方・追加日は古い方）
    fn merge(&self, other: &Self) -> Self {
        Self {
            play_count: self.play_count.max(other.play_count),
            skip_count: self.skip_count.max(other.skip_count),
            last_played: newer(self.last_played, other.last_played),
            date_added: older(self.date_added, other.date_added),
            loved: self.loved || other.loved,
        }
    }
}

fn newer(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

fn older(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// ライブラリの曲の、今の値（取り込む項目だけ）
#[derive(Debug, Clone, PartialEq)]
pub struct TrackStats {
    pub play_count: i64,
    pub skip_count: i64,
    pub last_played_at: Option<String>,
    pub created_at: String,
    pub is_favorite: bool,
}

/// DBに保存してある日時を読む（RFC 3339。古い行は`YYYY-MM-DD HH:MM:SS`（UTC）のことがある）
fn parse_stored_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .map(|date| date.and_utc())
        })
        .ok()
}

/// 今の値に、取り込む値を反映した値を返す（変わらなければ`None`）
///
/// 多くのプレーヤーの取り込みと同じく、どちらも1つの値として持ち、次のように決める。
///
/// - 再生回数・スキップ回数: 多い方（同じXMLを何度取り込んでも、増え続けない）
/// - 最後に再生した日時: 新しい方
/// - 追加した日時: 古い方（取り込んだ日ではなく、元のプレーヤーに追加した日にする）
/// - お気に入り: XMLでお気に入りなら、お気に入りにする（外すことはしない）
pub fn merge_stats(current: &TrackStats, imported: &ImportedStats) -> Option<TrackStats> {
    let mut merged = current.clone();
    merged.play_count = current.play_count.max(imported.play_count);
    merged.skip_count = current.skip_count.max(imported.skip_count);

    if let Some(last_played) = imported.last_played {
        let current_date = current
            .last_played_at
            .as_deref()
            .and_then(parse_stored_date);
        if current_date.is_none_or(|current| last_played > current) {
            merged.last_played_at = Some(last_played.to_rfc3339());
        }
    }
    if let Some(date_added) = imported.date_added
        // 今の値を読めない場合は、変えない
        && parse_stored_date(&current.created_at).is_some_and(|current| date_added < current)
    {
        merged.created_at = date_added.to_rfc3339();
    }
    merged.is_favorite = current.is_favorite || imported.loved;

    (merged != *current).then_some(merged)
}

/// プレイリストに入れる曲（ライブラリのトラックID）を決める
///
/// 対応が付かなかった曲は除き、同じ曲は最初の1回だけ入れる。
pub fn playlist_track_ids(playlist: &XmlPlaylist, track_ids: &HashMap<i64, String>) -> Vec<String> {
    let mut seen = HashSet::new();
    playlist
        .track_ids
        .iter()
        .filter_map(|id| track_ids.get(id))
        .filter(|track_id| seen.insert(track_id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::TimeZone;

    /// MusicBeeが書き出す形のXML（項目の並び・値の書き方は、実際の書き出しに合わせている）
    pub(crate) const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Major Version</key><integer>1</integer>
	<key>Minor Version</key><integer>1</integer>
	<key>Application Version</key><string>3.5.8698.34385</string>
	<key>Music Folder</key><string>file://localhost/D:/Music/</string>
	<key>Tracks</key>
	<dict>
		<key>1</key>
		<dict>
			<key>Track ID</key><integer>1</integer>
			<key>Name</key><string>青い地平線</string>
			<key>Artist</key><string>Aoi Sora</string>
			<key>Rating</key><integer>80</integer>
			<key>Total Time</key><integer>214000</integer>
			<key>Date Modified</key><date>2024-07-08T22:47:57Z</date>
			<key>Date Added</key><date>2019-03-10T22:30:36Z</date>
			<key>Play Count</key><integer>21</integer>
			<key>Play Date UTC</key><date>2024-09-25T18:31:11Z</date>
			<key>Track Type</key><string>File</string>
			<key>Location</key><string>file://localhost/D:/Music/Aoi%20Sora/Blue%20Horizon/01%20%E9%9D%92%E3%81%84%E5%9C%B0%E5%B9%B3%E7%B7%9A.mp3</string>
		</dict>
		<key>2</key>
		<dict>
			<key>Track ID</key><integer>2</integer>
			<key>Name</key><string>Paper Planes &#38; More</string>
			<key>Date Added</key><date>2020-01-02T03:04:05Z</date>
			<key>Skip Count</key><integer>4</integer>
			<key>Loved</key><true/>
			<key>Location</key><string>file://localhost/D:/Music/Aoi%20Sora/Blue%20Horizon/02%20Paper%20Planes.mp3</string>
		</dict>
		<key>3</key>
		<dict>
			<key>Track ID</key><integer>3</integer>
			<key>Name</key><string>ライブラリにない曲</string>
			<key>Play Count</key><integer>5</integer>
			<key>Location</key><string>file://localhost/D:/Music/Unknown/99%20Nothing.mp3</string>
		</dict>
		<key>4</key>
		<dict>
			<key>Track ID</key><integer>4</integer>
			<key>Name</key><string>ストリーム（場所がない）</string>
			<key>Play Count</key><integer>9</integer>
			<key>Track Type</key><string>URL</string>
		</dict>
	</dict>
	<key>Playlists</key>
	<array>
		<dict>
			<key>Name</key><string>Library</string>
			<key>Master</key><true/>
			<key>Playlist ID</key><integer>10</integer>
			<key>Playlist Items</key>
			<array>
				<dict><key>Track ID</key><integer>1</integer></dict>
			</array>
		</dict>
		<dict>
			<key>Playlist ID</key><integer>11</integer>
			<key>All Items</key><true/>
			<key>Name</key><string>通勤</string>
			<key>Playlist Items</key>
			<array>
				<dict><key>Track ID</key><integer>2</integer></dict>
				<dict><key>Track ID</key><integer>3</integer></dict>
				<dict><key>Track ID</key><integer>1</integer></dict>
				<dict><key>Track ID</key><integer>2</integer></dict>
			</array>
		</dict>
		<dict>
			<key>Playlist ID</key><integer>12</integer>
			<key>Name</key><string>空</string>
			<key>Playlist Items</key>
			<array>
			</array>
		</dict>
		<dict>
			<key>Playlist ID</key><integer>13</integer>
			<key>Name</key><string>フォルダ</string>
			<key>Folder</key><true/>
		</dict>
	</array>
</dict>
</plist>
"#;

    pub(crate) fn write_sample(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("muspice-xml-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("iTunes Music Library.xml");
        std::fs::write(&path, content).unwrap();
        path
    }

    fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
            .unwrap()
    }

    fn matcher() -> TrackMatcher {
        TrackMatcher::new(
            [
                (
                    "a1",
                    "/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3",
                ),
                (
                    "a2",
                    "/Users/me/Music/Aoi Sora/Blue Horizon/02 Paper Planes.mp3",
                ),
            ]
            .map(|(id, path)| (id.to_string(), path.to_string())),
        )
    }

    #[test]
    fn test_parse_reads_tracks_and_playlists() {
        let path = write_sample(SAMPLE_XML);
        let library = parse(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();

        // 場所のない曲は除く
        assert_eq!(
            library
                .tracks
                .iter()
                .map(|t| t.track_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            library.tracks[0].stats,
            ImportedStats {
                play_count: 21,
                skip_count: 0,
                last_played: Some(utc(2024, 9, 25, 18, 31, 11)),
                date_added: Some(utc(2019, 3, 10, 22, 30, 36)),
                loved: false,
            }
        );
        assert_eq!(
            library.tracks[1].stats,
            ImportedStats {
                play_count: 0,
                skip_count: 4,
                last_played: None,
                date_added: Some(utc(2020, 1, 2, 3, 4, 5)),
                loved: true,
            }
        );
        // ライブラリ全体・フォルダは除く
        assert_eq!(
            library.playlists,
            vec![
                XmlPlaylist {
                    name: "通勤".to_string(),
                    track_ids: vec![2, 3, 1, 2]
                },
                XmlPlaylist {
                    name: "空".to_string(),
                    track_ids: vec![]
                },
            ]
        );
    }

    #[test]
    fn test_parse_rejects_files_that_are_not_a_library() {
        let not_xml = write_sample("#EXTM3U\nmusic.mp3\n");
        assert!(matches!(parse(&not_xml), Err(AppError::Validation(_))));
        std::fs::remove_dir_all(not_xml.parent().unwrap()).unwrap();

        // プロパティリストだが、曲もプレイリストもない
        let empty = write_sample(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Other</key><integer>1</integer></dict></plist>"#,
        );
        assert_eq!(parse(&empty).unwrap(), XmlLibrary::default());
        std::fs::remove_dir_all(empty.parent().unwrap()).unwrap();

        assert!(matches!(
            parse(Path::new("/nonexistent/library.xml")),
            Err(AppError::Io(_))
        ));
    }

    #[test]
    fn test_match_tracks_by_location() {
        let path = write_sample(SAMPLE_XML);
        let library = parse(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();

        let matched = match_tracks(&library, &matcher());

        assert_eq!(matched.stats.len(), 2);
        assert_eq!(matched.stats["a1"].play_count, 21);
        assert!(matched.stats["a2"].loved);
        assert_eq!(matched.track_ids[&1], "a1");
        assert_eq!(matched.track_ids[&2], "a2");
        assert_eq!(matched.unmatched, vec!["D:/Music/Unknown/99 Nothing.mp3"]);

        // プレイリスト: 対応が付かなかった曲を除き、同じ曲は1回だけ
        assert_eq!(
            playlist_track_ids(&library.playlists[0], &matched.track_ids),
            vec!["a2".to_string(), "a1".to_string()]
        );
        assert!(playlist_track_ids(&library.playlists[1], &matched.track_ids).is_empty());
    }

    #[test]
    fn test_match_tracks_merges_duplicates_of_the_same_file() {
        let track = |track_id, play_count, day| XmlTrack {
            track_id,
            location: "file://localhost/D:/Music/Aoi%20Sora/Blue%20Horizon/02%20Paper%20Planes.mp3"
                .to_string(),
            stats: ImportedStats {
                play_count,
                skip_count: 0,
                last_played: Some(utc(2024, 1, day, 0, 0, 0)),
                date_added: Some(utc(2020, 1, day, 0, 0, 0)),
                loved: false,
            },
        };
        let library = XmlLibrary {
            tracks: vec![track(1, 3, 5), track(2, 8, 2)],
            playlists: vec![],
        };

        let matched = match_tracks(&library, &matcher());

        assert_eq!(
            matched.stats["a2"],
            ImportedStats {
                play_count: 8,
                skip_count: 0,
                last_played: Some(utc(2024, 1, 5, 0, 0, 0)),
                date_added: Some(utc(2020, 1, 2, 0, 0, 0)),
                loved: false,
            }
        );
    }

    /// 実際に書き出されたライブラリのXMLを読めるか確かめる（手動で実行する）
    ///
    /// `LIBRARY_XML_SAMPLE=<XMLのパス> cargo test real_library_xml -- --ignored --nocapture`
    #[test]
    #[ignore = "実際のライブラリのXMLが必要（LIBRARY_XML_SAMPLEで指定する）"]
    fn real_library_xml_can_be_parsed() {
        let path = std::env::var("LIBRARY_XML_SAMPLE").expect("LIBRARY_XML_SAMPLEを指定する");
        let started = std::time::Instant::now();
        let library = parse(Path::new(&path)).unwrap();
        let elapsed = started.elapsed();

        let with_plays = library
            .tracks
            .iter()
            .filter(|track| track.stats.play_count > 0)
            .count();
        let with_dates = library
            .tracks
            .iter()
            .filter(|track| track.stats.date_added.is_some() && track.stats.last_played.is_some())
            .count();
        println!(
            "tracks={} with_plays={} with_dates={} playlists={} ({} items) in {:?}",
            library.tracks.len(),
            with_plays,
            with_dates,
            library.playlists.len(),
            library
                .playlists
                .iter()
                .map(|playlist| playlist.track_ids.len())
                .sum::<usize>(),
            elapsed
        );
        let first = &library.tracks[0];
        println!(
            "first: {} {:?}",
            crate::m3u::display_location(&first.location),
            first.stats
        );

        // XMLのパスを、このコンピューターのパスへ読み替えたライブラリと突き合わせる
        let matcher = TrackMatcher::new(library.tracks.iter().map(|track| {
            let location = crate::m3u::display_location(&track.location);
            let relative = location
                .split_once(":/")
                .map_or(location.as_str(), |(_, path)| path);
            (
                format!("id-{}", track.track_id),
                format!("/Users/me/{relative}"),
            )
        }));
        let matched = match_tracks(&library, &matcher);
        println!(
            "matched={} unmatched={} {:?}",
            matched.track_ids.len(),
            matched.unmatched.len(),
            matched.unmatched.iter().take(3).collect::<Vec<_>>()
        );
        assert!(!library.tracks.is_empty());
        assert_eq!(matched.track_ids.len(), library.tracks.len());
    }

    fn current() -> TrackStats {
        TrackStats {
            play_count: 10,
            skip_count: 2,
            last_played_at: Some("2024-06-01T00:00:00+00:00".to_string()),
            created_at: "2024-01-01T00:00:00+00:00".to_string(),
            is_favorite: false,
        }
    }

    #[test]
    fn test_merge_stats_takes_the_larger_counts_and_the_wider_dates() {
        let merged = merge_stats(
            &current(),
            &ImportedStats {
                play_count: 21,
                skip_count: 1,
                last_played: Some(utc(2024, 9, 25, 18, 31, 11)),
                date_added: Some(utc(2019, 3, 10, 22, 30, 36)),
                loved: true,
            },
        )
        .unwrap();

        assert_eq!(merged.play_count, 21);
        // このアプリの値の方が多ければ、そのまま
        assert_eq!(merged.skip_count, 2);
        assert_eq!(
            merged.last_played_at.as_deref(),
            Some("2024-09-25T18:31:11+00:00")
        );
        assert_eq!(merged.created_at, "2019-03-10T22:30:36+00:00");
        assert!(merged.is_favorite);
    }

    #[test]
    fn test_merge_stats_returns_none_when_nothing_changes() {
        // このアプリの値の方が多い・新しい・古い場合は、変えない
        let imported = ImportedStats {
            play_count: 3,
            skip_count: 0,
            last_played: Some(utc(2024, 1, 1, 0, 0, 0)),
            date_added: Some(utc(2024, 5, 1, 0, 0, 0)),
            loved: false,
        };
        assert_eq!(merge_stats(&current(), &imported), None);
        assert_eq!(merge_stats(&current(), &ImportedStats::default()), None);

        // 取り込んだ後にもう一度取り込んでも、変わらない
        let imported = ImportedStats {
            play_count: 21,
            last_played: Some(utc(2024, 9, 25, 18, 31, 11)),
            date_added: Some(utc(2019, 3, 10, 22, 30, 36)),
            ..ImportedStats::default()
        };
        let merged = merge_stats(&current(), &imported).unwrap();
        assert_eq!(merge_stats(&merged, &imported), None);
    }

    #[test]
    fn test_merge_stats_reads_dates_in_the_old_format() {
        let mut stats = current();
        stats.last_played_at = None;
        stats.created_at = "2024-01-01 00:00:00".to_string();
        let imported = ImportedStats {
            last_played: Some(utc(2023, 1, 1, 0, 0, 0)),
            date_added: Some(utc(2019, 3, 10, 22, 30, 36)),
            ..ImportedStats::default()
        };

        let merged = merge_stats(&stats, &imported).unwrap();
        // 再生した記録がなければ、XMLの日時を入れる
        assert_eq!(
            merged.last_played_at.as_deref(),
            Some("2023-01-01T00:00:00+00:00")
        );
        assert_eq!(merged.created_at, "2019-03-10T22:30:36+00:00");

        // 今の値を読めない場合は、追加した日時を変えない
        stats.created_at = "unknown".to_string();
        assert_eq!(
            merge_stats(&stats, &imported).unwrap().created_at,
            "unknown"
        );
    }
}
