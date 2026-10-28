//! ファイルの整理（タグに基づくリネーム・移動）
//!
//! 選んだ曲のファイルを、書式（`%albumartist%/%album%/%track% %title%`など）に従って、その曲のある
//! ライブラリフォルダの中で移す（ADR-052）か、音楽フォルダへ集める（ADR-070）。計画（`plan`）と
//! 実行（`move_files`・`copy_for_move`・`tidy_folders`）を分け、画面は計画を一覧（今の場所 →
//! 整理後の場所）で見せてから実行する。
//!
//! - 移す先は、曲のあるライブラリフォルダの中（フォルダの外の曲は、動かさない）か、音楽フォルダ
//!   （どこにある曲も、音楽フォルダの書式どおりの場所へ移す）
//! - 移動先に同じ名前のファイルがある曲・選んだ曲の中で同じ移動先になる曲は、動かさない
//!   （上書きしない・連番も付けない）
//! - ボリュームをまたぐ曲（音楽フォルダへ集める時）は、コピーして大きさを確かめ、データベースを
//!   書き換えてから、元をゴミ箱へ移す（`copy_for_move` → `discard_originals`）
//! - 曲と同じ名前の歌詞ファイル（`.lrc`）は、曲と一緒に移す
//! - フォルダの画像（`cover.jpg`など）は、フォルダの曲がすべて同じフォルダへ移った場合に、一緒に移す
//! - 曲がなくなって空になったフォルダは、消す（ライブラリフォルダ自体と、ライブラリフォルダの外の
//!   フォルダは消さない）
//!
//! トラックのID・お気に入り・再生回数・プレイリストへの登録は、変えない（データベースの場所だけを
//! 書き換える。呼び出し側が`repository::relink_track`で行う）。

use crate::album_art::is_folder_image;
use crate::device_manifest::create_temp_file;
use crate::device_sync::{
    UNKNOWN_ALBUM, UNKNOWN_ARTIST, UNKNOWN_TITLE, path_key, sanitize_component,
};
use crate::error::{AppError, AppResult};
use crate::library::is_supported_audio_file;
use crate::models::Track;
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// 書式の長さの上限（文字数）
pub const MAX_TEMPLATE_LENGTH: usize = 255;

/// ジャンルのタグがない曲を置くフォルダの名前（アーティスト・アルバムと同じく、言語によらず固定）
const UNKNOWN_GENRE: &str = "Unknown Genre";

/// 歌詞ファイルの拡張子（大文字と小文字は区別しない）
pub(crate) const LYRICS_EXTENSION: &str = "lrc";

/// コピー中の一時ファイルの接尾辞
const TEMP_SUFFIX: &str = ".part";

/// 中身がこれらだけのフォルダは、空とみなして消す（OSが作る、表示用のファイル）
const JUNK_FILE_NAMES: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

/// 書式に書ける項目
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Genre,
    Year,
    Track,
    Disc,
    /// 今のファイル名（拡張子を除く）
    FileName,
}

impl Field {
    /// `%名前%`の名前から（大文字と小文字は区別しない）
    fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "title" => Some(Self::Title),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "albumartist" => Some(Self::AlbumArtist),
            "genre" => Some(Self::Genre),
            "year" => Some(Self::Year),
            "track" => Some(Self::Track),
            "disc" => Some(Self::Disc),
            "filename" => Some(Self::FileName),
            _ => None,
        }
    }

    /// 曲の値（値がなければNone）
    ///
    /// - タイトルのタグがない曲は、今のファイル名を使う
    /// - アルバムアーティストのタグがない曲は、アーティストを使う
    /// - トラック番号は2桁にそろえる（`01`）
    fn value(self, track: &Track, stem: &str) -> Option<String> {
        let text = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let positive = |value: Option<i32>| value.filter(|n| *n > 0);
        let stem = || Some(stem.trim().to_string()).filter(|s| !s.is_empty());
        match self {
            Self::Title => text(&track.title).or_else(stem),
            Self::Artist => text(&track.artist),
            Self::Album => text(&track.album),
            Self::AlbumArtist => text(&track.album_artist).or_else(|| text(&track.artist)),
            Self::Genre => text(&track.genre),
            Self::Year => positive(track.year).map(|year| year.to_string()),
            Self::Track => positive(track.track_number).map(|number| format!("{:02}", number)),
            Self::Disc => positive(track.disc_number).map(|number| number.to_string()),
            Self::FileName => stem(),
        }
    }

    /// 値がない時に書く文字列（番号・年は、何も書かない）
    fn fallback(self) -> &'static str {
        match self {
            Self::Title => UNKNOWN_TITLE,
            Self::Artist | Self::AlbumArtist => UNKNOWN_ARTIST,
            Self::Album => UNKNOWN_ALBUM,
            Self::Genre => UNKNOWN_GENRE,
            Self::Year | Self::Track | Self::Disc | Self::FileName => "",
        }
    }
}

/// 書式の部品
#[derive(Debug, Clone, PartialEq)]
enum Part {
    /// そのまま書く文字
    Text(String),
    Field(Field),
    /// `[ ]`で囲んだ部分（中の項目のどれにも値がなければ、まるごと省く）
    Optional(Vec<Part>),
}

impl Part {
    fn has_field(&self) -> bool {
        match self {
            Self::Text(_) => false,
            Self::Field(_) => true,
            Self::Optional(parts) => parts.iter().any(Self::has_field),
        }
    }
}

/// 解釈した書式
///
/// - `%名前%`が項目（`title`・`artist`・`album`・`albumartist`・`genre`・`year`・`track`・`disc`・
///   `filename`）で、ほかの文字はそのまま書く。`%%`は`%`
/// - `/`で区切ると、フォルダの階層になる。最後の区切りより後ろが、ファイル名（拡張子は、元のまま）
/// - `[ ]`で囲んだ部分は、中の項目のどれにも値がない曲では省く（`[%disc%-]%track% %title%`）
/// - `/`を含む書式は、ライブラリフォルダからの場所。含まない書式は、今のフォルダのまま、
///   ファイル名だけを変える
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// フォルダの階層（上から）
    folders: Vec<Vec<Part>>,
    file_name: Vec<Part>,
    /// 今のフォルダのまま、ファイル名だけを変えるか
    in_place: bool,
}

impl Template {
    /// 書式を解釈する（誤りは、入力の誤りとして返す）
    pub fn parse(text: &str) -> AppResult<Self> {
        let invalid = |message: String| Err(AppError::Validation(message));
        let text = text.trim();
        if text.is_empty() {
            return invalid("書式を入力してください".to_string());
        }
        if text.chars().count() > MAX_TEMPLATE_LENGTH {
            return invalid(format!(
                "書式が長すぎます（{}文字以内）",
                MAX_TEMPLATE_LENGTH
            ));
        }

        let is_separator = |c: char| c == '/' || c == '\\';
        let mut segments: Vec<&str> = text.split(is_separator).collect();
        let in_place = segments.len() == 1;
        let file_name = segments.pop().unwrap_or_default();
        if file_name.trim().is_empty() {
            return invalid("最後の「/」の後ろに、ファイル名の書式を入れてください".to_string());
        }
        let file_name = parse_segment(file_name)?;
        if !file_name.iter().any(Part::has_field) {
            return invalid(
                "ファイル名（最後の「/」より後ろ）に、項目（%title%など）を入れてください"
                    .to_string(),
            );
        }
        // 先頭の`/`・続いた`/`でできる、空の階層は除く
        let folders = segments
            .into_iter()
            .filter(|segment| !segment.trim().is_empty())
            .map(parse_segment)
            .collect::<AppResult<Vec<_>>>()?;

        Ok(Self {
            folders,
            file_name,
            in_place,
        })
    }

    /// 曲の、整理後の場所（基準のフォルダからの、フォルダ名 … ファイル名）
    ///
    /// フォルダ名・ファイル名は、ファイル名に使える形にする（`sanitize_component`）。値がなくて
    /// 空になった階層は省き、ファイル名が空になった曲は、今のファイル名のままにする。
    pub fn components(&self, track: &Track, from: &Path) -> Vec<String> {
        self.components_with_extension(track, from, from.extension().and_then(|e| e.to_str()))
    }

    /// 曲の、書式に従った場所（拡張子を、指定したものにする。形式の変換の、書き出す場所に使う）
    pub fn components_with_extension(
        &self,
        track: &Track,
        from: &Path,
        extension: Option<&str>,
    ) -> Vec<String> {
        let stem = from.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let mut components: Vec<String> = self
            .folders
            .iter()
            .map(|parts| render(parts, track, stem))
            .filter(|folder| !folder.trim().is_empty())
            .map(|folder| sanitize_component(&folder))
            .collect();

        let rendered = render(&self.file_name, track, stem);
        let name = sanitize_component(if rendered.trim().is_empty() {
            stem
        } else {
            &rendered
        });
        components.push(match extension {
            Some(extension) => format!("{}.{}", name, extension),
            None => name,
        });
        components
    }
}

/// 書式の1階層（フォルダ名か、ファイル名）を解釈する
fn parse_segment(segment: &str) -> AppResult<Vec<Part>> {
    let invalid = |message: String| Err(AppError::Validation(message));
    let mut parts: Vec<Part> = Vec::new();
    // `[ ]`の中を解釈している間は、中の部品をここに集める
    let mut optional: Option<Vec<Part>> = None;
    let mut chars = segment.chars();

    while let Some(c) = chars.next() {
        let part = match c {
            '%' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('%') => break,
                        Some(c) => name.push(c),
                        None => return invalid("「%」が閉じられていません".to_string()),
                    }
                }
                if name.is_empty() {
                    Part::Text("%".to_string())
                } else {
                    match Field::from_name(&name) {
                        Some(field) => Part::Field(field),
                        None => return invalid(format!("使えない項目です: %{}%", name)),
                    }
                }
            }
            '[' => {
                if optional.is_some() {
                    return invalid("「[」の中に「[」は書けません".to_string());
                }
                optional = Some(Vec::new());
                continue;
            }
            ']' => match optional.take() {
                Some(inner) => Part::Optional(inner),
                None => return invalid("「]」に対応する「[」がありません".to_string()),
            },
            c => Part::Text(c.to_string()),
        };
        let target = optional.as_mut().unwrap_or(&mut parts);
        // 続いた文字は、1つにまとめる
        match (target.last_mut(), part) {
            (Some(Part::Text(text)), Part::Text(more)) => text.push_str(&more),
            (_, part) => target.push(part),
        }
    }

    if optional.is_some() {
        return invalid("「[」が閉じられていません".to_string());
    }
    Ok(parts)
}

/// 部品を、曲の値で文字列にする
fn render(parts: &[Part], track: &Track, stem: &str) -> String {
    let mut rendered = String::new();
    for part in parts {
        match part {
            Part::Text(text) => rendered.push_str(text),
            Part::Field(field) => match field.value(track, stem) {
                Some(value) => rendered.push_str(&value),
                None => rendered.push_str(field.fallback()),
            },
            Part::Optional(inner) => {
                let has_value = inner.iter().any(|part| match part {
                    Part::Field(field) => field.value(track, stem).is_some(),
                    _ => false,
                });
                if has_value {
                    rendered.push_str(&render(inner, track, stem));
                }
            }
        }
    }
    rendered
}

/// 曲を動かすか、動かさない理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OrganizeStatus {
    /// 動かす
    Move,
    /// すでに、書式どおりの場所にある
    Unchanged,
    /// ライブラリフォルダの外にある
    OutsideLibrary,
    /// ファイルが見つからない
    Missing,
    /// 移動先に、同じ名前のファイルがある
    TargetExists,
    /// 移動先の場所を、ライブラリの曲（ファイルが見つからない曲）が使っている（ファイルはない）
    TargetRegistered,
    /// 選んだ曲の中に、同じ移動先になる曲がある
    SameTarget,
}

/// 1曲の計画（一覧に表示する）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeEntry {
    pub track_id: String,
    /// 今の場所（ライブラリフォルダからの場所。フォルダの外の曲は、ファイルのパス）
    pub current_path: String,
    /// 整理後の場所（ライブラリフォルダからの場所。決まらない曲はNone）
    pub new_path: Option<String>,
    pub status: OrganizeStatus,
}

/// ファイルの整理の計画（選んだ曲の順）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub entries: Vec<OrganizeEntry>,
    /// ボリュームをまたぐため、コピーしてから元をゴミ箱へ移す曲の数と、要る容量（バイト。歌詞ファイルを
    /// 含む）。音楽フォルダへ集める時だけ
    pub copy_count: u32,
    #[specta(type = specta_typescript::Number)]
    pub copy_bytes: i64,
    /// 音楽フォルダのドライブの空き容量（バイト。音楽フォルダへ集める時だけ。取得できなければnull）
    #[specta(type = Option<specta_typescript::Number>)]
    pub free_bytes: Option<i64>,
}

/// 1曲の移動（実行用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMove {
    pub track_id: String,
    pub from: PathBuf,
    pub to: PathBuf,
    /// 一緒に移す歌詞ファイル（今の場所, 移動先）
    pub lyrics: Option<(PathBuf, PathBuf)>,
    /// 曲のあるライブラリフォルダ（空になったフォルダは、この手前まで消す。ライブラリフォルダの外の曲は
    /// None で、フォルダを消さない）
    pub library_folder: Option<PathBuf>,
    /// ボリュームをまたぐため、コピーしてから元をゴミ箱へ移すか（音楽フォルダへ集める時）
    pub copy: bool,
}

impl PlannedMove {
    /// コピーに要る容量（曲と、歌詞ファイルの大きさ。バイト）
    pub fn size(&self) -> u64 {
        let size = |path: &Path| fs::metadata(path).map_or(0, |metadata| metadata.len());
        size(&self.from) + self.lyrics.as_ref().map_or(0, |(from, _)| size(from))
    }
}

/// 計画（表示用の一覧と、実行する移動）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub entries: Vec<OrganizeEntry>,
    pub moves: Vec<PlannedMove>,
}

/// フォルダの中身（計画の間だけ覚え、曲ごとにフォルダを読み直さない）
#[derive(Default)]
pub(crate) struct DirCache {
    entries: HashMap<PathBuf, Vec<PathBuf>>,
}

impl DirCache {
    fn list(&mut self, dir: &Path) -> &[PathBuf] {
        self.entries
            .entry(dir.to_path_buf())
            .or_insert_with(|| list_dir(dir).unwrap_or_default())
    }

    /// 曲と同じフォルダにある、同じ名前の歌詞ファイル（`lyrics::read_track_lyrics`が読むもの）
    pub(crate) fn lyrics_file(&mut self, track_path: &Path) -> Option<PathBuf> {
        let stem = track_path.file_stem()?;
        self.list(track_path.parent()?)
            .iter()
            .find(|path| {
                path.file_stem() == Some(stem)
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case(LYRICS_EXTENSION))
                    && path.is_file()
            })
            .cloned()
    }
}

pub(crate) fn list_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect())
}

/// 文字の形（NFC・NFD）の違いを除いたパス
fn nfc(path: &Path) -> String {
    path.to_string_lossy().nfc().collect()
}

/// 大文字と小文字・文字の形を区別しないファイルシステムで、同じ場所かを比べるためのキー
pub(crate) fn location_key(path: &Path) -> String {
    path_key(&nfc(path))
}

/// 2つのパスが、同じファイルを指しているか（大文字と小文字・文字の形だけが違う名前）
///
/// 大文字と小文字を区別しないファイルシステムでは、名前の大文字と小文字だけを変える場合に、
/// 移動先が「すでにある」ように見える。その場合は、移動先がふさがっているとみなさない。
fn is_same_entry(a: &Path, b: &Path) -> bool {
    location_key(a) == location_key(b) && is_same_inode(a, b)
}

#[cfg(unix)]
fn is_same_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn is_same_inode(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// 移動先がふさがっているか（何かがあり、動かすファイル自身ではない）
fn is_occupied(to: &Path, from: &Path) -> bool {
    fs::symlink_metadata(to).is_ok() && !is_same_entry(from, to)
}

/// 基準のフォルダからの場所（表示用）
fn display_path(path: &Path, base: Option<&Path>) -> String {
    base.and_then(|base| path.strip_prefix(base).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// 動かす候補（移動先がふさがっていないかを、まとめて調べる）
struct Candidate {
    /// `entries`の中の位置
    index: usize,
    planned: PlannedMove,
}

impl Candidate {
    /// この曲が使う移動先（曲と、歌詞ファイル）
    fn targets(&self) -> impl Iterator<Item = (&Path, &Path)> {
        let lyrics = self
            .planned
            .lyrics
            .as_ref()
            .map(|(from, to)| (from.as_path(), to.as_path()));
        std::iter::once((self.planned.from.as_path(), self.planned.to.as_path())).chain(lyrics)
    }
}

/// 2つの場所が、同じボリュームにあるか（調べられない場合は、違うとみなす）
#[cfg(unix)]
pub(crate) fn is_same_volume(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev(),
        _ => false,
    }
}

/// 2つの場所が、同じボリュームにあるか（Unix以外では調べず、いつもコピーで移す）
#[cfg(not(unix))]
pub(crate) fn is_same_volume(_a: &Path, _b: &Path) -> bool {
    false
}

/// 選んだ曲の、ファイルの整理の計画を立てる
///
/// @param tracks - 選んだ曲（この順に、一覧にする）
/// @param library_folders - ライブラリフォルダのパス
/// @param registered_paths - ライブラリに登録されている、すべての曲のパス（見つからない曲を含む。
///   ファイルがなくても、その場所は移動先にしない）
/// @param music_folder - 音楽フォルダへ集める時は、音楽フォルダ（どの曲も、音楽フォルダからの書式どおりの
///   場所へ移す。書式に`/`がなくても、音楽フォルダの直下に置く）。Noneなら、曲のあるライブラリフォルダの中で
///   移す
pub fn plan(
    tracks: &[Track],
    library_folders: &[PathBuf],
    template: &Template,
    registered_paths: &HashSet<String>,
    music_folder: Option<&Path>,
) -> Plan {
    let mut dirs = DirCache::default();
    let mut entries: Vec<OrganizeEntry> = Vec::with_capacity(tracks.len());
    let mut candidates: Vec<Candidate> = Vec::new();

    for track in tracks {
        let from = PathBuf::from(&track.file_path);
        let folder = library_folders
            .iter()
            .find(|folder| from.starts_with(folder) && from != **folder);
        let mut entry = OrganizeEntry {
            track_id: track.id.clone(),
            current_path: display_path(&from, folder.map(PathBuf::as_path)),
            new_path: None,
            status: OrganizeStatus::Move,
        };

        // 移す先の基準のフォルダ（音楽フォルダか、曲のあるライブラリフォルダ）
        let Some(base_folder) = music_folder.or(folder.map(PathBuf::as_path)) else {
            entry.status = OrganizeStatus::OutsideLibrary;
            entries.push(entry);
            continue;
        };
        if track.is_missing || !from.is_file() {
            entry.status = OrganizeStatus::Missing;
            entries.push(entry);
            continue;
        }

        let base = match from.parent() {
            Some(parent) if template.in_place && music_folder.is_none() => parent,
            _ => base_folder,
        };
        let to = template
            .components(track, &from)
            .iter()
            .fold(base.to_path_buf(), |path, component| path.join(component));
        entry.new_path = Some(display_path(&to, Some(base_folder)));

        // 文字の形（NFC・NFD）だけが違う名前は、同じ場所とみなす（名前を付け直さない）
        if nfc(&to) == nfc(&from) {
            entry.status = OrganizeStatus::Unchanged;
        } else {
            let lyrics = dirs.lyrics_file(&from).map(|lyrics| {
                let moved = match lyrics.extension() {
                    Some(extension) => to.with_extension(extension),
                    None => to.with_extension(LYRICS_EXTENSION),
                };
                (lyrics, moved)
            });
            candidates.push(Candidate {
                index: entries.len(),
                planned: PlannedMove {
                    track_id: track.id.clone(),
                    copy: music_folder.is_some_and(|music| !is_same_volume(&from, music)),
                    from,
                    to,
                    lyrics,
                    library_folder: folder.cloned(),
                },
            });
        }
        entries.push(entry);
    }

    // 選んだ曲の中で、同じ移動先になる曲（どれも動かさない）
    let mut users: HashMap<String, HashSet<usize>> = HashMap::new();
    for candidate in &candidates {
        for (_, to) in candidate.targets() {
            users
                .entry(location_key(to))
                .or_default()
                .insert(candidate.index);
        }
    }

    let mut moves = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let shared = candidate
            .targets()
            .any(|(_, to)| users.get(&location_key(to)).is_some_and(|u| u.len() > 1));
        let occupied = candidate.targets().any(|(from, to)| is_occupied(to, from));
        // ファイルはないが、ライブラリの曲（見つからない曲）の場所（その曲が見つかった時に重なる）
        let registered = candidate
            .planned
            .to
            .to_str()
            .is_some_and(|to| registered_paths.contains(to));

        entries[candidate.index].status = if shared {
            OrganizeStatus::SameTarget
        } else if occupied {
            OrganizeStatus::TargetExists
        } else if registered {
            OrganizeStatus::TargetRegistered
        } else {
            moves.push(candidate.planned);
            OrganizeStatus::Move
        };
    }

    Plan { entries, moves }
}

/// ファイルを移す（移動先に何かあれば、上書きせずに失敗する）
///
/// `fs::rename`は、移動先にファイルがあると上書きする。計画を立ててから移すまでの間に、移動先へ
/// ファイルが置かれても上書きしないよう、ハードリンクを作ってから元を消す（ハードリンクは、
/// 移動先があると失敗する）。ハードリンクを作れないファイルシステム（FAT32・exFATなど）では、
/// 移動先がないことを確かめてから移す。
pub(crate) fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    if is_same_entry(from, to) {
        // 大文字と小文字だけを変える（同じファイルのため、上書きにならない）
        return fs::rename(from, to);
    }
    match fs::hard_link(from, to) {
        Ok(()) => fs::remove_file(from).inspect_err(|_| {
            // 元を消せなかった場合は、作ったリンクを消して、移す前に戻す
            fs::remove_file(to).ok();
        }),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => {
            if fs::symlink_metadata(to).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "移動先に同じ名前のファイルがあります",
                ));
            }
            fs::rename(from, to)
        }
    }
}

/// ファイルをコピーする（コピー先に何かあれば、上書きせずに失敗する）
///
/// 別名（`.part`）に書いてから、`move_no_replace`で置く（途中で止まっても、書きかけのファイルを
/// 曲として残さない。書いている間にコピー先へファイルが置かれても、上書きしない）。一時ファイルの
/// 名前にリンクが置かれていても、たどらない（`create_temp_file`）。`std::fs::copy`は使わない
/// （macOSでは拡張属性（ダウンロードしたファイルの検疫の印など）もコピーするため。デバイスへの転送と
/// 同じ）。更新日時は、元のファイルに合わせる。
/// @returns コピーしたバイト数
pub(crate) fn copy_no_replace(from: &Path, to: &Path, buffer: &mut [u8]) -> io::Result<u64> {
    let mut name = to.file_name().unwrap_or_default().to_os_string();
    name.push(TEMP_SUFFIX);
    let temp = to.with_file_name(name);

    let written = (|| {
        let mut reader = fs::File::open(from)?;
        let mut writer = create_temp_file(&temp)?;
        let mut written: u64 = 0;
        loop {
            let read = reader.read(buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
            written += read as u64;
        }
        if let Ok(modified) = reader.metadata().and_then(|metadata| metadata.modified()) {
            writer.set_modified(modified).ok();
        }
        Ok(written)
    })()
    .and_then(|written| move_no_replace(&temp, to).map(|()| written));
    if written.is_err() {
        fs::remove_file(&temp).ok();
    }
    written
}

/// 移動先・コピー先のフォルダを作る（なければ）
///
/// @returns 作る前からあった、いちばん近いフォルダ（移せなかった時に、作ったフォルダだけを片付ける
///   `remove_created_folders`の上限）
pub(crate) fn create_target_folder(dir: &Path) -> io::Result<PathBuf> {
    let mut existing = dir;
    while !existing.is_dir() {
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }
    let existing = existing.to_path_buf();
    fs::create_dir_all(dir)?;
    Ok(existing)
}

/// 移せなかった時に、`create_target_folder`で作ったフォルダのうち、空のものを片付ける
pub(crate) fn remove_created_folders(dir: &Path, existing: &Path) {
    remove_empty_folders(dir, existing);
}

/// 1曲のファイル（と、同じ名前の歌詞ファイル）を移す
///
/// 移動先のフォルダは、なければ作る（移せなかった場合は、作ったフォルダを片付ける）。歌詞ファイルを
/// 移せなかった場合は、曲は移したままにして、理由を返す（曲の移動は、成功として扱う）。
/// @returns 歌詞ファイルを移せなかった場合の理由
pub fn move_files(planned: &PlannedMove) -> AppResult<Option<String>> {
    let failed = |e: io::Error| {
        AppError::Io(if e.kind() == io::ErrorKind::AlreadyExists {
            "移動先に同じ名前のファイルがあります".to_string()
        } else {
            format!("ファイルを移せませんでした: {}", e)
        })
    };
    let created = match planned.to.parent() {
        Some(parent) => Some((
            parent,
            create_target_folder(parent)
                .map_err(|e| AppError::Io(format!("移動先のフォルダを作れませんでした: {}", e)))?,
        )),
        None => None,
    };
    if let Err(e) = move_no_replace(&planned.from, &planned.to) {
        if let Some((parent, existing)) = &created {
            remove_created_folders(parent, existing);
        }
        return Err(failed(e));
    }
    // 移した曲は、この後でDBの場所を書き換える。ライブラリフォルダの監視が、この移動で再スキャン
    // しないように（`own_writes`）
    crate::own_writes::record(&planned.from);
    crate::own_writes::record(&planned.to);

    Ok(planned.lyrics.as_ref().and_then(|(from, to)| {
        move_no_replace(from, to)
            .err()
            .map(|e| format!("歌詞ファイルを移せませんでした: {}", e))
    }))
}

/// 移したファイルを、元の場所へ戻す（データベースを書き換えられなかった時に使う）
pub fn undo_move(planned: &PlannedMove) {
    let restore = |from: &Path, to: &Path| {
        if let Err(e) = move_no_replace(to, from) {
            log::error!(
                "Failed to move file back to its original location: {} -> {}: {}",
                to.display(),
                from.display(),
                e
            );
        }
    };
    if let Some((from, to)) = &planned.lyrics
        && to.exists()
    {
        restore(from, to);
    }
    restore(&planned.from, &planned.to);
}

/// 移した元のファイルを消す関数（普段は`library::discard_track_file`。ゴミ箱の中の場所を返す）
pub type Discard<'a> = dyn Fn(&Path) -> AppResult<Option<PathBuf>> + 'a;

/// ボリュームをまたいで1曲を移す前半: 曲（と、同じ名前の歌詞ファイル）をコピーし、大きさを確かめる
///
/// 元は、まだ消さない（データベースを書き換えてから、`discard_originals`で消す）。移動先のフォルダは、
/// なければ作る。曲のコピーが元と同じ大きさでなければ、コピーを消してエラーにする（どちらの失敗でも、
/// 作ったフォルダは片付ける）。
pub fn copy_for_move(planned: &PlannedMove, buffer: &mut [u8]) -> AppResult<CopiedMove> {
    let created = match planned.to.parent() {
        Some(parent) => Some((
            parent,
            create_target_folder(parent)
                .map_err(|e| AppError::Io(format!("移動先のフォルダを作れませんでした: {}", e)))?,
        )),
        None => None,
    };
    let cleanup = || {
        if let Some((parent, existing)) = &created {
            remove_created_folders(parent, existing);
        }
    };
    let written = copy_no_replace(&planned.from, &planned.to, buffer).map_err(|e| {
        cleanup();
        AppError::Io(if e.kind() == io::ErrorKind::AlreadyExists {
            "移動先に同じ名前のファイルがあります".to_string()
        } else {
            format!("ファイルをコピーできませんでした: {}", e)
        })
    })?;
    let source_size = fs::metadata(&planned.from).map(|metadata| metadata.len());
    let copied_size = fs::metadata(&planned.to).map(|metadata| metadata.len());
    if source_size.ok() != Some(written) || copied_size.ok() != Some(written) {
        fs::remove_file(&planned.to).ok();
        cleanup();
        return Err(AppError::Io(
            "コピーしたファイルの大きさが、元と違います".to_string(),
        ));
    }
    // コピーした曲は、この後でDBの場所を書き換える（元は`discard_originals`で消す）
    crate::own_writes::record(&planned.to);

    let lyrics = planned
        .lyrics
        .as_ref()
        .map(|(from, to)| copy_no_replace(from, to, buffer));
    Ok(CopiedMove {
        lyrics_copied: matches!(lyrics, Some(Ok(_))),
        warning: match lyrics {
            Some(Err(e)) => Some(format!("歌詞ファイルをコピーできませんでした: {}", e)),
            _ => None,
        },
    })
}

/// ボリュームをまたいで1曲をコピーした結果
#[derive(Debug, PartialEq, Eq)]
pub struct CopiedMove {
    /// 歌詞ファイルもコピーしたか（ない場合はfalse）
    pub lyrics_copied: bool,
    /// 歌詞ファイルをコピーできなかった場合の理由（曲は、移すものとして扱う）
    pub warning: Option<String>,
}

/// コピーしたファイルを消す（データベースを書き換えられなかった時に使う。元のファイルは、そのまま）
pub fn remove_copied(planned: &PlannedMove, copied: &CopiedMove) {
    let remove = |path: &Path| {
        if let Err(e) = fs::remove_file(path) {
            log::error!("Failed to delete copied file: {}: {}", path.display(), e);
        }
    };
    remove(&planned.to);
    if let Some((_, to)) = planned.lyrics.as_ref().filter(|_| copied.lyrics_copied) {
        remove(to);
    }
}

/// ボリュームをまたいで1曲を移す後半: 元の曲（と、コピーした歌詞ファイルの元）を消す
///
/// `discard`は、普段は`library::discard_track_file`（macOSではゴミ箱へ移し、ほかのOSでは削除する）。
/// @returns 消せなかった場合の理由（曲はコピー先へ移ったものとして扱う。元は残る）
pub fn discard_originals(
    planned: &PlannedMove,
    copied: &CopiedMove,
    discard: &Discard<'_>,
) -> Option<String> {
    let lyrics = planned
        .lyrics
        .as_ref()
        .filter(|_| copied.lyrics_copied)
        .map(|(from, _)| from.as_path());
    let failures: Vec<String> = std::iter::once(planned.from.as_path())
        .chain(lyrics)
        .filter_map(|path| {
            discard(path)
                .err()
                .map(|e| format!("{}（{}）", e, path.display()))
        })
        .collect();
    (!failures.is_empty()).then(|| {
        format!(
            "元のファイルを消せませんでした（コピー先へ移した曲として扱います）: {}",
            failures.join("、")
        )
    })
}

/// 曲を移した後の片付け: フォルダの画像を移し、空になったフォルダを消す
///
/// フォルダの画像（`cover.jpg`など。埋め込みの画像がない曲のアルバムアートに使う）は、元のフォルダに
/// 曲が残らず、移した曲の移動先が1つのフォルダで、そのフォルダへ移ってきた曲がすべて同じフォルダから
/// 来た場合に、一緒に移す（アルバムのフォルダを、そのまま移した場合）。移動先に同じ名前の画像が
/// あれば、移さない。ボリュームをまたいで移した曲のフォルダの画像は、コピーしてから元を`discard`で消す。
/// @param moved - 移した曲
pub fn tidy_folders(moved: &[PlannedMove], discard: &Discard<'_>) {
    // 元のフォルダ → (移動先のフォルダ, ライブラリフォルダ, コピーで移したか)。同じフォルダの中での
    // 改名は除く
    let mut destinations: HashMap<&Path, (HashSet<&Path>, Option<&Path>, bool)> = HashMap::new();
    // 移動先のフォルダ → 元のフォルダ
    let mut sources: HashMap<&Path, HashSet<&Path>> = HashMap::new();
    for planned in moved {
        let (Some(from_dir), Some(to_dir)) = (planned.from.parent(), planned.to.parent()) else {
            continue;
        };
        if from_dir == to_dir {
            continue;
        }
        let destination = destinations.entry(from_dir).or_insert_with(|| {
            (
                HashSet::new(),
                planned.library_folder.as_deref(),
                planned.copy,
            )
        });
        destination.0.insert(to_dir);
        destination.2 |= planned.copy;
        sources.entry(to_dir).or_default().insert(from_dir);
    }

    for (from_dir, (to_dirs, library_folder, copy)) in destinations {
        let mut to_dirs = to_dirs.into_iter();
        if let (Some(to_dir), None) = (to_dirs.next(), to_dirs.next())
            && sources.get(to_dir).is_some_and(|from| from.len() == 1)
            && !has_audio_files(from_dir)
        {
            move_folder_images(from_dir, to_dir, copy.then_some(discard));
        }
        if let Some(library_folder) = library_folder {
            remove_empty_folders(from_dir, library_folder);
        }
    }
}

/// フォルダの直下に、音楽ファイルがあるか（調べられない場合は、あるとみなす）
fn has_audio_files(dir: &Path) -> bool {
    list_dir(dir).map_or(true, |paths| {
        paths.iter().any(|path| is_supported_audio_file(path))
    })
}

/// フォルダの画像を、別のフォルダへ移す（同じ名前のファイルがあれば移さない。失敗はログに残す）
///
/// @param discard - ボリュームをまたぐ場合に、コピーした後で元を消す関数（Noneなら、そのまま移す）
fn move_folder_images(from_dir: &Path, to_dir: &Path, discard: Option<&Discard<'_>>) {
    let mut buffer = Vec::new();
    for image in list_dir(from_dir).unwrap_or_default() {
        let Some(name) = image.file_name() else {
            continue;
        };
        if !is_folder_image(&image) || !image.is_file() {
            continue;
        }
        let to = to_dir.join(name);
        let moved = match discard {
            None => move_no_replace(&image, &to).map_err(|e| e.to_string()),
            Some(discard) => {
                buffer.resize(64 * 1024, 0);
                copy_no_replace(&image, &to, &mut buffer)
                    .map_err(|e| e.to_string())
                    .and_then(|_| discard(&image).map(|_| ()).map_err(|e| e.to_string()))
            }
        };
        if let Err(e) = moved {
            log::info!("Did not move folder image: {}: {}", image.display(), e);
        }
    }
}

/// 空になったフォルダを、上へさかのぼって消す（ライブラリフォルダ自体と、その外は消さない）
pub(crate) fn remove_empty_folders(dir: &Path, library_folder: &Path) {
    let mut current = dir;
    while current != library_folder && current.starts_with(library_folder) {
        if !remove_if_empty(current) {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
}

/// フォルダが空なら消す（OSが作る表示用のファイルだけが残っている場合も、空とみなす）
///
/// @returns 消したか
fn remove_if_empty(dir: &Path) -> bool {
    let Ok(paths) = list_dir(dir) else {
        return false;
    };
    let is_junk = |path: &PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| JUNK_FILE_NAMES.contains(&name))
            && path.is_file()
    };
    if !paths.iter().all(is_junk) {
        return false;
    }
    for path in &paths {
        fs::remove_file(path).ok();
    }
    fs::remove_dir(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::models::ReplayGain;

    /// 移せなかった・コピーできなかった時は、そのために作った空のフォルダを残さない（前からあるフォルダは残す）
    #[test]
    fn test_failed_moves_remove_the_folders_they_created() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        fs::create_dir_all(library.join("Existing")).unwrap();
        let planned = |to: PathBuf, copy: bool| PlannedMove {
            track_id: "t1".to_string(),
            // 元のファイルがない（移す前に消された）
            from: dir.path().join("gone.mp3"),
            to,
            lyrics: None,
            library_folder: Some(library.clone()),
            copy,
        };

        let to = library.join("Existing/New Artist/New Album/01 a.mp3");
        assert!(move_files(&planned(to.clone(), false)).is_err());
        assert!(!library.join("Existing/New Artist").exists());
        assert!(library.join("Existing").is_dir());

        let mut buffer = vec![0u8; 64];
        assert!(copy_for_move(&planned(to, true), &mut buffer).is_err());
        assert!(!library.join("Existing/New Artist").exists());
        assert!(library.join("Existing").is_dir());
    }

    fn track(id: &str, path: &Path) -> Track {
        Track {
            id: id.to_string(),
            file_path: path.to_str().unwrap().to_string(),
            file_name: path.file_name().unwrap().to_str().unwrap().to_string(),
            title: Some("Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: None,
            genre: None,
            year: None,
            track_number: Some(1),
            disc_number: None,
            duration: Some(200),
            file_size: 4,
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
            sort_tags: Default::default(),
            is_missing: false,
        }
    }

    /// ファイルを作る（上のフォルダも作る）
    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"data").unwrap();
    }

    fn parse(text: &str) -> Template {
        Template::parse(text).unwrap()
    }

    fn parse_error(text: &str) -> String {
        match Template::parse(text) {
            Err(AppError::Validation(message)) => message,
            other => panic!("入力の誤りになるはず: {:?}", other),
        }
    }

    /// 書式を、曲に当てはめた場所（`/`でつないだもの）
    fn rendered(template: &str, track: &Track) -> String {
        parse(template)
            .components(track, Path::new(&track.file_path))
            .join("/")
    }

    const DEFAULT: &str = "%albumartist%/%album%/%track% %title%";

    fn plan_in(library: &Path, tracks: &[Track], template: &str) -> Plan {
        plan(
            tracks,
            &[library.to_path_buf()],
            &parse(template),
            &tracks.iter().map(|t| t.file_path.clone()).collect(),
            None,
        )
    }

    fn statuses(plan: &Plan) -> Vec<OrganizeStatus> {
        plan.entries.iter().map(|entry| entry.status).collect()
    }

    #[test]
    fn test_parse_rejects_invalid_templates() {
        assert!(parse_error("  ").contains("書式を入力"));
        assert!(parse_error(&"a".repeat(MAX_TEMPLATE_LENGTH + 1)).contains("長すぎます"));
        assert!(parse_error("%composer% - %title%").contains("%composer%"));
        assert!(parse_error("%title").contains("閉じられていません"));
        assert!(parse_error("[%track% [%disc%]]%title%").contains("書けません"));
        assert!(parse_error("%track%] %title%").contains("対応する"));
        assert!(parse_error("[%track% %title%").contains("閉じられていません"));
        // ファイル名に項目がないと、どの曲も同じ名前になる
        assert!(parse_error("%album%/track").contains("項目"));
        assert!(parse_error("%artist%/%album%/").contains("ファイル名の書式"));
    }

    #[test]
    fn test_parse_reads_folders_and_file_name() {
        let template = parse(" %AlbumArtist%/%album%\\%track% %title% ");
        assert_eq!(template.folders.len(), 2);
        assert_eq!(template.folders[0], [Part::Field(Field::AlbumArtist)]);
        assert_eq!(
            template.file_name,
            [
                Part::Field(Field::Track),
                Part::Text(" ".to_string()),
                Part::Field(Field::Title)
            ]
        );
        assert!(!template.in_place);

        // `/`がなければ、今のフォルダのまま、ファイル名だけを変える
        assert!(parse("%artist% - %title%").in_place);
        // 先頭の`/`は、ライブラリフォルダの直下
        let root = parse("/%artist% - %title%");
        assert!(!root.in_place);
        assert!(root.folders.is_empty());
        // `%%`は`%`
        assert_eq!(
            parse("100%% %title%").file_name[0],
            Part::Text("100% ".to_string())
        );
    }

    #[test]
    fn test_components_use_tags_with_fallbacks() {
        let mut song = track("t1", Path::new("/music/in/original name.MP3"));
        song.album_artist = Some("Various".to_string());
        song.genre = Some("Rock; Pop".to_string());
        song.year = Some(1999);
        song.disc_number = Some(2);
        assert_eq!(rendered(DEFAULT, &song), "Various/Album/01 Song.MP3");
        assert_eq!(
            rendered("%genre%/%year%/%disc%-%track% %artist% - %filename%", &song),
            "Rock; Pop/1999/2-01 Artist - original name.MP3"
        );

        // タグのない曲: アルバムアーティストはアーティスト、タイトルは今のファイル名、番号は書かない
        let mut untagged = track("t2", Path::new("/music/in/original name.flac"));
        untagged.title = None;
        untagged.album = Some("  ".to_string());
        untagged.track_number = None;
        assert_eq!(
            rendered(DEFAULT, &untagged),
            "Artist/Unknown Album/original name.flac"
        );
        untagged.artist = None;
        assert_eq!(
            rendered("%artist%/%genre%/%title%", &untagged),
            "Unknown Artist/Unknown Genre/original name.flac"
        );
    }

    #[test]
    fn test_components_are_safe_file_names() {
        let mut song = track("t1", Path::new("/music/a.mp3"));
        song.artist = Some("AC/DC".to_string());
        song.album = Some("..".to_string());
        song.title = Some("What? .".to_string());
        // 値の中の`/`は、階層にしない。使えない文字・前後のドットと空白は、取り除く
        assert_eq!(rendered(DEFAULT, &song), "AC_DC/_/01 What_.mp3");

        // 濁点などを分けた形（NFD）は、合成した形（NFC）にする
        song.title = Some("か\u{3099}".to_string());
        assert_eq!(rendered("%title%", &song), "が.mp3");
    }

    #[test]
    fn test_optional_sections_and_empty_folders_are_omitted() {
        let template = "%albumartist%/[%year% ]%album%/[Disc %disc%]/[%disc%-][%track% ]%title%";
        let mut song = track("t1", Path::new("/music/a.mp3"));
        song.track_number = None;
        // 値のない部分と、空になった階層は省く
        assert_eq!(rendered(template, &song), "Artist/Album/Song.mp3");

        song.year = Some(2001);
        song.disc_number = Some(2);
        song.track_number = Some(7);
        assert_eq!(
            rendered(template, &song),
            "Artist/2001 Album/Disc 2/2-07 Song.mp3"
        );

        // `[ ]`の中の項目のどれかに値があれば書く（値のない項目は、何も書かないか、既定の名前）
        song.disc_number = None;
        assert_eq!(rendered("[%disc%-%track% ]%title%", &song), "-07 Song.mp3");

        // ファイル名が空になる曲は、今のファイル名のままにする
        song.track_number = None;
        assert_eq!(rendered("[%track%]%disc%", &song), "a.mp3");
    }

    #[test]
    fn test_plan_moves_tracks_inside_their_library_folder() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);

        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);

        assert_eq!(
            plan.entries,
            [OrganizeEntry {
                track_id: "t1".to_string(),
                current_path: Path::new("incoming").join("a.mp3").display().to_string(),
                new_path: Some(
                    Path::new("Artist")
                        .join("Album")
                        .join("01 Song.mp3")
                        .display()
                        .to_string()
                ),
                status: OrganizeStatus::Move,
            }]
        );
        assert_eq!(
            plan.moves,
            [PlannedMove {
                track_id: "t1".to_string(),
                from: source,
                to: library.join("Artist/Album/01 Song.mp3"),
                lyrics: None,
                library_folder: Some(library),
                copy: false,
            }]
        );
    }

    #[test]
    fn test_plan_renames_in_place_without_folders() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);

        let plan = plan_in(&library, &[track("t1", &source)], "%artist% - %title%");

        assert_eq!(plan.moves[0].to, library.join("incoming/Artist - Song.mp3"));
    }

    #[test]
    fn test_plan_skips_tracks_that_cannot_move() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let organized = library.join("Artist/Album/01 Song.mp3");
        let outside = dir.path().join("Elsewhere/b.mp3");
        let gone = library.join("incoming/gone.mp3");
        let marked = library.join("incoming/marked.mp3");
        for path in [&organized, &outside, &marked] {
            touch(path);
        }
        let mut marked_missing = track("t4", &marked);
        marked_missing.is_missing = true;
        let tracks = [
            track("t1", &organized),
            track("t2", &outside),
            track("t3", &gone),
            marked_missing,
        ];

        let plan = plan_in(&library, &tracks, DEFAULT);

        assert_eq!(
            statuses(&plan),
            [
                OrganizeStatus::Unchanged,
                OrganizeStatus::OutsideLibrary,
                OrganizeStatus::Missing,
                OrganizeStatus::Missing,
            ]
        );
        assert!(plan.moves.is_empty());
        // フォルダの外の曲は、ファイルのパスをそのまま表示する
        assert_eq!(plan.entries[1].current_path, outside.display().to_string());
        assert_eq!(plan.entries[1].new_path, None);
    }

    #[test]
    fn test_plan_does_not_overwrite_existing_files() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);
        touch(&library.join("Artist/Album/01 Song.mp3"));

        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);

        assert_eq!(statuses(&plan), [OrganizeStatus::TargetExists]);
        assert!(plan.moves.is_empty());
        assert!(plan.entries[0].new_path.is_some());
    }

    #[test]
    fn test_plan_avoids_paths_registered_for_missing_tracks() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);
        // ファイルはないが、見つからない曲として登録されている場所
        let registered = library.join("Artist/Album/01 Song.mp3");

        let plan = plan(
            &[track("t1", &source)],
            std::slice::from_ref(&library),
            &parse(DEFAULT),
            &HashSet::from([registered.to_str().unwrap().to_string()]),
            None,
        );

        // ファイルがある場合（`TargetExists`）と、理由を分ける
        assert_eq!(statuses(&plan), [OrganizeStatus::TargetRegistered]);
    }

    #[test]
    fn test_plan_does_not_move_tracks_sharing_a_target() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let first = library.join("a/one.mp3");
        let second = library.join("b/two.mp3");
        let other = library.join("b/three.mp3");
        for path in [&first, &second, &other] {
            touch(path);
        }
        let mut different = track("t3", &other);
        different.title = Some("Other".to_string());
        // 大文字と小文字だけが違う移動先も、同じ移動先とみなす
        let mut lower = track("t2", &second);
        lower.title = Some("song".to_string());

        let plan = plan_in(
            &library,
            &[track("t1", &first), lower, different],
            "%artist%/%title%",
        );

        assert_eq!(
            statuses(&plan),
            [
                OrganizeStatus::SameTarget,
                OrganizeStatus::SameTarget,
                OrganizeStatus::Move
            ]
        );
        assert_eq!(plan.moves.len(), 1);
        assert_eq!(plan.moves[0].track_id, "t3");
    }

    #[test]
    fn test_plan_allows_changing_only_the_case_of_a_name() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("artist/song.mp3");
        touch(&source);

        // 大文字と小文字を区別しないファイルシステムでは、移動先が「ある」ように見える（自分自身）
        let plan = plan_in(&library, &[track("t1", &source)], "%title%");

        assert_eq!(statuses(&plan), [OrganizeStatus::Move]);
        assert!(move_files(&plan.moves[0]).unwrap().is_none());
        let names: Vec<String> = list_dir(&library.join("artist"))
            .unwrap()
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["Song.mp3"]);
    }

    #[test]
    fn test_plan_treats_normalization_differences_as_unchanged() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        // ファイル名が、濁点を分けた形（NFD）
        let source = library.join("か\u{3099}.mp3");
        touch(&source);
        let mut song = track("t1", &source);
        song.title = Some("が".to_string());

        let plan = plan_in(&library, &[song], "%title%");

        assert_eq!(statuses(&plan), [OrganizeStatus::Unchanged]);
    }

    #[test]
    fn test_lyrics_file_moves_with_the_track() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);
        touch(&library.join("incoming/a.LRC"));
        // 名前の違う歌詞ファイルは、移さない
        touch(&library.join("incoming/b.lrc"));

        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);
        assert_eq!(
            plan.moves[0].lyrics,
            Some((
                library.join("incoming/a.LRC"),
                library.join("Artist/Album/01 Song.LRC")
            ))
        );

        assert_eq!(move_files(&plan.moves[0]).unwrap(), None);
        assert!(!source.exists());
        assert!(library.join("Artist/Album/01 Song.mp3").is_file());
        assert!(library.join("Artist/Album/01 Song.LRC").is_file());
        assert!(library.join("incoming/b.lrc").is_file());
    }

    #[test]
    fn test_plan_does_not_move_when_lyrics_target_exists() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);
        touch(&library.join("incoming/a.lrc"));
        touch(&library.join("Artist/Album/01 Song.lrc"));

        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);

        assert_eq!(statuses(&plan), [OrganizeStatus::TargetExists]);
    }

    #[test]
    fn test_move_files_never_overwrites() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        touch(&source);
        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);
        // 計画を立てた後に、移動先へ別のファイルが置かれた
        let target = library.join("Artist/Album/01 Song.mp3");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"other").unwrap();

        let error = move_files(&plan.moves[0]).unwrap_err();

        assert!(error.to_string().contains("同じ名前のファイル"));
        assert_eq!(fs::read(&source).unwrap(), b"data");
        assert_eq!(fs::read(&target).unwrap(), b"other");
    }

    #[test]
    fn test_undo_move_restores_files() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("incoming/a.mp3");
        let lyrics = library.join("incoming/a.lrc");
        touch(&source);
        touch(&lyrics);
        let plan = plan_in(&library, &[track("t1", &source)], DEFAULT);
        move_files(&plan.moves[0]).unwrap();
        assert!(!source.exists());

        undo_move(&plan.moves[0]);

        assert!(source.is_file());
        assert!(lyrics.is_file());
        assert!(!library.join("Artist/Album/01 Song.mp3").exists());
    }

    /// 曲を移して、片付けまで行う
    fn organize(library: &Path, tracks: &[Track], template: &str) {
        let plan = plan_in(library, tracks, template);
        for planned in &plan.moves {
            move_files(planned).unwrap();
        }
        tidy_folders(&plan.moves, &remove_file);
    }

    /// テストの`discard`（ゴミ箱を使わず、ファイルを消す）
    fn remove_file(path: &Path) -> AppResult<Option<PathBuf>> {
        fs::remove_file(path)
            .map(|()| None)
            .map_err(|e| AppError::Io(e.to_string()))
    }

    #[test]
    fn test_tidy_moves_folder_images_with_the_whole_album() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let first = library.join("incoming/rip/a.mp3");
        let second = library.join("incoming/rip/b.mp3");
        for path in [&first, &second] {
            touch(path);
        }
        touch(&library.join("incoming/rip/cover.jpg"));
        touch(&library.join("incoming/rip/.DS_Store"));
        let mut second_track = track("t2", &second);
        second_track.title = Some("Second".to_string());
        second_track.track_number = Some(2);

        organize(&library, &[track("t1", &first), second_track], DEFAULT);

        assert!(library.join("Artist/Album/01 Song.mp3").is_file());
        assert!(library.join("Artist/Album/02 Second.mp3").is_file());
        assert!(library.join("Artist/Album/cover.jpg").is_file());
        // 空になったフォルダは、上のフォルダも消す（ライブラリフォルダは消さない）
        assert!(!library.join("incoming").exists());
        assert!(library.is_dir());
    }

    #[test]
    fn test_tidy_keeps_folder_images_when_tracks_remain_or_split() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");

        // 曲が残るフォルダ: 画像もフォルダも残す
        let moved = library.join("mixed/a.mp3");
        touch(&moved);
        touch(&library.join("mixed/stays.mp3"));
        touch(&library.join("mixed/cover.jpg"));
        organize(&library, &[track("t1", &moved)], DEFAULT);
        assert!(library.join("mixed/cover.jpg").is_file());
        assert!(!library.join("Artist/Album/cover.jpg").exists());

        // 曲が別々のフォルダへ移るフォルダ: 画像は残す（フォルダも残る）
        let one = library.join("split/a.mp3");
        let two = library.join("split/b.mp3");
        touch(&one);
        touch(&two);
        touch(&library.join("split/folder.png"));
        let mut other_album = track("t3", &two);
        other_album.album = Some("Other".to_string());
        organize(
            &library,
            &[track("t2", &one), other_album],
            "%album%/%filename%",
        );
        assert!(library.join("split/folder.png").is_file());
        assert!(library.join("Album/a.mp3").is_file());
        assert!(library.join("Other/b.mp3").is_file());
    }

    #[test]
    fn test_tidy_keeps_images_when_folders_merge_and_never_overwrites_them() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");

        // 2つのフォルダの曲が、同じフォルダへ移る: どちらの画像も移さない
        let one = library.join("one/a.mp3");
        let two = library.join("two/b.mp3");
        for path in [&one, &two] {
            touch(path);
        }
        touch(&library.join("one/cover.jpg"));
        touch(&library.join("two/cover.jpg"));
        organize(
            &library,
            &[track("t1", &one), track("t2", &two)],
            "%artist%/%filename%",
        );
        assert!(library.join("one/cover.jpg").is_file());
        assert!(library.join("two/cover.jpg").is_file());
        assert!(!library.join("Artist/cover.jpg").exists());

        // 移動先に同じ名前の画像がある: 上書きしない
        let source = library.join("rip/c.mp3");
        touch(&source);
        fs::write(library.join("rip/cover.jpg"), b"new").unwrap();
        fs::create_dir_all(library.join("Album")).unwrap();
        fs::write(library.join("Album/cover.jpg"), b"old").unwrap();
        organize(&library, &[track("t3", &source)], "%album%/%filename%");
        assert_eq!(fs::read(library.join("Album/cover.jpg")).unwrap(), b"old");
        assert_eq!(fs::read(library.join("rip/cover.jpg")).unwrap(), b"new");
    }

    #[test]
    fn test_remove_empty_folders_stops_at_other_files() {
        let dir = TempDir::new();
        let library = dir.path().join("Music");
        let source = library.join("keep/sub/a.mp3");
        touch(&source);
        touch(&library.join("keep/notes.txt"));

        organize(&library, &[track("t1", &source)], DEFAULT);

        // 空になったフォルダだけを消す（ほかのファイルがあるフォルダは残す）
        assert!(!library.join("keep/sub").exists());
        assert!(library.join("keep/notes.txt").is_file());
    }

    /// 音楽フォルダへ集める計画を立てる（ライブラリフォルダは`library`）
    fn plan_into_music(library: &Path, music: &Path, tracks: &[Track], template: &str) -> Plan {
        plan(
            tracks,
            &[library.to_path_buf(), music.to_path_buf()],
            &parse(template),
            &tracks.iter().map(|t| t.file_path.clone()).collect(),
            Some(music),
        )
    }

    #[test]
    fn test_plan_collects_tracks_into_the_music_folder() {
        let dir = TempDir::new();
        let library = dir.path().join("Old Library");
        let music = dir.path().join("Music");
        fs::create_dir_all(&music).unwrap();
        let inside = library.join("rip/a.mp3");
        let outside = dir.path().join("Downloads/b.mp3");
        let in_music = music.join("loose/c.mp3");
        for path in [&inside, &outside, &in_music] {
            touch(path);
        }
        let mut second = track("t2", &outside);
        second.title = Some("Second".to_string());
        second.track_number = Some(2);
        let mut third = track("t3", &in_music);
        third.title = Some("Third".to_string());
        third.track_number = Some(3);

        // 書式に`/`がなくても、音楽フォルダの直下に置く
        let flat = plan_into_music(&library, &music, &[track("t1", &inside)], "%title%");
        assert_eq!(flat.moves[0].to, music.join("Song.mp3"));

        let plan = plan_into_music(
            &library,
            &music,
            &[track("t1", &inside), second, third],
            DEFAULT,
        );

        // ライブラリフォルダの外の曲も、音楽フォルダの中の曲も、音楽フォルダの書式どおりの場所へ移す
        assert_eq!(
            statuses(&plan),
            [
                OrganizeStatus::Move,
                OrganizeStatus::Move,
                OrganizeStatus::Move
            ]
        );
        let targets: Vec<&Path> = plan.moves.iter().map(|m| m.to.as_path()).collect();
        assert_eq!(
            targets,
            [
                music.join("Artist/Album/01 Song.mp3"),
                music.join("Artist/Album/02 Second.mp3"),
                music.join("Artist/Album/03 Third.mp3")
            ]
        );
        // 今の場所は、曲のあるライブラリフォルダから（外の曲は、そのままのパス）。整理後の場所は、音楽フォルダから
        assert_eq!(
            plan.entries[0].current_path,
            Path::new("rip/a.mp3").display().to_string()
        );
        assert_eq!(plan.entries[1].current_path, outside.display().to_string());
        assert_eq!(
            plan.entries[1].new_path.as_deref(),
            Some(
                Path::new("Artist/Album/02 Second.mp3")
                    .display()
                    .to_string()
                    .as_str()
            )
        );
        // 空になったフォルダを消す範囲は、曲のあるライブラリフォルダ（外の曲は、消さない）
        let folders: Vec<Option<&Path>> = plan
            .moves
            .iter()
            .map(|m| m.library_folder.as_deref())
            .collect();
        assert_eq!(
            folders,
            [Some(library.as_path()), None, Some(music.as_path())]
        );
        // 同じボリュームの中のため、コピーせずに移す
        assert!(plan.moves.iter().all(|m| !m.copy));
    }

    #[test]
    fn test_copy_for_move_copies_and_discards_originals_afterwards() {
        let dir = TempDir::new();
        let library = dir.path().join("Old Library");
        let music = dir.path().join("Music");
        fs::create_dir_all(&music).unwrap();
        let source = library.join("rip/a.mp3");
        touch(&source);
        touch(&library.join("rip/a.lrc"));
        let mut planned = plan_into_music(&library, &music, &[track("t1", &source)], DEFAULT)
            .moves
            .remove(0);
        // ボリュームをまたぐ場合の処理を、同じボリュームで確かめる
        planned.copy = true;
        assert_eq!(planned.size(), 8);

        let copied = copy_for_move(&planned, &mut [0u8; 3]).unwrap();

        assert_eq!(
            copied,
            CopiedMove {
                lyrics_copied: true,
                warning: None
            }
        );
        let target = music.join("Artist/Album/01 Song.mp3");
        assert_eq!(fs::read(&target).unwrap(), b"data");
        assert!(music.join("Artist/Album/01 Song.lrc").is_file());
        // 元は、データベースを書き換えるまで残す
        assert!(source.is_file());

        let discarded = std::cell::RefCell::new(Vec::new());
        let warning = discard_originals(&planned, &copied, &|path| {
            discarded.borrow_mut().push(path.to_path_buf());
            remove_file(path)
        });
        assert_eq!(warning, None);
        assert_eq!(
            *discarded.borrow(),
            [source.clone(), library.join("rip/a.lrc")]
        );
        tidy_folders(std::slice::from_ref(&planned), &remove_file);
        // 空になったフォルダは消す（ライブラリフォルダは残す）
        assert!(!library.join("rip").exists());
        assert!(library.is_dir());
    }

    #[test]
    fn test_remove_copied_keeps_the_originals() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        fs::create_dir_all(&music).unwrap();
        let source = dir.path().join("Downloads/a.mp3");
        touch(&source);
        let mut planned = plan_into_music(&music, &music, &[track("t1", &source)], DEFAULT)
            .moves
            .remove(0);
        planned.copy = true;
        let copied = copy_for_move(&planned, &mut [0u8; 16]).unwrap();

        remove_copied(&planned, &copied);

        assert!(!planned.to.exists());
        assert!(source.is_file());
    }

    #[test]
    fn test_discard_failures_are_reported() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        fs::create_dir_all(&music).unwrap();
        let source = dir.path().join("Downloads/a.mp3");
        touch(&source);
        let planned = plan_into_music(&music, &music, &[track("t1", &source)], DEFAULT)
            .moves
            .remove(0);
        let copied = CopiedMove {
            lyrics_copied: false,
            warning: None,
        };

        let warning = discard_originals(&planned, &copied, &|_| {
            Err(AppError::Io("ゴミ箱がありません".to_string()))
        })
        .unwrap();

        assert!(warning.contains("元のファイルを消せませんでした"));
        assert!(warning.contains("ゴミ箱がありません"));
        assert!(source.is_file());
    }

    #[test]
    fn test_folder_images_are_copied_when_moving_across_volumes() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let library = dir.path().join("Old Library");
        fs::create_dir_all(&music).unwrap();
        let source = library.join("rip/a.mp3");
        touch(&source);
        touch(&library.join("rip/cover.jpg"));
        let mut planned = plan_into_music(&library, &music, &[track("t1", &source)], DEFAULT)
            .moves
            .remove(0);
        planned.copy = true;
        let copied = copy_for_move(&planned, &mut [0u8; 16]).unwrap();
        discard_originals(&planned, &copied, &remove_file);

        tidy_folders(std::slice::from_ref(&planned), &remove_file);

        assert!(music.join("Artist/Album/cover.jpg").is_file());
        assert!(!library.join("rip").exists());
    }

    #[test]
    fn test_copy_for_move_never_overwrites() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        fs::create_dir_all(&music).unwrap();
        let source = dir.path().join("Downloads/a.mp3");
        touch(&source);
        let mut planned = plan_into_music(&music, &music, &[track("t1", &source)], DEFAULT)
            .moves
            .remove(0);
        planned.copy = true;
        fs::create_dir_all(planned.to.parent().unwrap()).unwrap();
        fs::write(&planned.to, b"other").unwrap();

        let error = copy_for_move(&planned, &mut [0u8; 16]).unwrap_err();

        assert!(error.to_string().contains("同じ名前のファイル"));
        assert_eq!(fs::read(&planned.to).unwrap(), b"other");
        assert!(source.is_file());
    }
}
