//! CUEシート（アルバム1ファイル＋`.cue`）の曲（ADR-123）
//!
//! アルバムを1つの音声ファイルにして、CUEシートに曲の区切りを書いた音源を、曲ごとに一覧・再生する。
//!
//! - 走査（`library::scan_directory`）で、CUEシートが2曲以上を書いている音声ファイルを、その曲の
//!   仮のパス（`アルバム.cue#03`。`virtual_path`）に置き換える。取り込み・再スキャンは、仮のパスを
//!   ふつうのファイルと同じく扱う（新しい・変わった・なくなった曲を、パスで突き合わせる）
//! - 曲の情報は、CUEシートの`TITLE`・`PERFORMER`・`SONGWRITER`・`REM GENRE`・`REM DATE`と、
//!   音声ファイルのタグ・オーディオの情報から作る（`read_track`）。曲の範囲は`INDEX 01`から、
//!   次の曲の`INDEX 01`まで（最後の曲は、ファイルの終わりまで）
//! - 再生は、元の音声ファイルの範囲だけを鳴らす（`playback::TrackRegion`）
//! - ファイルを書き換える操作（タグの編集など）には対応しない（1つのファイルに複数の曲があるため）
//!
//! 位置は、CUEシートと同じ1/75秒（CDのフレーム）の単位で持つ（44.1kHzなら、ちょうど588サンプル）。

use crate::error::{AppError, AppResult};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// CUEシートの位置の単位（1秒あたりのフレーム数）
pub const FRAMES_PER_SECOND: u64 = 75;

/// 読み込むCUEシートの大きさの上限（CUEシートではない大きなファイルを読まない）
const MAX_CUE_BYTES: u64 = 1024 * 1024;

/// CUEシートの1曲
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CueTrack {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    /// 曲の始まり（`INDEX 01`。1/75秒）。`INDEX 01`のない曲はNone
    pub start: Option<u64>,
}

/// CUEシートの`FILE`（1つの音声ファイルと、その中の曲）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CueFile {
    /// 書いてあるファイル名（CUEシートのあるフォルダからの場所）
    pub name: String,
    pub tracks: Vec<CueTrack>,
}

/// CUEシート
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CueSheet {
    /// アルバムのアーティストとタイトル（`FILE`より前の`PERFORMER`・`TITLE`）
    pub performer: Option<String>,
    pub title: Option<String>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub disc_number: Option<i32>,
    pub files: Vec<CueFile>,
}

/// 行の残りから、値を1つ取り出す（`"…"`で囲んだ値は、囲みを外す）
fn value(rest: &str) -> Option<String> {
    let rest = rest.trim();
    let text = match rest.strip_prefix('"') {
        Some(quoted) => match quoted.rfind('"') {
            Some(end) => &quoted[..end],
            None => quoted,
        },
        None => rest,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// `FILE "名前" WAVE`の、ファイル名
fn file_name(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        return value(&quoted[..end]);
    }
    // 囲みのない名前は、最後の語（形式）の前まで
    let name = match rest.rsplit_once(char::is_whitespace) {
        Some((name, _)) => name,
        None => rest,
    };
    value(name)
}

/// `mm:ss:ff`を、1/75秒の単位にする
fn position(text: &str) -> Option<u64> {
    let mut parts = text.trim().split(':');
    let minutes: u64 = parts.next()?.trim().parse().ok()?;
    let seconds: u64 = parts.next()?.trim().parse().ok()?;
    let frames: u64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || seconds >= 60 || frames >= FRAMES_PER_SECOND {
        return None;
    }
    Some((minutes * 60 + seconds) * FRAMES_PER_SECOND + frames)
}

/// CUEシートの文字列を読む（読めない行は飛ばす）
pub fn parse(text: &str) -> CueSheet {
    let mut sheet = CueSheet::default();
    // 番号を読めない`TRACK`の行の後（次の`TRACK`・`FILE`まで）は、前の曲の値にしない
    let mut in_unreadable_track = false;
    for line in text.lines() {
        let line = line.trim();
        let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let command = command.to_ascii_uppercase();
        let before_files = sheet.files.is_empty();
        let current_track = sheet
            .files
            .last_mut()
            .and_then(|file| file.tracks.last_mut())
            .filter(|_| !in_unreadable_track);
        match command.as_str() {
            "FILE" => {
                in_unreadable_track = false;
                if let Some(name) = file_name(rest) {
                    sheet.files.push(CueFile {
                        name,
                        tracks: Vec::new(),
                    });
                }
            }
            "TRACK" => {
                let number = rest
                    .split_whitespace()
                    .next()
                    .and_then(|number| number.parse().ok());
                in_unreadable_track = number.is_none();
                if let (Some(number), Some(file)) = (number, sheet.files.last_mut()) {
                    file.tracks.push(CueTrack {
                        number,
                        ..CueTrack::default()
                    });
                }
            }
            "INDEX" => {
                let mut parts = rest.split_whitespace();
                let is_start = parts.next().and_then(|index| index.parse::<u32>().ok()) == Some(1);
                if let (true, Some(track), Some(start)) =
                    (is_start, current_track, parts.next().and_then(position))
                {
                    track.start = Some(start);
                }
            }
            "TITLE" | "PERFORMER" | "SONGWRITER" => {
                let text = value(rest);
                match (current_track, command.as_str()) {
                    (Some(track), "TITLE") => track.title = text,
                    (Some(track), "PERFORMER") => track.performer = text,
                    (Some(track), _) => track.songwriter = text,
                    // `FILE`より前（アルバムの値）
                    (None, "TITLE") if before_files => sheet.title = text,
                    (None, "PERFORMER") if before_files => sheet.performer = text,
                    _ => {}
                }
            }
            "REM" => {
                let (key, rest) = rest
                    .trim()
                    .split_once(char::is_whitespace)
                    .unwrap_or((rest, ""));
                match key.to_ascii_uppercase().as_str() {
                    "GENRE" => sheet.genre = value(rest),
                    "DATE" => sheet.date = value(rest),
                    "DISCNUMBER" => sheet.disc_number = value(rest).and_then(|n| n.parse().ok()),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    sheet
}

/// CUEシートのファイルを読む（文字コードは、M3Uと同じく自動で判定する）
pub fn read(path: &Path) -> AppResult<CueSheet> {
    let size = fs::metadata(path)
        .map_err(|e| AppError::Io(format!("CUEシートを読めません: {}", e)))?
        .len();
    if size > MAX_CUE_BYTES {
        return Err(AppError::Validation("CUEシートが大きすぎます".to_string()));
    }
    let bytes =
        fs::read(path).map_err(|e| AppError::Io(format!("CUEシートを読めません: {}", e)))?;
    Ok(parse(&crate::m3u::decode(&bytes)))
}

/// CUEシートの曲の仮のパス（`CUEシートのパス#03`）
pub fn virtual_path(cue_path: &Path, number: u32) -> String {
    format!("{}#{:02}", cue_path.to_string_lossy(), number)
}

/// 仮のパスを、CUEシートのパスと曲の番号に分ける（仮のパスでなければNone）
pub fn parse_virtual(path: &str) -> Option<(PathBuf, u32)> {
    let (cue, number) = path.rsplit_once('#')?;
    let is_cue = Path::new(cue)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cue"));
    if !is_cue || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((PathBuf::from(cue), number.parse().ok()?))
}

/// パスが、CUEシートの曲の仮のパスか
pub fn is_virtual(path: &str) -> bool {
    parse_virtual(path).is_some()
}

/// CUEシートの`FILE`が指す音声ファイル（`audio_files`の中から選ぶ。見つからなければNone）
///
/// 書いてある名前のファイルがなければ、拡張子を除いた名前が同じ音声ファイルを使う（取り込んだ後に
/// 形式を変えた音源（`.wav`と書いてあるが`.flac`がある）が多いため）。大文字と小文字は区別しない。
fn resolve_audio(cue_dir: &Path, name: &str, audio_files: &[&Path]) -> Option<PathBuf> {
    let written = cue_dir.join(name.replace('\\', "/"));
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    if let Some(found) = audio_files
        .iter()
        .find(|path| lower(path) == lower(&written))
    {
        return Some(found.to_path_buf());
    }
    let stem = |path: &Path| {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
    };
    let written_stem = stem(&written)?;
    let same_stem: Vec<&&Path> = audio_files
        .iter()
        .filter(|path| {
            path.parent() == written.parent() && stem(path).as_deref() == Some(&written_stem)
        })
        .collect();
    match same_stem.as_slice() {
        [only] => Some(only.to_path_buf()),
        _ => None,
    }
}

/// 走査で見つけた音声ファイルのうち、CUEシートが2曲以上を書いているものを、その曲の仮のパスに置き換える
///
/// 同じ音声ファイルを複数のCUEシートが指す場合は、名前の順で最初のCUEシートを使う。読めない
/// CUEシート・曲が1つのファイル・`INDEX 01`のない曲があるファイルは、置き換えない（ふつうのファイルの
/// まま）。
pub fn expand(audio_files: Vec<PathBuf>, cue_files: &[PathBuf]) -> Vec<PathBuf> {
    if cue_files.is_empty() {
        return audio_files;
    }
    let mut by_dir: HashMap<&Path, Vec<&Path>> = HashMap::new();
    for path in &audio_files {
        if let Some(dir) = path.parent() {
            by_dir.entry(dir).or_default().push(path);
        }
    }

    let mut sorted_cues: Vec<&PathBuf> = cue_files.iter().collect();
    sorted_cues.sort();
    let mut covered: HashSet<PathBuf> = HashSet::new();
    let mut virtual_paths: Vec<PathBuf> = Vec::new();
    for cue_path in sorted_cues {
        let Some(dir) = cue_path.parent() else {
            continue;
        };
        let Some(dir_audio) = by_dir.get(dir) else {
            continue;
        };
        let sheet = match read(cue_path) {
            Ok(sheet) => sheet,
            Err(e) => {
                log::warn!("Failed to read CUE sheet {}: {}", cue_path.display(), e);
                continue;
            }
        };
        for file in &sheet.files {
            if file.tracks.len() < 2 || file.tracks.iter().any(|track| track.start.is_none()) {
                continue;
            }
            let Some(audio) = resolve_audio(dir, &file.name, dir_audio) else {
                continue;
            };
            if !covered.insert(audio) {
                continue;
            }
            virtual_paths.extend(
                file.tracks
                    .iter()
                    .map(|track| PathBuf::from(virtual_path(cue_path, track.number))),
            );
        }
    }
    if covered.is_empty() {
        return audio_files;
    }
    let mut expanded: Vec<PathBuf> = audio_files
        .into_iter()
        .filter(|path| !covered.contains(path))
        .chain(virtual_paths)
        .collect();
    expanded.sort();
    expanded
}

/// CUEシートの1曲を、ライブラリに登録するために読んだもの
#[derive(Debug, Clone, PartialEq)]
pub struct CueTrackSource {
    /// 曲の入っている音声ファイル
    pub audio_path: PathBuf,
    pub sheet: CueSheet,
    pub track: CueTrack,
    /// 曲の範囲（1/75秒。終わりがNoneなら、ファイルの終わりまで）
    pub start: u64,
    pub end: Option<u64>,
}

/// 仮のパスの曲を、CUEシートから読む
pub fn read_track(virtual_path: &str) -> AppResult<CueTrackSource> {
    let not_found =
        || AppError::NotFound(format!("CUEシートの曲が見つかりません: {}", virtual_path));
    let (cue_path, number) = parse_virtual(virtual_path).ok_or_else(not_found)?;
    let sheet = read(&cue_path)?;
    let dir = cue_path.parent().ok_or_else(not_found)?;
    let dir_audio: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| AppError::Io(format!("フォルダを読めません: {}", e)))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| crate::library::is_supported_audio_file(path))
        .collect();
    let dir_audio: Vec<&Path> = dir_audio.iter().map(PathBuf::as_path).collect();
    for file in &sheet.files {
        let Some(index) = file.tracks.iter().position(|track| track.number == number) else {
            continue;
        };
        let audio_path = resolve_audio(dir, &file.name, &dir_audio).ok_or_else(not_found)?;
        let track = file.tracks[index].clone();
        let start = track.start.ok_or_else(not_found)?;
        let end = file.tracks.get(index + 1).and_then(|next| next.start);
        return Ok(CueTrackSource {
            audio_path,
            sheet: sheet.clone(),
            track,
            start,
            end,
        });
    }
    Err(not_found())
}

/// 仮のパスの曲の、変わったかを見るための大きさと更新日時
///
/// 大きさは音声ファイルのもの、更新日時はCUEシートと音声ファイルの新しい方（CUEシートを書き直しても、
/// 再スキャンで読み直す）。どちらかがなければNone。
pub fn file_state(virtual_path: &str) -> Option<(i64, Option<i64>)> {
    let source = read_track(virtual_path).ok()?;
    let (cue_path, _) = parse_virtual(virtual_path)?;
    let audio = fs::metadata(&source.audio_path).ok()?;
    let cue = fs::metadata(&cue_path).ok()?;
    let modified_at = match (
        crate::library::modified_at_of(&audio),
        crate::library::modified_at_of(&cue),
    ) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    Some((i64::try_from(audio.len()).unwrap_or(i64::MAX), modified_at))
}

/// 仮のパスの曲を、ライブラリに登録する曲にする（新しいIDを振る）
///
/// 曲の名前・アーティスト・作曲者はCUEシートの曲、アルバム・アルバムアーティスト・ジャンル・年は
/// CUEシートの値（なければ音声ファイルのタグ）。長さは範囲から、大きさ・形式・ビットレート・
/// サンプルレート・ビット深度・チャンネル数・音量の情報は音声ファイルから。大きさは、変わったかを
/// 見るため、音声ファイル全体の大きさにする（`file_state`と同じ）。
pub fn build_track(virtual_path: &str) -> AppResult<crate::models::Track> {
    let source = read_track(virtual_path)?;
    let info = crate::metadata::extract_all_file_info(&source.audio_path)?;
    let file_size = crate::library::get_file_size(&source.audio_path)?;
    let file_frames = info
        .duration
        .map(|seconds| u64::try_from(seconds).unwrap_or(0) * FRAMES_PER_SECOND);
    let end_frames = source.end.or(file_frames);
    let duration = end_frames.map(|end| {
        i32::try_from(end.saturating_sub(source.start) / FRAMES_PER_SECOND).unwrap_or(i32::MAX)
    });
    let year = source
        .sheet
        .date
        .as_deref()
        .and_then(|date| date.get(..4))
        .and_then(|year| year.parse().ok())
        .or(info.metadata.year);
    let now = crate::datetime::now_rfc3339();
    let file_name = Path::new(virtual_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(crate::models::Track {
        id: uuid::Uuid::new_v4().to_string(),
        file_path: virtual_path.to_string(),
        file_name,
        title: Some(
            source
                .track
                .title
                .clone()
                .unwrap_or_else(|| format!("Track {:02}", source.track.number)),
        ),
        artist: source
            .track
            .performer
            .clone()
            .or_else(|| source.sheet.performer.clone())
            .or(info.metadata.artist),
        album: source.sheet.title.clone().or(info.metadata.album),
        album_artist: source
            .sheet
            .performer
            .clone()
            .or(info.metadata.album_artist),
        genre: source.sheet.genre.clone().or(info.metadata.genre),
        year,
        track_number: i32::try_from(source.track.number).ok(),
        disc_number: source.sheet.disc_number.or(info.metadata.disc_number),
        duration,
        file_size,
        format: crate::library::get_file_format(&source.audio_path),
        bitrate: info.bitrate,
        sample_rate: info.sample_rate,
        is_favorite: false,
        rating: 0,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        created_at: now.clone(),
        updated_at: now,
        replay_gain: info.replay_gain,
        sort_tags: Default::default(),
        is_missing: false,
        compilation: false,
        details: crate::models::TrackDetails {
            composer: source.track.songwriter.clone(),
            bit_depth: info.details.bit_depth,
            channels: info.details.channels,
            ..Default::default()
        },
        cue: Some(crate::models::TrackCue {
            source: source.audio_path.to_string_lossy().into_owned(),
            start: i64::try_from(source.start).unwrap_or(i64::MAX),
            end: source.end.and_then(|end| i64::try_from(end).ok()),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;

    const SHEET: &str = r#"REM GENRE "Jazz"
REM DATE 1959
REM DISCNUMBER 2
PERFORMER "Miles Davis"
TITLE "Kind of Blue"
FILE "Kind of Blue.wav" WAVE
  TRACK 01 AUDIO
    TITLE "So What"
    PERFORMER "Miles Davis"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Freddie Freeloader"
    SONGWRITER "Miles Davis"
    INDEX 00 09:20:50
    INDEX 01 09:22:10
  TRACK 03 AUDIO
    TITLE "Blue in Green"
    INDEX 01 19:00:00
"#;

    #[test]
    fn test_parse_reads_album_and_tracks() {
        let sheet = parse(SHEET);
        assert_eq!(sheet.performer.as_deref(), Some("Miles Davis"));
        assert_eq!(sheet.title.as_deref(), Some("Kind of Blue"));
        assert_eq!(sheet.genre.as_deref(), Some("Jazz"));
        assert_eq!(sheet.date.as_deref(), Some("1959"));
        assert_eq!(sheet.disc_number, Some(2));
        assert_eq!(sheet.files.len(), 1);
        let file = &sheet.files[0];
        assert_eq!(file.name, "Kind of Blue.wav");
        assert_eq!(file.tracks.len(), 3);
        assert_eq!(file.tracks[1].title.as_deref(), Some("Freddie Freeloader"));
        assert_eq!(file.tracks[1].songwriter.as_deref(), Some("Miles Davis"));
        // INDEX 00（前の曲との間）ではなく、INDEX 01から
        assert_eq!(file.tracks[1].start, Some((9 * 60 + 22) * 75 + 10));
        assert_eq!(file.tracks[2].start, Some(19 * 60 * 75));
    }

    #[test]
    fn test_parse_tolerates_unquoted_and_broken_lines() {
        let sheet = parse(
            "FILE album.flac WAVE\r\nTRACK 1 AUDIO\r\nINDEX 01 0:00:00\r\nTRACK x AUDIO\r\nINDEX 01 bad\r\nTRACK 2 AUDIO\r\nINDEX 01 01:00:74\r\n",
        );
        assert_eq!(sheet.files[0].name, "album.flac");
        assert_eq!(
            sheet.files[0]
                .tracks
                .iter()
                .map(|track| (track.number, track.start))
                .collect::<Vec<_>>(),
            [(1, Some(0)), (2, Some(60 * 75 + 74))]
        );
        assert_eq!(position("00:60:00"), None);
        assert_eq!(position("00:00:75"), None);
    }

    #[test]
    fn test_virtual_paths() {
        let path = virtual_path(Path::new("/Music/A/Album.cue"), 3);
        assert_eq!(path, "/Music/A/Album.cue#03");
        assert_eq!(
            parse_virtual(&path),
            Some((PathBuf::from("/Music/A/Album.cue"), 3))
        );
        assert!(is_virtual("/Music/A/Album.CUE#12"));
        assert!(!is_virtual("/Music/A/Song #1.mp3"));
        assert!(!is_virtual("/Music/A/Album.cue#"));
        assert!(!is_virtual("/Music/A/Album.cue#1a"));
    }

    #[test]
    fn test_expand_replaces_covered_audio_with_virtual_tracks() {
        let dir = TempDir::new();
        let album = dir.path().join("album");
        fs::create_dir_all(&album).unwrap();
        // CUEシートは.wavと書いているが、.flacに変えてある（名前で対応させる）
        fs::write(album.join("Kind of Blue.cue"), SHEET).unwrap();
        let audio = album.join("Kind of Blue.flac");
        fs::write(&audio, b"x").unwrap();
        let other = dir.path().join("single.mp3");
        fs::write(&other, b"x").unwrap();
        // 曲が1つのCUEシートは、置き換えない
        fs::write(
            dir.path().join("single.cue"),
            "FILE \"single.mp3\" MP3\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n",
        )
        .unwrap();

        let expanded = expand(
            vec![audio.clone(), other.clone()],
            &[
                album.join("Kind of Blue.cue"),
                dir.path().join("single.cue"),
            ],
        );
        let cue = album.join("Kind of Blue.cue");
        assert_eq!(
            expanded,
            [
                PathBuf::from(virtual_path(&cue, 1)),
                PathBuf::from(virtual_path(&cue, 2)),
                PathBuf::from(virtual_path(&cue, 3)),
                other,
            ]
        );

        let source = read_track(&virtual_path(&cue, 2)).unwrap();
        assert_eq!(source.audio_path, audio);
        assert_eq!(source.track.title.as_deref(), Some("Freddie Freeloader"));
        assert_eq!(source.start, (9 * 60 + 22) * 75 + 10);
        assert_eq!(source.end, Some(19 * 60 * 75));
        assert_eq!(read_track(&virtual_path(&cue, 3)).unwrap().end, None);
        assert!(read_track(&virtual_path(&cue, 9)).is_err());
        assert!(file_state(&virtual_path(&cue, 1)).is_some());
    }

    #[test]
    fn test_read_decodes_shift_jis() {
        let dir = TempDir::new();
        let path = dir.path().join("album.cue");
        let (bytes, _, _) =
            encoding_rs::SHIFT_JIS.encode("TITLE \"夜想曲\"\nFILE \"a.flac\" WAVE\n");
        fs::write(&path, bytes).unwrap();
        assert_eq!(read(&path).unwrap().title.as_deref(), Some("夜想曲"));
    }
}
