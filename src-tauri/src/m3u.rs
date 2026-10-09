//! プレイリストのM3U（M3U8）の読み込みと書き出し
//!
//! 読み込み（ADR-032）:
//!
//! - 文字コードは自動で判定する（BOMがあればそれに従い、なければUTF-8、UTF-8として
//!   正しくなければShift_JIS）
//! - 1行に1曲の場所（パス）が書かれている。`#`で始まる行（`#EXTM3U`・`#EXTINF`など）は読み飛ばす
//! - 曲の場所は、ライブラリの曲のパスと突き合わせる。別のOS・別の場所で書き出したM3U
//!   （`C:\Music\…`・区切り文字の違い・フォルダの場所の違い）でも対応が付くよう、
//!   パスの末尾（ファイル名と、その上のフォルダ）がいちばん長く一致する曲を選ぶ
//!
//! 書き出し: 拡張M3U（`#EXTINF`付き）を、UTF-8の`.m3u8`として書く。パスは、絶対パスか、
//! 書き出し先のフォルダからの相対パス。

use crate::error::{AppError, AppResult};
use crate::models::Track;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// 読み込むM3Uのファイルの大きさの上限（プレイリストではないファイルを読み続けないようにする）
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// 結果に含める、対応が付かなかった行の数の上限
pub const MAX_REPORTED_UNMATCHED: usize = 200;

/// M3Uのファイルを読んで、ライブラリの曲と突き合わせた結果
#[derive(Debug, PartialEq)]
pub struct ReadPlaylist {
    /// ファイル名から作った、プレイリストの名前
    pub name: String,
    /// 対応が付いた曲（M3Uに書かれていた順。同じ曲は最初の1回だけ）
    pub track_ids: Vec<String>,
    /// 同じ曲が2回以上書かれていて、飛ばした数
    pub duplicate_count: u32,
    /// 対応が付かなかった行の数
    pub unmatched_count: u32,
    /// 対応が付かなかった行（先頭の`MAX_REPORTED_UNMATCHED`件まで）
    pub unmatched: Vec<String>,
}

/// M3Uのファイルを読み、ライブラリの曲と突き合わせる（DBには触れない）
pub fn read_playlist(path: &Path, matcher: &TrackMatcher) -> AppResult<ReadPlaylist> {
    let size = std::fs::metadata(path)
        .map_err(|e| AppError::Io(format!("ファイルを読めません: {}", e)))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(AppError::Validation(
            "ファイルが大きすぎます（プレイリストのファイルではない可能性があります）".to_string(),
        ));
    }
    let bytes =
        std::fs::read(path).map_err(|e| AppError::Io(format!("ファイルを読めません: {}", e)))?;
    let text = decode(&bytes);
    let base = path.parent().unwrap_or(Path::new("/"));

    let mut playlist = ReadPlaylist {
        name: playlist_name_from_file(path),
        track_ids: Vec::new(),
        duplicate_count: 0,
        unmatched_count: 0,
        unmatched: Vec::new(),
    };
    let mut seen = HashSet::new();
    for line in entries(&text) {
        match matcher.find(line, base) {
            Some(track_id) if seen.insert(track_id) => {
                playlist.track_ids.push(track_id.to_string());
            }
            Some(_) => playlist.duplicate_count += 1,
            None => {
                playlist.unmatched_count += 1;
                if playlist.unmatched.len() < MAX_REPORTED_UNMATCHED {
                    playlist.unmatched.push(line.to_string());
                }
            }
        }
    }
    Ok(playlist)
}

/// M3Uのファイルの中身を、文字列にする（文字コードを自動で判定する）
pub fn decode(bytes: &[u8]) -> String {
    // BOMがあれば、それに従う（UTF-8・UTF-16）
    if let Some((encoding, bom_length)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding
            .decode_without_bom_handling(&bytes[bom_length..])
            .0
            .into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        // UTF-8として正しくなければ、Shift_JIS（日本語のWindowsの文字コード）として読む
        Err(_) => encoding_rs::SHIFT_JIS
            .decode_without_bom_handling(bytes)
            .0
            .into_owned(),
    }
}

/// M3Uの中の、曲の場所の行を取り出す（空行と、`#`で始まる行は除く）
pub fn entries(text: &str) -> Vec<&str> {
    text.split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// 曲の場所の書き方
enum Location {
    /// このコンピューターの絶対パス（`/`で始まる）か、M3Uのフォルダから解決した相対パス
    Local(Vec<String>),
    /// 別のOSの絶対パス（`C:\…`・`\\server\…`）。フォルダの場所は当てにならない
    Foreign(Vec<String>),
    /// ファイルではないもの（`http://`など）
    Unsupported,
}

/// 比較用に、フォルダ・ファイルの名前をそろえる（NFC・小文字）
///
/// macOSのファイル名は濁点などを分けた形（NFD）のことがあり、Windowsで書き出したM3Uは
/// 合わせた形（NFC）のため、そろえて比べる。大文字・小文字の違いも同じとみなす。
fn normalize_name(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// パスを、フォルダ・ファイルの名前の並びにする（区切りは`/`と`\`。`.`は除き、`..`は1つ上へ戻る）
fn push_components(components: &mut Vec<String>, path: &str) {
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            name => components.push(normalize_name(name)),
        }
    }
}

/// `file://`のURLの`%XX`を、元の文字へ戻す
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = (bytes[index] == b'%')
            .then(|| value.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// 曲の場所を、表示用の文字列にする（`file://`のURLは、パスへ戻す）
pub fn display_location(location: &str) -> String {
    let is_file_url = location
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"));
    if !is_file_url {
        return location.to_string();
    }
    let rest = &location[7..];
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest);
    // `/C:/Music/…`は、先頭の`/`を除く
    match decoded.strip_prefix('/') {
        Some(windows) if windows.split('/').next().is_some_and(is_drive) => windows.to_string(),
        _ => decoded,
    }
}

/// `C:`のような、Windowsのドライブの指定か
fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// M3Uの1行を、曲の場所として解釈する
///
/// - `base`: M3Uのファイルがあるフォルダ（相対パスの基準）
fn parse_location(line: &str, base: &Path) -> Location {
    // file://のURLは、パスへ戻す（`file:///C:/Music/a.mp3`・`file://localhost/Users/…`）
    let is_file_url = line
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"));
    let decoded;
    let path = if is_file_url {
        decoded = display_location(line);
        decoded.as_str()
    } else if line.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    }) {
        return Location::Unsupported;
    } else {
        line
    };

    let mut components = Vec::new();
    let first = path.split(['/', '\\']).next().unwrap_or("");
    if is_drive(first) {
        // ドライブの指定は、突き合わせに使わない
        push_components(&mut components, &path[first.len()..]);
        Location::Foreign(components)
    } else if path.starts_with("\\\\") {
        push_components(&mut components, path);
        Location::Foreign(components)
    } else if path.starts_with('/') {
        push_components(&mut components, path);
        Location::Local(components)
    } else {
        // 相対パス: M3Uのあるフォルダから解決する
        push_components(&mut components, &base.to_string_lossy());
        push_components(&mut components, path);
        Location::Local(components)
    }
}

/// M3Uの曲の場所を、ライブラリの曲と突き合わせる
pub struct TrackMatcher {
    /// ライブラリの曲（トラックIDと、パスの名前の並び）
    tracks: Vec<(String, Vec<String>)>,
    /// ファイル名から、その名前の曲（`tracks`の中の位置）を引く
    by_file_name: HashMap<String, Vec<usize>>,
}

impl TrackMatcher {
    /// - `library`: ライブラリの曲の、トラックIDとファイルのパス
    pub fn new(library: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut tracks = Vec::new();
        let mut by_file_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (track_id, file_path) in library {
            let mut components = Vec::new();
            push_components(&mut components, &file_path);
            let Some(file_name) = components.last() else {
                continue;
            };
            by_file_name
                .entry(file_name.clone())
                .or_default()
                .push(tracks.len());
            tracks.push((track_id, components));
        }
        Self {
            tracks,
            by_file_name,
        }
    }

    /// M3Uの1行に対応する曲のトラックIDを返す（対応が付かなければ`None`）
    ///
    /// - このコンピューターのパスとしてそのまま一致する曲があれば、その曲
    /// - なければ、ファイル名が同じ曲のうち、パスの末尾（上のフォルダ）がいちばん長く一致する曲。
    ///   同じ長さで一致する曲が複数ある場合は、どれか決められないため対応を付けない
    pub fn find(&self, line: &str, base: &Path) -> Option<&str> {
        let (components, local) = match parse_location(line, base) {
            Location::Local(components) => (components, true),
            Location::Foreign(components) => (components, false),
            Location::Unsupported => return None,
        };
        let candidates = self.by_file_name.get(components.last()?)?;

        if local
            && let Some(&index) = candidates
                .iter()
                .find(|&&index| self.tracks[index].1 == components)
        {
            return Some(&self.tracks[index].0);
        }

        let mut best: Option<(usize, usize)> = None;
        let mut tied = false;
        for &index in candidates {
            let matched = common_suffix_length(&self.tracks[index].1, &components);
            match best {
                Some((_, length)) if matched < length => {}
                Some((_, length)) if matched == length => tied = true,
                _ => {
                    best = Some((index, matched));
                    tied = false;
                }
            }
        }
        match best {
            Some((index, _)) if !tied => Some(&self.tracks[index].0),
            _ => None,
        }
    }
}

/// 2つのパスの、末尾から一致する名前の数
fn common_suffix_length(a: &[String], b: &[String]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

/// M3Uのファイル名から、プレイリストの名前を作る
///
/// 拡張子を除き、プレイリスト名に使えない文字（`validate_playlist_name`）を`_`にする。
/// 名前が空になる場合は`Playlist`にする。
pub fn playlist_name_from_file(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    sanitize_playlist_name(&stem)
}

/// プレイリスト名に使えない文字（`validate_playlist_name`）を`_`にする
///
/// 名前が空になる場合は`Playlist`にする。
pub fn sanitize_playlist_name(name: &str) -> String {
    let name: String = name
        .nfc()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Playlist".to_string()
    } else {
        name.to_string()
    }
}

/// プレイリスト名の長さの上限（バイト数。`validate_playlist_name`と同じ）
const MAX_PLAYLIST_NAME_BYTES: usize = 100;

/// 文字の途中で切らずに、指定したバイト数までに収める
fn truncate_to_bytes(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].trim_end()
}

/// すでにあるプレイリストと重ならない名前にする（重なる場合は` (2)`・` (3)`…を付ける）
pub fn unique_playlist_name(name: &str, existing: &[String]) -> String {
    let base = truncate_to_bytes(name, MAX_PLAYLIST_NAME_BYTES);
    if !existing.iter().any(|other| other == base) {
        return base.to_string();
    }
    (2..)
        .map(|number| {
            let suffix = format!(" ({})", number);
            let base = truncate_to_bytes(name, MAX_PLAYLIST_NAME_BYTES - suffix.len());
            format!("{}{}", base, suffix)
        })
        .find(|candidate| !existing.iter().any(|other| other == candidate))
        .expect("番号を増やせば、重ならない名前が見つかる")
}

/// プレイリストでの曲の表示名（`アーティスト - タイトル`。改行などの制御文字は空白にする）
fn track_label(track: &Track) -> String {
    let non_empty = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let title = non_empty(&track.title).unwrap_or_else(|| track.file_name.clone());
    let label = match non_empty(&track.artist) {
        Some(artist) => format!("{} - {}", artist, title),
        None => title,
    };
    label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// 書き出し先のフォルダから見た、曲のファイルの相対パス（区切りは`/`）
///
/// 共通するフォルダがない（別のボリュームなど）場合は、絶対パスのまま返す。
fn relative_path(file_path: &str, base: &Path) -> String {
    let file: Vec<&str> = file_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let base = base.to_string_lossy();
    let base: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();

    let common = file
        .iter()
        .zip(&base)
        .take_while(|(file, base)| file == base)
        .count();
    if common == 0 || !file_path.starts_with('/') {
        return file_path.to_string();
    }

    let mut parts: Vec<&str> = vec![".."; base.len() - common];
    parts.extend(&file[common..]);
    parts.join("/")
}

/// プレイリストのファイル（拡張M3U、UTF-8）の内容を作る
///
/// - `base`: 書き出し先のフォルダ。渡すと、曲の場所をそこからの相対パスで書く（`None`は絶対パス）
pub fn build_m3u8(tracks: &[Track], base: Option<&Path>) -> String {
    let mut content = String::from("#EXTM3U\r\n");
    for track in tracks {
        let path = match base {
            Some(base) => relative_path(&track.file_path, base),
            None => track.file_path.clone(),
        };
        content.push_str(&format!(
            "#EXTINF:{},{}\r\n{}\r\n",
            track.duration.unwrap_or(-1),
            track_label(track),
            path
        ));
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ReplayGain;

    fn library() -> TrackMatcher {
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
                ("b1", "/Users/me/Music/ネオン通り/夜明け/01 Intro.flac"),
                ("c1", "/Users/me/Music/The Voltage/Circuit/01 Intro.flac"),
                ("d1", "/Volumes/SD/Live/Disc 1/01 Intro.flac"),
                ("d2", "/Volumes/SD/Live/Disc 2/01 Intro.flac"),
            ]
            .map(|(id, path)| (id.to_string(), path.to_string())),
        )
    }

    fn find(line: &str) -> Option<String> {
        library()
            .find(line, Path::new("/Users/me/Playlists"))
            .map(str::to_string)
    }

    #[test]
    fn test_decode_detects_the_encoding() {
        let text = "#EXTM3U\r\nC:\\Music\\ソフト\\表示.mp3\r\n";

        // UTF-8（BOMなし・BOM付き）
        assert_eq!(decode(text.as_bytes()), text);
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend(text.as_bytes());
        assert_eq!(decode(&with_bom), text);

        // UTF-16（BOM付き）
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&utf16), text);

        // Shift_JIS: 「ソ」「表」は、2バイト目が`\`（0x5C）と同じ値になる。文字として読んでから
        // 区切るため、パスの区切りと取り違えない
        let (shift_jis, _, _) = encoding_rs::SHIFT_JIS.encode(text);
        assert!(std::str::from_utf8(&shift_jis).is_err());
        assert_eq!(decode(&shift_jis), text);
    }

    #[test]
    fn test_entries_skips_comments_and_blank_lines() {
        let text =
            "#EXTM3U\r\n#EXTINF:214,Aoi Sora - 青い地平線\r\n  a.mp3  \r\n\r\nb/c.mp3\rd.mp3\n";
        assert_eq!(entries(text), vec!["a.mp3", "b/c.mp3", "d.mp3"]);
        assert!(entries("").is_empty());
    }

    #[test]
    fn test_find_matches_local_paths() {
        // 絶対パス
        assert_eq!(
            find("/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3").as_deref(),
            Some("a1")
        );
        // M3Uのフォルダからの相対パス（`..`・`.`を含む）
        assert_eq!(
            find("../Music/./Aoi Sora/Blue Horizon/02 Paper Planes.mp3").as_deref(),
            Some("a2")
        );
        // file://のURL
        assert_eq!(
            find("file:///Users/me/Music/Aoi%20Sora/Blue%20Horizon/02%20Paper%20Planes.mp3")
                .as_deref(),
            Some("a2")
        );
        // 同じファイル名の曲がほかにあっても、パスがそのまま一致する曲を選ぶ
        assert_eq!(
            find("/Volumes/SD/Live/Disc 2/01 Intro.flac").as_deref(),
            Some("d2")
        );
    }

    #[test]
    fn test_find_matches_paths_from_another_computer_by_their_tail() {
        // Windowsの絶対パス（区切り文字・大文字と小文字の違い）
        assert_eq!(
            find("C:\\Users\\taro\\Music\\AOI SORA\\Blue Horizon\\01 青い地平線.mp3").as_deref(),
            Some("a1")
        );
        // file://のWindowsのパス
        assert_eq!(
            find("file:///D:/Music/Aoi%20Sora/Blue%20Horizon/02%20Paper%20Planes.mp3").as_deref(),
            Some("a2")
        );
        // 同じファイル名の曲が複数ある場合は、上のフォルダがいちばん長く一致する曲
        assert_eq!(
            find("D:\\Music\\The Voltage\\Circuit\\01 Intro.flac").as_deref(),
            Some("c1")
        );
        assert_eq!(
            find("\\\\nas\\music\\Live\\Disc 2\\01 Intro.flac").as_deref(),
            Some("d2")
        );
        // 別の場所にある相対パスのM3U（フォルダの場所が違う）
        assert_eq!(
            find("ネオン通り/夜明け/01 Intro.flac").as_deref(),
            Some("b1")
        );
        // ファイル名が同じ曲が1つだけなら、ファイル名だけで対応を付ける
        assert_eq!(
            find("E:\\backup\\02 Paper Planes.mp3").as_deref(),
            Some("a2")
        );
    }

    #[test]
    fn test_find_normalizes_unicode_forms() {
        // ライブラリのパスがNFD（濁点を分けた形）でも、NFCのM3Uと対応が付く
        let nfd: String = "/Users/me/Music/ガール/デモ.mp3".nfd().collect();
        let matcher = TrackMatcher::new([("x".to_string(), nfd)]);
        assert_eq!(
            matcher.find("C:\\Music\\ガール\\デモ.mp3", Path::new("/tmp")),
            Some("x")
        );
    }

    #[test]
    fn test_find_returns_none_when_no_track_or_several_tracks_match() {
        // ライブラリにないファイル
        assert_eq!(find("C:\\Music\\Unknown\\99 Nothing.mp3"), None);
        // ファイル名しか一致せず、同じ名前の曲が複数ある
        assert_eq!(find("E:\\backup\\01 Intro.flac"), None);
        // 上のフォルダまで同じ長さで一致する曲が複数ある
        let matcher = TrackMatcher::new(
            [
                ("p", "/Users/me/A/Album/01.mp3"),
                ("q", "/Users/me/B/Album/01.mp3"),
            ]
            .map(|(id, path)| (id.to_string(), path.to_string())),
        );
        assert_eq!(matcher.find("C:\\Album\\01.mp3", Path::new("/tmp")), None);
        // ファイルではないもの
        assert_eq!(find("https://example.com/stream.mp3"), None);
        assert_eq!(find("/"), None);
    }

    #[test]
    fn test_playlist_name_from_file() {
        assert_eq!(
            playlist_name_from_file(Path::new("/tmp/ドライブ用.m3u8")),
            "ドライブ用"
        );
        assert_eq!(
            playlist_name_from_file(Path::new("/tmp/a:b?c*.m3u")),
            "a_b_c_"
        );
        assert_eq!(playlist_name_from_file(Path::new("/tmp/ .m3u")), "Playlist");
    }

    #[test]
    fn test_unique_playlist_name() {
        let existing = vec!["通勤".to_string(), "通勤 (2)".to_string()];
        assert_eq!(unique_playlist_name("作業用", &existing), "作業用");
        assert_eq!(unique_playlist_name("通勤", &existing), "通勤 (3)");

        // 長い名前は、番号を付けた後も上限（100バイト）に収める
        let long = "あ".repeat(40);
        let name = unique_playlist_name(&long, &[]);
        assert_eq!(name, "あ".repeat(33));
        let numbered = unique_playlist_name(&long, &[name]);
        assert!(numbered.len() <= 100);
        assert!(numbered.ends_with(" (2)"));
        assert!(crate::validation::validate_playlist_name(&numbered).is_ok());
    }

    fn track(file_path: &str, title: Option<&str>, artist: Option<&str>) -> Track {
        Track {
            id: "id".to_string(),
            file_path: file_path.to_string(),
            file_name: file_path.rsplit('/').next().unwrap().to_string(),
            title: title.map(str::to_string),
            artist: artist.map(str::to_string),
            album: None,
            album_artist: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration: Some(214),
            file_size: 1,
            format: "mp3".to_string(),
            bitrate: None,
            sample_rate: None,
            is_favorite: false,
            rating: 0,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            replay_gain: ReplayGain::default(),
            sort_tags: Default::default(),
            is_missing: false,
        }
    }

    #[test]
    fn test_build_m3u8_with_absolute_and_relative_paths() {
        let mut untitled = track("/Volumes/SD/Other/x.flac", None, None);
        untitled.duration = None;
        let tracks = vec![
            track(
                "/Users/me/Music/Aoi Sora/01 青い地平線.mp3",
                Some("青い地平線"),
                Some("Aoi Sora"),
            ),
            track("/Users/me/Playlists/local.mp3", Some("改行\nあり"), None),
            untitled,
        ];

        assert_eq!(
            build_m3u8(&tracks, None),
            "#EXTM3U\r\n\
             #EXTINF:214,Aoi Sora - 青い地平線\r\n/Users/me/Music/Aoi Sora/01 青い地平線.mp3\r\n\
             #EXTINF:214,改行 あり\r\n/Users/me/Playlists/local.mp3\r\n\
             #EXTINF:-1,x.flac\r\n/Volumes/SD/Other/x.flac\r\n"
        );
        // 相対パス: 書き出し先のフォルダから。共通するフォルダがない曲は絶対パスのまま
        assert_eq!(
            build_m3u8(&tracks, Some(Path::new("/Users/me/Playlists"))),
            "#EXTM3U\r\n\
             #EXTINF:214,Aoi Sora - 青い地平線\r\n../Music/Aoi Sora/01 青い地平線.mp3\r\n\
             #EXTINF:214,改行 あり\r\nlocal.mp3\r\n\
             #EXTINF:-1,x.flac\r\n/Volumes/SD/Other/x.flac\r\n"
        );
    }

    #[test]
    fn test_read_playlist_reads_a_shift_jis_file_from_windows() {
        let dir = std::env::temp_dir().join(format!("muspice-m3u-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("通勤:朝.m3u");
        let text = "#EXTM3U\r\n\
                    #EXTINF:214,Aoi Sora - 青い地平線\r\n\
                    C:\\Music\\Aoi Sora\\Blue Horizon\\01 青い地平線.mp3\r\n\
                    C:\\Music\\Aoi Sora\\Blue Horizon\\02 Paper Planes.mp3\r\n\
                    C:\\Music\\Aoi Sora\\Blue Horizon\\01 青い地平線.mp3\r\n\
                    C:\\Music\\ソフト\\表示.mp3\r\n\
                    http://example.com/radio\r\n";
        std::fs::write(&path, encoding_rs::SHIFT_JIS.encode(text).0).unwrap();

        let playlist = read_playlist(&path, &library()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            playlist,
            ReadPlaylist {
                name: "通勤_朝".to_string(),
                track_ids: vec!["a1".to_string(), "a2".to_string()],
                duplicate_count: 1,
                unmatched_count: 2,
                unmatched: vec![
                    "C:\\Music\\ソフト\\表示.mp3".to_string(),
                    "http://example.com/radio".to_string()
                ],
            }
        );
    }

    #[test]
    fn test_read_playlist_limits_reported_unmatched_lines() {
        let dir = std::env::temp_dir().join(format!("muspice-m3u-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("many.m3u8");
        let lines: Vec<String> = (0..MAX_REPORTED_UNMATCHED + 50)
            .map(|index| format!("missing/{index}.mp3"))
            .collect();
        std::fs::write(&path, lines.join("\n")).unwrap();

        let playlist = read_playlist(&path, &library()).unwrap();
        let missing = read_playlist(&dir.join("none.m3u8"), &library());
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(playlist.track_ids.is_empty());
        assert_eq!(
            playlist.unmatched_count as usize,
            MAX_REPORTED_UNMATCHED + 50
        );
        assert_eq!(playlist.unmatched.len(), MAX_REPORTED_UNMATCHED);
        assert!(matches!(missing, Err(AppError::Io(_))));
    }

    #[test]
    fn test_written_m3u8_can_be_read_back() {
        let tracks = vec![track(
            "/Users/me/Music/Aoi Sora/Blue Horizon/01 青い地平線.mp3",
            Some("青い地平線"),
            Some("Aoi Sora"),
        )];
        for base in [None, Some(Path::new("/Users/me/Playlists"))] {
            let content = build_m3u8(&tracks, base);
            let lines = entries(&content);
            assert_eq!(lines.len(), 1);
            assert_eq!(
                library().find(lines[0], Path::new("/Users/me/Playlists")),
                Some("a1")
            );
        }
    }
}
