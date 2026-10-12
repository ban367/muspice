//! ファイルの整理（タグに基づくリネーム・移動）
//!
//! 選んだ曲のファイルを、書式（`%albumartist%/%album%/%track% %title%`など）に従って、その曲のある
//! ライブラリフォルダの中で移す（ADR-052）。計画（`plan`）と実行（`move_files`・`tidy_folders`）を
//! 分け、画面は計画を一覧（今の場所 → 整理後の場所）で見せてから実行する。
//!
//! - 移す先は、曲のあるライブラリフォルダの中だけ（フォルダの外の曲は、動かさない）
//! - 移動先に同じ名前のファイルがある曲・選んだ曲の中で同じ移動先になる曲は、動かさない
//!   （上書きしない・連番も付けない）
//! - 曲と同じ名前の歌詞ファイル（`.lrc`）は、曲と一緒に移す
//! - フォルダの画像（`cover.jpg`など）は、フォルダの曲がすべて同じフォルダへ移った場合に、一緒に移す
//! - 曲がなくなって空になったフォルダは、消す（ライブラリフォルダ自体は消さない）
//!
//! トラックのID・お気に入り・再生回数・プレイリストへの登録は、変えない（データベースの場所だけを
//! 書き換える。呼び出し側が`repository::relink_track`で行う）。

use crate::album_art::is_folder_image;
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
use std::io;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// 書式の長さの上限（文字数）
pub const MAX_TEMPLATE_LENGTH: usize = 255;

/// ジャンルのタグがない曲を置くフォルダの名前（アーティスト・アルバムと同じく、言語によらず固定）
const UNKNOWN_GENRE: &str = "Unknown Genre";

/// 歌詞ファイルの拡張子（大文字と小文字は区別しない）
const LYRICS_EXTENSION: &str = "lrc";

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
    fn components(&self, track: &Track, from: &Path) -> Vec<String> {
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
}

/// 1曲の移動（実行用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMove {
    pub track_id: String,
    pub from: PathBuf,
    pub to: PathBuf,
    /// 一緒に移す歌詞ファイル（今の場所, 移動先）
    pub lyrics: Option<(PathBuf, PathBuf)>,
    /// 曲のあるライブラリフォルダ（空になったフォルダは、この手前まで消す）
    pub library_folder: PathBuf,
}

/// 計画（表示用の一覧と、実行する移動）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub entries: Vec<OrganizeEntry>,
    pub moves: Vec<PlannedMove>,
}

/// フォルダの中身（計画の間だけ覚え、曲ごとにフォルダを読み直さない）
#[derive(Default)]
struct DirCache {
    entries: HashMap<PathBuf, Vec<PathBuf>>,
}

impl DirCache {
    fn list(&mut self, dir: &Path) -> &[PathBuf] {
        self.entries
            .entry(dir.to_path_buf())
            .or_insert_with(|| list_dir(dir).unwrap_or_default())
    }

    /// 曲と同じフォルダにある、同じ名前の歌詞ファイル（`lyrics::read_track_lyrics`が読むもの）
    fn lyrics_file(&mut self, track_path: &Path) -> Option<PathBuf> {
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

fn list_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
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

/// 選んだ曲の、ファイルの整理の計画を立てる
///
/// @param tracks - 選んだ曲（この順に、一覧にする）
/// @param library_folders - ライブラリフォルダのパス
/// @param registered_paths - ライブラリに登録されている、すべての曲のパス（見つからない曲を含む。
///   ファイルがなくても、その場所は移動先にしない）
pub fn plan(
    tracks: &[Track],
    library_folders: &[PathBuf],
    template: &Template,
    registered_paths: &HashSet<String>,
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

        let Some(folder) = folder else {
            entry.status = OrganizeStatus::OutsideLibrary;
            entries.push(entry);
            continue;
        };
        if track.is_missing || !from.is_file() {
            entry.status = OrganizeStatus::Missing;
            entries.push(entry);
            continue;
        }

        let base = if template.in_place {
            from.parent().unwrap_or(folder)
        } else {
            folder
        };
        let to = template
            .components(track, &from)
            .iter()
            .fold(base.to_path_buf(), |path, component| path.join(component));
        entry.new_path = Some(display_path(&to, Some(folder)));

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
                    from,
                    to,
                    lyrics,
                    library_folder: folder.clone(),
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
        let occupied = candidate.targets().any(|(from, to)| is_occupied(to, from))
            || candidate
                .planned
                .to
                .to_str()
                .is_some_and(|to| registered_paths.contains(to));

        entries[candidate.index].status = if shared {
            OrganizeStatus::SameTarget
        } else if occupied {
            OrganizeStatus::TargetExists
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

/// 1曲のファイル（と、同じ名前の歌詞ファイル）を移す
///
/// 移動先のフォルダは、なければ作る。歌詞ファイルを移せなかった場合は、曲は移したままにして、
/// 理由を返す（曲の移動は、成功として扱う）。
/// @returns 歌詞ファイルを移せなかった場合の理由
pub fn move_files(planned: &PlannedMove) -> AppResult<Option<String>> {
    let failed = |e: io::Error| {
        AppError::Io(if e.kind() == io::ErrorKind::AlreadyExists {
            "移動先に同じ名前のファイルがあります".to_string()
        } else {
            format!("ファイルを移せませんでした: {}", e)
        })
    };
    if let Some(parent) = planned.to.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Io(format!("移動先のフォルダを作れませんでした: {}", e)))?;
    }
    move_no_replace(&planned.from, &planned.to).map_err(failed)?;

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
                "ファイルを元の場所へ戻せませんでした: {} → {}: {}",
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

/// 曲を移した後の片付け: フォルダの画像を移し、空になったフォルダを消す
///
/// フォルダの画像（`cover.jpg`など。埋め込みの画像がない曲のアルバムアートに使う）は、元のフォルダに
/// 曲が残らず、移した曲の移動先が1つのフォルダで、そのフォルダへ移ってきた曲がすべて同じフォルダから
/// 来た場合に、一緒に移す（アルバムのフォルダを、そのまま移した場合）。移動先に同じ名前の画像が
/// あれば、移さない。
/// @param moved - 移した曲
pub fn tidy_folders(moved: &[PlannedMove]) {
    // 元のフォルダ → (移動先のフォルダ, ライブラリフォルダ)。同じフォルダの中での改名は除く
    let mut destinations: HashMap<&Path, (HashSet<&Path>, &Path)> = HashMap::new();
    // 移動先のフォルダ → 元のフォルダ
    let mut sources: HashMap<&Path, HashSet<&Path>> = HashMap::new();
    for planned in moved {
        let (Some(from_dir), Some(to_dir)) = (planned.from.parent(), planned.to.parent()) else {
            continue;
        };
        if from_dir == to_dir {
            continue;
        }
        destinations
            .entry(from_dir)
            .or_insert_with(|| (HashSet::new(), planned.library_folder.as_path()))
            .0
            .insert(to_dir);
        sources.entry(to_dir).or_default().insert(from_dir);
    }

    for (from_dir, (to_dirs, library_folder)) in destinations {
        let mut to_dirs = to_dirs.into_iter();
        if let (Some(to_dir), None) = (to_dirs.next(), to_dirs.next())
            && sources.get(to_dir).is_some_and(|from| from.len() == 1)
            && !has_audio_files(from_dir)
        {
            move_folder_images(from_dir, to_dir);
        }
        remove_empty_folders(from_dir, library_folder);
    }
}

/// フォルダの直下に、音楽ファイルがあるか（調べられない場合は、あるとみなす）
fn has_audio_files(dir: &Path) -> bool {
    list_dir(dir).map_or(true, |paths| {
        paths.iter().any(|path| is_supported_audio_file(path))
    })
}

/// フォルダの画像を、別のフォルダへ移す（同じ名前のファイルがあれば移さない。失敗はログに残す）
fn move_folder_images(from_dir: &Path, to_dir: &Path) {
    for image in list_dir(from_dir).unwrap_or_default() {
        let Some(name) = image.file_name() else {
            continue;
        };
        if !is_folder_image(&image) || !image.is_file() {
            continue;
        }
        if let Err(e) = move_no_replace(&image, &to_dir.join(name)) {
            log::info!(
                "フォルダの画像を移しませんでした: {}: {}",
                image.display(),
                e
            );
        }
    }
}

/// 空になったフォルダを、上へさかのぼって消す（ライブラリフォルダ自体と、その外は消さない）
fn remove_empty_folders(dir: &Path, library_folder: &Path) {
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
                library_folder: library,
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
        );

        assert_eq!(statuses(&plan), [OrganizeStatus::TargetExists]);
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
        tidy_folders(&plan.moves);
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
}
