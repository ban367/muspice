//! ライブラリの書き出し（iTunes形式のライブラリXMLとCSV。ADR-121）
//!
//! - XML: iTunes / ミュージックが書き出すのと同じプロパティリスト（`Tracks`の辞書と`Playlists`の配列）。
//!   DJソフト（rekordbox・Serato・Traktorなど）やほかの管理ソフトが、曲・評価・再生回数・プレイリストを
//!   読める。このアプリのライブラリXMLの取り込み（`library_xml`）でも読み戻せる
//! - CSV: 曲の一覧（表計算ソフト用。UTF-8のBOM付き・CRLF）
//!
//! どちらも、DBの記録から作る（ファイルのタグは読まない）。

use crate::models::{PlaylistFolder, Track};
use crate::settings::Language;
use chrono::{DateTime, Utc};
use plist::{Dictionary, Value};
use std::time::SystemTime;

/// 書き出すプレイリスト（自動プレイリストは、書き出す時点で条件に合う曲）
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlaylist {
    pub id: String,
    pub name: String,
    /// 入っているフォルダ（フォルダの外ならNone）
    pub folder_id: Option<String>,
    /// 曲（並び順）
    pub track_ids: Vec<String>,
}

/// XMLの曲・プレイリストの番号の始まり（iTunesと同じく、小さな番号を避ける）
const FIRST_ID: i64 = 1000;

/// 曲・プレイリストのIDから、XMLの`Persistent ID`（16桁の16進数の大文字）を作る
fn persistent_id(id: &str) -> String {
    let hex: String = id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    format!("{:0>16}", &hex[..hex.len().min(16)])
}

/// パスを、`file://`のURLにする（iTunesと同じく`file://localhost`で始め、英数字と`-._~/`のほかは`%XX`にする）
pub fn file_url(path: &str) -> String {
    let mut url = String::from("file://localhost");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{:02X}", byte));
        }
    }
    url
}

/// DBに保存した日時（RFC 3339）を、プロパティリストの日時にする（読めなければNone）
fn plist_date(value: &str) -> Option<Value> {
    let parsed = DateTime::parse_from_rfc3339(value).ok()?;
    Some(Value::Date(
        SystemTime::from(parsed.with_timezone(&Utc)).into(),
    ))
}

fn integer(value: impl Into<i64>) -> Value {
    Value::Integer(value.into().into())
}

/// 値のある文字列だけを入れる
fn insert_text(dict: &mut Dictionary, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        dict.insert(key.to_string(), Value::String(value.to_string()));
    }
}

/// 値のある数だけを入れる
fn insert_number(dict: &mut Dictionary, key: &str, value: Option<i32>) {
    if let Some(value) = value {
        dict.insert(key.to_string(), integer(value));
    }
}

/// iTunesの`Kind`（形式の説明。`MPEG audio file`など）
fn kind(format: &str) -> String {
    match format.to_ascii_uppercase().as_str() {
        "MP3" | "MP2" => "MPEG audio file".to_string(),
        "M4A" | "AAC" => "AAC audio file".to_string(),
        "WAV" => "WAV audio file".to_string(),
        "AIFF" => "AIFF audio file".to_string(),
        other => format!("{} audio file", other),
    }
}

/// 1曲の辞書（iTunesの`Tracks`の値）
fn track_dict(track: &Track, xml_id: i64) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.insert("Track ID".to_string(), integer(xml_id));
    dict.insert(
        "Persistent ID".to_string(),
        Value::String(persistent_id(&track.id)),
    );
    dict.insert("Track Type".to_string(), Value::String("File".to_string()));
    insert_text(
        &mut dict,
        "Name",
        Some(track.title.as_deref().unwrap_or(&track.file_name)),
    );
    insert_text(&mut dict, "Artist", track.artist.as_deref());
    insert_text(&mut dict, "Album Artist", track.album_artist.as_deref());
    insert_text(&mut dict, "Composer", track.details.composer.as_deref());
    insert_text(&mut dict, "Album", track.album.as_deref());
    insert_text(&mut dict, "Grouping", track.details.grouping.as_deref());
    insert_text(&mut dict, "Genre", track.genre.as_deref());
    dict.insert("Kind".to_string(), Value::String(kind(&track.format)));
    dict.insert("Size".to_string(), integer(track.file_size));
    if let Some(seconds) = track.duration {
        dict.insert("Total Time".to_string(), integer(i64::from(seconds) * 1000));
    }
    insert_number(&mut dict, "Disc Number", track.disc_number);
    insert_number(&mut dict, "Track Number", track.track_number);
    insert_number(&mut dict, "Year", track.year);
    insert_number(&mut dict, "BPM", track.details.bpm);
    if let Some(date) = plist_date(&track.updated_at) {
        dict.insert("Date Modified".to_string(), date);
    }
    if let Some(date) = plist_date(&track.created_at) {
        dict.insert("Date Added".to_string(), date);
    }
    insert_number(&mut dict, "Bit Rate", track.bitrate);
    insert_number(&mut dict, "Sample Rate", track.sample_rate);
    insert_text(&mut dict, "Comments", track.details.comment.as_deref());
    if track.play_count > 0 {
        dict.insert("Play Count".to_string(), integer(track.play_count));
    }
    if let Some(date) = track.last_played_at.as_deref().and_then(plist_date) {
        dict.insert("Play Date UTC".to_string(), date);
    }
    if track.skip_count > 0 {
        dict.insert("Skip Count".to_string(), integer(track.skip_count));
    }
    // 評価は0〜100（半分の星の単位の0〜10を10倍。星1つは20）
    if track.rating > 0 {
        dict.insert("Rating".to_string(), integer(track.rating * 10));
    }
    if track.is_favorite {
        dict.insert("Loved".to_string(), Value::Boolean(true));
    }
    if track.compilation {
        dict.insert("Compilation".to_string(), Value::Boolean(true));
    }
    insert_text(&mut dict, "Sort Name", track.sort_tags.title.as_deref());
    insert_text(&mut dict, "Sort Artist", track.sort_tags.artist.as_deref());
    insert_text(&mut dict, "Sort Album", track.sort_tags.album.as_deref());
    insert_text(
        &mut dict,
        "Sort Album Artist",
        track.sort_tags.album_artist.as_deref(),
    );
    dict.insert(
        "Location".to_string(),
        Value::String(file_url(&track.file_path)),
    );
    dict
}

/// プレイリストの曲（`Playlist Items`。XMLの曲の番号の配列。ライブラリにない曲は除く）
fn playlist_items<'a>(
    track_ids: impl IntoIterator<Item = &'a String>,
    xml_ids: &std::collections::HashMap<&str, i64>,
) -> Value {
    Value::Array(
        track_ids
            .into_iter()
            .filter_map(|id| xml_ids.get(id.as_str()))
            .map(|&xml_id| {
                let mut item = Dictionary::new();
                item.insert("Track ID".to_string(), integer(xml_id));
                Value::Dictionary(item)
            })
            .collect(),
    )
}

/// iTunes形式のライブラリXML（プロパティリスト）を作る
///
/// 曲（`Tracks`）と、プレイリスト（`Playlists`。最初にライブラリ全体の`Master`、次にフォルダ、
/// プレイリストの順）を書く。プレイリストのフォルダは`Folder`、入っているプレイリストは
/// `Parent Persistent ID`で表す。
/// - `music_folder`: 音楽フォルダ（`Music Folder`に書く。未設定ならNone）
pub fn build_itunes_xml(
    tracks: &[Track],
    playlists: &[ExportPlaylist],
    folders: &[PlaylistFolder],
    music_folder: Option<&str>,
    app_version: &str,
    now: DateTime<Utc>,
) -> Value {
    let xml_ids: std::collections::HashMap<&str, i64> = tracks
        .iter()
        .zip(FIRST_ID..)
        .map(|(track, xml_id)| (track.id.as_str(), xml_id))
        .collect();

    let mut track_dicts = Dictionary::new();
    for (track, xml_id) in tracks.iter().zip(FIRST_ID..) {
        track_dicts.insert(
            xml_id.to_string(),
            Value::Dictionary(track_dict(track, xml_id)),
        );
    }

    let mut playlist_values = Vec::with_capacity(1 + folders.len() + playlists.len());
    let mut next_playlist_id = FIRST_ID + i64::try_from(tracks.len()).unwrap_or(0);
    let mut new_playlist = |name: &str, persistent: String| {
        let mut dict = Dictionary::new();
        dict.insert("Name".to_string(), Value::String(name.to_string()));
        dict.insert("Playlist ID".to_string(), integer(next_playlist_id));
        dict.insert(
            "Playlist Persistent ID".to_string(),
            Value::String(persistent),
        );
        dict.insert("All Items".to_string(), Value::Boolean(true));
        next_playlist_id += 1;
        dict
    };

    // ライブラリ全体（iTunesの`Master`。読む側の多くが、これを全曲として扱う）
    let mut master = new_playlist("Library", persistent_id("ffffffffffffffff"));
    master.insert("Master".to_string(), Value::Boolean(true));
    master.insert("Visible".to_string(), Value::Boolean(false));
    master.insert(
        "Playlist Items".to_string(),
        playlist_items(tracks.iter().map(|track| &track.id), &xml_ids),
    );
    playlist_values.push(Value::Dictionary(master));

    for folder in folders {
        let mut dict = new_playlist(&folder.name, persistent_id(&folder.id));
        dict.insert("Folder".to_string(), Value::Boolean(true));
        dict.insert("Playlist Items".to_string(), Value::Array(Vec::new()));
        playlist_values.push(Value::Dictionary(dict));
    }
    for playlist in playlists {
        let mut dict = new_playlist(&playlist.name, persistent_id(&playlist.id));
        if let Some(folder_id) = playlist
            .folder_id
            .as_ref()
            .filter(|id| folders.iter().any(|folder| &folder.id == *id))
        {
            dict.insert(
                "Parent Persistent ID".to_string(),
                Value::String(persistent_id(folder_id)),
            );
        }
        dict.insert(
            "Playlist Items".to_string(),
            playlist_items(&playlist.track_ids, &xml_ids),
        );
        playlist_values.push(Value::Dictionary(dict));
    }

    let mut root = Dictionary::new();
    root.insert("Major Version".to_string(), integer(1));
    root.insert("Minor Version".to_string(), integer(1));
    root.insert(
        "Application Version".to_string(),
        Value::String(format!("Muspice {}", app_version)),
    );
    root.insert(
        "Date".to_string(),
        Value::Date(SystemTime::from(now).into()),
    );
    root.insert("Features".to_string(), integer(5));
    root.insert("Show Content Ratings".to_string(), Value::Boolean(true));
    if let Some(folder) = music_folder {
        let mut url = file_url(folder);
        if !url.ends_with('/') {
            url.push('/');
        }
        root.insert("Music Folder".to_string(), Value::String(url));
    }
    root.insert("Tracks".to_string(), Value::Dictionary(track_dicts));
    root.insert("Playlists".to_string(), Value::Array(playlist_values));
    Value::Dictionary(root)
}

/// プロパティリストを、XMLのバイト列にする
pub fn to_xml_bytes(value: &Value) -> Result<Vec<u8>, plist::Error> {
    let mut bytes = Vec::new();
    value.to_writer_xml(&mut bytes)?;
    Ok(bytes)
}

/// CSVの1つの値（`,`・`"`・改行を含む値は`"`で囲む。表計算ソフトで式として読まれる値の頭には`'`を付ける）
fn csv_field(value: &str) -> String {
    // `=`・`+`・`-`・`@`・タブ・CRで始まる値は、表計算ソフトが式として実行することがある
    // （タグはファイルから読んだ値で、利用者が書いたものとは限らない）
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{}", value)
    } else {
        value.to_string()
    };
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// 長さ（秒）を`m:ss`（1時間以上は`h:mm:ss`）にする
fn csv_duration(seconds: Option<i32>) -> String {
    let Some(seconds) = seconds.filter(|seconds| *seconds >= 0) else {
        return String::new();
    };
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

/// CSVの見出し（アプリの言語に合わせる）
fn csv_headers(language: Language) -> [&'static str; 22] {
    match language {
        Language::Ja => [
            "タイトル",
            "アーティスト",
            "アルバムアーティスト",
            "アルバム",
            "作曲者",
            "グループ",
            "ジャンル",
            "年",
            "トラック番号",
            "ディスク番号",
            "時間",
            "評価",
            "お気に入り",
            "再生回数",
            "スキップ回数",
            "最終再生日",
            "追加日",
            "形式",
            "ビットレート",
            "サンプルレート",
            "サイズ",
            "場所",
        ],
        Language::En => [
            "Title",
            "Artist",
            "Album Artist",
            "Album",
            "Composer",
            "Grouping",
            "Genre",
            "Year",
            "Track Number",
            "Disc Number",
            "Time",
            "Rating",
            "Favorite",
            "Plays",
            "Skips",
            "Last Played",
            "Date Added",
            "Format",
            "Bit Rate",
            "Sample Rate",
            "Size",
            "Location",
        ],
    }
}

/// 曲の一覧のCSV（UTF-8のBOM付き・CRLF。表計算ソフトで文字化けしないように）
///
/// 評価は星の数（半分の星は`.5`）、お気に入りは`1`、時間は`m:ss`、日時はDBに保存した値（RFC 3339）。
pub fn build_csv(tracks: &[Track], language: Language) -> String {
    let mut csv = String::from("\u{FEFF}");
    csv.push_str(&csv_headers(language).map(csv_field).join(","));
    csv.push_str("\r\n");
    let number = |value: Option<i32>| value.map(|value| value.to_string()).unwrap_or_default();
    for track in tracks {
        let rating = if track.rating > 0 {
            let stars = f64::from(track.rating) / 2.0;
            if track.rating % 2 == 0 {
                format!("{}", track.rating / 2)
            } else {
                format!("{:.1}", stars)
            }
        } else {
            String::new()
        };
        let fields = [
            track
                .title
                .clone()
                .unwrap_or_else(|| track.file_name.clone()),
            track.artist.clone().unwrap_or_default(),
            track.album_artist.clone().unwrap_or_default(),
            track.album.clone().unwrap_or_default(),
            track.details.composer.clone().unwrap_or_default(),
            track.details.grouping.clone().unwrap_or_default(),
            track.genre.clone().unwrap_or_default(),
            number(track.year),
            number(track.track_number),
            number(track.disc_number),
            csv_duration(track.duration),
            rating,
            if track.is_favorite { "1" } else { "" }.to_string(),
            track.play_count.to_string(),
            track.skip_count.to_string(),
            track.last_played_at.clone().unwrap_or_default(),
            track.created_at.clone(),
            track.format.clone(),
            number(track.bitrate),
            number(track.sample_rate),
            track.file_size.to_string(),
            track.file_path.clone(),
        ];
        csv.push_str(
            &fields
                .iter()
                .map(|value| csv_field(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::models::{ReplayGain, SortTags, TrackDetails};

    fn track(id: &str, path: &str) -> Track {
        Track {
            cue: None,
            id: id.to_string(),
            file_path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap().to_string(),
            title: Some("Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: None,
            genre: Some("Rock; Pop".to_string()),
            year: Some(2020),
            track_number: Some(1),
            disc_number: None,
            duration: Some(185),
            file_size: 1234,
            format: "MP3".to_string(),
            bitrate: Some(320),
            sample_rate: Some(44100),
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: "2026-01-02T03:04:05+00:00".to_string(),
            updated_at: "2026-01-03T00:00:00+00:00".to_string(),
            replay_gain: ReplayGain::default(),
            sort_tags: SortTags::default(),
            is_missing: false,
            compilation: false,
            details: TrackDetails::default(),
        }
    }

    #[test]
    fn test_file_url_and_persistent_id() {
        assert_eq!(
            file_url("/Users/me/Music/Ä B&C#1.mp3"),
            "file://localhost/Users/me/Music/%C3%84%20B%26C%231.mp3"
        );
        // 取り込み（M3U）と同じ解釈で、元のパスへ戻る
        assert_eq!(
            crate::m3u::display_location(&file_url("/Volumes/NAS/曲 (1).flac")),
            "/Volumes/NAS/曲 (1).flac"
        );
        assert_eq!(
            persistent_id("a1000000-0000-4000-8000-000000000001"),
            "A100000000004000"
        );
        assert_eq!(persistent_id("p1"), "0000000000000001");
    }

    /// 書き出したXMLを、このアプリの取り込み（`library_xml`）で読み戻せる
    #[test]
    fn test_itunes_xml_round_trips_through_the_importer() {
        let mut played = track("a1000000-0000-4000-8000-000000000001", "/Music/曲 1.mp3");
        played.play_count = 12;
        played.skip_count = 3;
        played.is_favorite = true;
        played.rating = 7;
        played.last_played_at = Some("2026-02-03T04:05:06+00:00".to_string());
        let other = track(
            "a1000000-0000-4000-8000-000000000002",
            "/Music/Song, 2.flac",
        );
        let folder = PlaylistFolder {
            id: "f1000000-0000-4000-8000-000000000001".to_string(),
            name: "気分".to_string(),
            position: 0,
            created_at: String::new(),
        };
        let playlists = [ExportPlaylist {
            id: "b1000000-0000-4000-8000-000000000001".to_string(),
            name: "通勤".to_string(),
            folder_id: Some(folder.id.clone()),
            track_ids: vec![other.id.clone(), played.id.clone(), "unknown".to_string()],
        }];
        let value = build_itunes_xml(
            &[played.clone(), other.clone()],
            &playlists,
            std::slice::from_ref(&folder),
            Some("/Music"),
            "1.0.0",
            Utc::now(),
        );

        let dir = TempDir::new();
        let path = dir.path().join("Library.xml");
        std::fs::write(&path, to_xml_bytes(&value).unwrap()).unwrap();
        let library = crate::library_xml::parse(&path).unwrap();

        assert_eq!(library.tracks.len(), 2);
        let first = &library.tracks[0];
        assert_eq!(
            crate::m3u::display_location(&first.location),
            played.file_path
        );
        assert_eq!(first.stats.play_count, 12);
        assert_eq!(first.stats.skip_count, 3);
        assert!(first.stats.loved);
        assert_eq!(
            first.stats.last_played.map(|date| date.to_rfc3339()),
            Some("2026-02-03T04:05:06+00:00".to_string())
        );
        assert_eq!(
            first.stats.date_added.map(|date| date.to_rfc3339()),
            Some("2026-01-02T03:04:05+00:00".to_string())
        );
        // ライブラリ全体（Master）とフォルダは、プレイリストとして読まない
        assert_eq!(library.playlists.len(), 1);
        assert_eq!(library.playlists[0].name, "通勤");
        assert_eq!(
            library.playlists[0].track_ids,
            [library.tracks[1].track_id, first.track_id]
        );

        // 評価は0〜100、フォルダは親の印で表す
        let Value::Dictionary(root) = &value else {
            panic!("root is not a dictionary");
        };
        let tracks = root.get("Tracks").and_then(Value::as_dictionary).unwrap();
        let rating = tracks
            .get("1000")
            .and_then(Value::as_dictionary)
            .and_then(|track| track.get("Rating"))
            .and_then(Value::as_signed_integer);
        assert_eq!(rating, Some(70));
        let playlist_values = root.get("Playlists").and_then(Value::as_array).unwrap();
        let parent = playlist_values[2]
            .as_dictionary()
            .and_then(|playlist| playlist.get("Parent Persistent ID"))
            .and_then(Value::as_string);
        assert_eq!(parent, Some(persistent_id(&folder.id).as_str()));
    }

    #[test]
    fn test_csv_quotes_values_and_neutralizes_formulas() {
        let mut first = track("t1", "/Music/a.mp3");
        first.title = Some("Hello, \"World\"".to_string());
        first.artist = Some("=HYPERLINK(\"x\")".to_string());
        first.rating = 7;
        first.is_favorite = true;
        first.duration = Some(3725);
        let csv = build_csv(&[first], Language::En);

        assert!(csv.starts_with("\u{FEFF}Title,Artist,"));
        let row = csv.lines().nth(1).unwrap();
        assert!(row.starts_with("\"Hello, \"\"World\"\"\",\"'=HYPERLINK(\"\"x\"\")\","));
        assert!(row.contains(",1:02:05,3.5,1,"));
        assert!(csv.ends_with("/Music/a.mp3\r\n"));
        assert!(build_csv(&[], Language::Ja).starts_with("\u{FEFF}タイトル,"));
    }
}
