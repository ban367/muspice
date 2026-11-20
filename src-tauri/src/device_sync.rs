//! デバイスへの同期の差分の計算（デバイス上の配置と、コピー・削除・リネームの決定）
//!
//! 同期する曲・プレイリストと、デバイスの管理ファイル（`device_manifest`）を比べて、
//! 同期で行う処理（`SyncPlan`）を決める。ファイルシステムには触れない（デバイス上の
//! ファイルのサイズは、呼び出し側が渡す関数で調べる）。実行は`device_transfer`が行う。
//!
//! - 曲は、デバイスごとの配置の書式（ファイルの整理と同じ書き方。既定は
//!   `アルバムアーティスト/アルバム/01 タイトル.拡張子`。ADR-111）でタグから配置する。一度決めたパスは
//!   管理ファイルに記録し、タグ・書式が変わらない限り変えない
//! - 削除・上書きするのは、管理ファイルに記録のあるファイルだけ。記録のないファイルが
//!   同じ場所にある場合は、別の名前（` (2)`を付ける）にする。ただし同じ場所に同じサイズの
//!   ファイルがある場合は、コピー済みとして扱う（管理ファイルをなくした場合や、ライブラリ内で
//!   ファイルを移動してトラックが登録し直された場合に、同じ曲をコピーし直さない）
//! - 元のファイルが変わったかは、コピーした時点のサイズ・更新日時（管理ファイルの記録）と
//!   比べて判定する
//! - 変換してコピーする曲（ADR-118）は、拡張子を`.m4a`にし、変換の設定（管理ファイルの`transcode`）も
//!   比べる。設定が変わった曲は、コピーし直す。コピーする容量は、ビットレートと長さから見積もる

use crate::device_manifest::{Manifest, ManifestExtra, ManifestFile, ManifestPlaylist};
use crate::file_organize::Template;
use crate::library_folder::is_dst_shift;
use crate::repository::TransferTrack;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// アーティスト・アルバムのタグがない曲を置くフォルダの名前
///
/// デバイス上のフォルダ名のため、表示の言語によらず固定にする（言語を切り替えても、
/// コピー済みの曲のパスが変わらないようにする）。ファイルの整理（`file_organize`）も同じ名前を使う。
pub(crate) const UNKNOWN_ARTIST: &str = "Unknown Artist";
pub(crate) const UNKNOWN_ALBUM: &str = "Unknown Album";
pub(crate) const UNKNOWN_TITLE: &str = "Unknown Title";

/// プレイリストのファイルの拡張子（UTF-8のM3U）
const PLAYLIST_EXTENSION: &str = "m3u8";

/// フォルダ名・ファイル名（拡張子を除く）の長さの上限
///
/// FAT32・exFATの上限（255文字）と、パス全体の長さに上限のある機器を考えて短めにする。
/// 連番（` (2)`）・拡張子・コピー中の一時ファイルの接尾辞を足しても255バイトに収まる。
const MAX_COMPONENT_CHARS: usize = 80;
const MAX_COMPONENT_BYTES: usize = 200;

/// FAT32・exFAT・Windowsでファイル名に使えない文字
const FORBIDDEN_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windowsでファイル名に使えない名前（拡張子が付いていても使えない）
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 曲をAACに変換してコピーする時の設定（ADR-118）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transcode {
    /// AACのビットレート（kbps）
    pub bitrate_kbps: u32,
}

impl Transcode {
    /// 変換後のファイルの拡張子
    pub const EXTENSION: &'static str = "m4a";

    /// 管理ファイルに記録する、変換の設定（`aac-256`など）
    pub fn signature(self) -> String {
        format!("aac-{}", self.bitrate_kbps)
    }

    /// 変換後のファイルのサイズの見積もり（ビットレート×長さに、タグ・アルバムアートの分を足す）
    pub fn estimated_size(self, duration_seconds: Option<i32>, source_size: i64) -> i64 {
        /// タグ・アルバムアート・コンテナの分として足す大きさ
        const OVERHEAD_BYTES: i64 = 256 * 1024;
        match duration_seconds {
            Some(seconds) if seconds > 0 => {
                i64::from(self.bitrate_kbps) * 1000 / 8 * i64::from(seconds) + OVERHEAD_BYTES
            }
            // 長さが分からない曲は、元のサイズで見積もる（多めに見る）
            _ => source_size,
        }
    }
}

/// 同期元のトラックと、元のファイルの今の状態
#[derive(Debug, Clone, PartialEq)]
pub struct SourceTrack {
    pub track: TransferTrack,
    /// 元のファイルが見つからない場合はNone
    pub file: Option<SourceFile>,
    /// AACに変換してコピーするか（`path`の拡張子も`.m4a`にする。ADR-118）
    pub transcode: Option<Transcode>,
    /// デバイス上の配置（デバイスの書式から求めた、デバイスのフォルダからの場所。区切りは`/`。
    /// `track_relative_path`）
    pub path: String,
    /// 曲に添えてコピーするファイル（歌詞ファイル・フォルダの画像。ADR-112）
    pub extras: Vec<SourceExtra>,
}

/// 曲に添えるファイルの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraKind {
    /// 曲と同じ名前の歌詞ファイル（`.lrc`）。デバイスでも、曲と同じ名前にする
    Lyrics,
    /// フォルダの画像（`cover.jpg`など）。デバイスでは、曲のフォルダに同じ名前で置く
    FolderImage,
}

/// 曲に添えてコピーするファイル
#[derive(Debug, Clone, PartialEq)]
pub struct SourceExtra {
    pub kind: ExtraKind,
    /// 元のファイルのパス
    pub source: String,
    pub file: SourceFile,
}

/// デバイスへコピーする、曲に添えるファイル
#[derive(Debug, Clone, PartialEq)]
pub struct ExtraCopyOp {
    /// 元のファイルのパス
    pub source: String,
    /// コピー先（デバイスのフォルダからの相対パス）
    pub dest: String,
    pub size: i64,
    pub modified_at: Option<i64>,
}

/// 元のファイルのサイズと更新日時（UNIX時間の秒）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceFile {
    pub size: i64,
    pub modified_at: Option<i64>,
}

/// 同期するプレイリスト（`track_ids`は曲順）
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePlaylist {
    pub id: String,
    pub name: String,
    pub track_ids: Vec<String>,
}

/// デバイスへコピーするファイル
#[derive(Debug, Clone, PartialEq)]
pub struct CopyOp {
    pub track_id: String,
    /// 元のファイルのパス
    pub source: String,
    /// コピー先（デバイスのフォルダからの相対パス）
    pub dest: String,
    /// 元のファイルのサイズと更新日時
    pub size: i64,
    pub modified_at: Option<i64>,
    /// AACに変換してコピーするか（ADR-118）
    pub transcode: Option<Transcode>,
    /// デバイス上のファイルのサイズ（変換する曲は見積もり。変換しない曲は`size`と同じ）
    pub device_size: i64,
    /// コピーできた後に削除する、同じ曲の古いファイル（変換の設定を変えて拡張子が変わった曲。
    /// 変換に失敗した・中止した時に、デバイスに曲が1つもなくならないよう、先に消さない）
    pub replaces: Option<ManifestFile>,
}

/// デバイス上で名前を変えるファイル（タグが変わって配置が変わった曲）
#[derive(Debug, Clone, PartialEq)]
pub struct RenameOp {
    /// 今の記録（`path`が変更前のパス）
    pub entry: ManifestFile,
    /// 変更後のパス
    pub to: String,
}

/// デバイスに書き出すプレイリスト
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistOp {
    pub playlist_id: String,
    /// 書き出し先（デバイスのフォルダの直下のファイル名）
    pub path: String,
    /// 曲順どおりの曲
    pub entries: Vec<PlaylistEntry>,
}

/// プレイリストの1曲
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistEntry {
    pub track_id: String,
    /// 長さ（秒）
    pub duration: Option<i32>,
    /// 表示名（`アーティスト - タイトル`）
    pub label: String,
}

/// 同期で行う処理
#[derive(Debug, Default, PartialEq)]
pub struct SyncPlan {
    /// デバイスから削除する曲（対象から外れた曲と、コピーし直して配置が変わる曲の古いファイル）
    pub to_delete: Vec<ManifestFile>,
    /// 削除するファイルの、デバイス上のサイズの合計
    pub delete_bytes: i64,
    /// デバイス上で名前を変える曲
    pub to_rename: Vec<RenameOp>,
    /// コピーする曲（アルバムの曲順）
    pub to_copy: Vec<CopyOp>,
    /// そのまま残す曲
    pub kept: Vec<ManifestFile>,
    /// 書き出すプレイリスト
    pub playlists: Vec<PlaylistOp>,
    /// 削除するプレイリストのファイル
    pub playlists_to_delete: Vec<ManifestPlaylist>,
    /// そのまま残すプレイリストのファイル（対象から外れたが、削除しない設定のもの）
    pub kept_playlists: Vec<ManifestPlaylist>,
    /// 元のファイルが見つからなかった曲数（デバイスにコピー済みなら、そのまま残す）
    pub missing_source_count: u32,
    /// 曲に添えるファイルのうち、コピーするもの・削除するもの・そのまま残すもの（ADR-112）
    pub extras_to_copy: Vec<ExtraCopyOp>,
    pub extras_to_delete: Vec<ManifestExtra>,
    pub kept_extras: Vec<ManifestExtra>,
    /// 削除する、曲に添えたファイルの、デバイス上のサイズの合計
    pub extra_delete_bytes: i64,
}

impl SyncPlan {
    /// コピーするファイルの、デバイス上のサイズの合計（変換する曲は見積もり）
    pub fn copy_bytes(&self) -> i64 {
        self.to_copy.iter().map(|op| op.device_size).sum()
    }

    /// 削除する曲（先に削除する曲と、コピーできた後に削除する古いファイル）
    pub fn deleted_files(&self) -> impl Iterator<Item = &ManifestFile> {
        self.to_delete
            .iter()
            .chain(self.to_copy.iter().filter_map(|op| op.replaces.as_ref()))
    }

    /// コピーする曲のうち、変換する曲数
    pub fn transcode_count(&self) -> usize {
        self.to_copy
            .iter()
            .filter(|op| op.transcode.is_some())
            .count()
    }

    /// コピーする、曲に添えるファイルのサイズの合計
    pub fn extra_copy_bytes(&self) -> i64 {
        self.extras_to_copy.iter().map(|op| op.size).sum()
    }
}

/// 大文字・小文字を区別しないファイルシステム（FAT32・exFAT）で、同じパスかを比べるためのキー
pub fn path_key(path: &str) -> String {
    path.to_lowercase()
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// フォルダ名・ファイル名として使える形にする
///
/// - NFCにそろえる（濁点などを分けた形のままだと、表示できない機器がある）
/// - FAT32・exFAT・Windowsで使えない文字と制御文字を`_`にする
/// - 先頭のドット（隠しファイルになる）と、前後の空白・末尾のドット（Windowsで使えない）を除く
/// - 長すぎる名前を切り詰め、Windowsで使えない名前（`CON`など）には`_`を付ける
pub fn sanitize_component(name: &str) -> String {
    let replaced: String = name
        .nfc()
        .map(|c| {
            if c.is_control() || FORBIDDEN_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let is_trimmed = |c: char| c == '.' || c.is_whitespace();
    let trimmed = replaced.trim_matches(is_trimmed);

    let mut truncated = String::new();
    for c in trimmed.chars().take(MAX_COMPONENT_CHARS) {
        if truncated.len() + c.len_utf8() > MAX_COMPONENT_BYTES {
            break;
        }
        truncated.push(c);
    }
    // 切り詰めた位置が空白・ドットの場合は、末尾に残さない
    let result = truncated.trim_end_matches(is_trimmed);

    if result.is_empty() {
        return "_".to_string();
    }
    let base = result.split('.').next().unwrap_or(result).trim_end();
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(base)) {
        format!("_{}", result)
    } else {
        result.to_string()
    }
}

/// 元のファイルの拡張子（小文字。英数字以外を含む場合・ない場合はNone）
fn source_extension(file_path: &str) -> Option<String> {
    let extension = Path::new(file_path).extension()?.to_str()?;
    (!extension.is_empty() && extension.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| extension.to_ascii_lowercase())
}

/// 曲のデバイス上の配置（デバイスのフォルダからの場所。区切りは`/`。ADR-111）
///
/// 書式は、ファイルの整理と同じ書き方（値のない項目・ファイル名に使える形の決まりも同じ。
/// `file_organize::Template`）。`/`のない書式は、デバイスのフォルダの直下に置く。拡張子は、元の
/// ファイルの拡張子を小文字にしたもの（変換してコピーする曲は`m4a`。ADR-118）。
pub fn track_relative_path(
    track: &TransferTrack,
    template: &Template,
    transcode: Option<Transcode>,
) -> String {
    let extension = match transcode {
        Some(_) => Some(Transcode::EXTENSION.to_string()),
        None => source_extension(&track.file_path),
    };
    template
        .components_with_extension(track, Path::new(&track.file_path), extension.as_deref())
        .join("/")
}

/// プレイリストのファイル名（`プレイリスト名.m3u8`）
pub fn playlist_file_name(name: &str) -> String {
    format!("{}.{}", sanitize_component(name), PLAYLIST_EXTENSION)
}

/// パスを、拡張子の前と拡張子（ドットを含む）に分ける
fn split_extension(path: &str) -> (&str, &str) {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    match path[name_start..].rfind('.') {
        Some(dot) if dot > 0 => path.split_at(name_start + dot),
        _ => (path, ""),
    }
}

/// `actual`が`desired`と同じか、`desired`に連番（` (2)`など）を付けたものか
///
/// 大文字・小文字の違いは同じとみなす（大文字・小文字を区別しないファイルシステムで、
/// 大文字・小文字だけの変更を別のファイルへのリネームとして扱わないため）。
fn is_same_or_numbered(actual: &str, desired: &str) -> bool {
    let (actual, desired) = (path_key(actual), path_key(desired));
    if actual == desired {
        return true;
    }
    let (stem, extension) = split_extension(&desired);
    actual
        .strip_prefix(stem)
        .and_then(|rest| rest.strip_suffix(extension))
        .and_then(|middle| middle.strip_prefix(" ("))
        .and_then(|middle| middle.strip_suffix(')'))
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// 空いているパスを返す（`desired`が空いていなければ、連番を付けたパスを順に試す）
fn unique_path(desired: &str, is_free: impl Fn(&str) -> bool) -> String {
    if is_free(desired) {
        return desired.to_string();
    }
    let (stem, extension) = split_extension(desired);
    let mut number = 2u32;
    loop {
        let candidate = format!("{} ({}){}", stem, number, extension);
        if is_free(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

/// 元のファイルが、コピーした時点から変わっていないか
///
/// サイズが同じで更新日時がちょうど1時間ずれた場合は、夏時間の切り替えによるずれ
/// （ライブラリが外付けドライブのFAT32にある場合など）とみなす。更新日時を取得できない
/// 場合は、サイズだけで判定する。
fn is_source_unchanged(entry: &ManifestFile, file: &SourceFile) -> bool {
    if entry.size != file.size {
        return false;
    }
    match (entry.modified_at, file.modified_at) {
        (Some(recorded), Some(current)) => recorded == current || is_dst_shift(recorded, current),
        _ => true,
    }
}

/// コピー済みのファイルが、今の変換の設定でコピーしたものか（どちらも変換しない場合を含む）
fn is_same_transcode(entry: &ManifestFile, source: &SourceTrack) -> bool {
    entry.transcode == source.transcode.map(Transcode::signature)
}

/// デバイス上のコピーが、そのまま使えるか（元のファイル・変換の設定が変わらず、デバイス上のファイルがある）
fn is_copy_current(
    entry: &ManifestFile,
    source: &SourceTrack,
    file: &SourceFile,
    device_file_size: &impl Fn(&str) -> Option<i64>,
) -> bool {
    is_source_unchanged(entry, file)
        && is_same_transcode(entry, source)
        && device_file_size(&entry.path) == Some(entry.device_size())
}

fn copy_op(source: &SourceTrack, file: &SourceFile, dest: String) -> CopyOp {
    CopyOp {
        track_id: source.track.id.clone(),
        source: source.track.file_path.clone(),
        dest,
        size: file.size,
        modified_at: file.modified_at,
        transcode: source.transcode,
        device_size: source.transcode.map_or(file.size, |transcode| {
            transcode.estimated_size(source.track.duration, file.size)
        }),
        replaces: None,
    }
}

/// プレイリストでの曲の表示名（`アーティスト - タイトル`。改行などの制御文字は空白にする）
fn track_label(track: &TransferTrack) -> String {
    let title = non_empty(&track.title)
        .or_else(|| {
            Path::new(&track.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
        })
        .unwrap_or(UNKNOWN_TITLE);
    let label = match non_empty(&track.artist) {
        Some(artist) => format!("{} - {}", artist, title),
        None => title.to_string(),
    };
    label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// 同期する曲・プレイリストとデバイスの管理ファイルを比べて、同期で行う処理を決める
///
/// - `tracks`: 同期する曲（この順にコピーする）
/// - `playlists`: 書き出すプレイリスト
/// - `remove_unselected`: 対象から外れた曲・プレイリストを、デバイスから削除するか
/// - `device_file_size`: デバイス上のファイルのサイズ（相対パスを渡す。なければNone）
pub fn plan_sync(
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    manifest: &Manifest,
    remove_unselected: bool,
    device_file_size: impl Fn(&str) -> Option<i64>,
) -> SyncPlan {
    let mut plan = SyncPlan::default();
    let track_index: HashMap<&str, usize> = tracks
        .iter()
        .enumerate()
        .map(|(index, source)| (source.track.id.as_str(), index))
        .collect();

    // 使うことが決まったパス
    let mut claimed: HashSet<String> = HashSet::new();
    // 対象から外れた曲の記録（パスのキー → 管理ファイルでの位置）
    let mut stale: HashMap<String, usize> = HashMap::new();
    // 記録のあるトラック
    let mut recorded: HashSet<&str> = HashSet::new();
    // 新しいパスを決めるトラック: (トラックの位置, 今の記録の位置)
    let mut pending: Vec<(usize, Option<usize>)> = Vec::new();

    // 1. 管理ファイルの記録を、今の対象と比べる
    for (entry_index, entry) in manifest.files.iter().enumerate() {
        let key = path_key(&entry.path);
        // 同じパスの記録が重なっている場合は、最初の1件だけを使う
        if claimed.contains(&key) || stale.contains_key(&key) {
            continue;
        }
        let source = track_index
            .get(entry.track_id.as_str())
            // 同じトラックの記録が重なっている場合、2件目からは対象から外れた曲として扱う
            .filter(|_| recorded.insert(entry.track_id.as_str()))
            .map(|&index| (index, &tracks[index]));
        let Some((index, source)) = source else {
            stale.insert(key, entry_index);
            continue;
        };

        claimed.insert(key);
        let Some(file) = &source.file else {
            // 元のファイルが見つからない（ライブラリのドライブが外れているなど）: デバイス上の
            // コピーはそのまま残す
            plan.kept.push(entry.clone());
            plan.missing_source_count += 1;
            continue;
        };
        if !is_same_or_numbered(&entry.path, &source.path) {
            // タグが変わって配置が変わった（今のパスは、リネーム・削除するまで使ったままにする）
            pending.push((index, Some(entry_index)));
        } else if is_copy_current(entry, source, file, &device_file_size) {
            plan.kept.push(entry.clone());
        } else {
            // 元のファイル・変換の設定が変わったか、デバイス上のファイルがない・壊れている
            plan.to_copy.push(copy_op(source, file, entry.path.clone()));
        }
    }

    // 2. 記録のないトラックと、配置が変わったトラックのパスを決める
    for (index, source) in tracks.iter().enumerate() {
        if recorded.contains(source.track.id.as_str()) {
            continue;
        }
        if source.file.is_some() {
            pending.push((index, None));
        } else {
            plan.missing_source_count += 1;
        }
    }
    pending.sort_by_key(|&(index, _)| index);

    for (index, entry_index) in pending {
        let source = &tracks[index];
        let Some(file) = &source.file else { continue };
        let desired = source.path.clone();
        let desired_key = path_key(&desired);

        // 同じ場所に同じサイズのファイルがある: コピー済みとして扱う（変換する曲は、変換後のサイズが
        // 分からないため、扱わない）
        if entry_index.is_none()
            && source.transcode.is_none()
            && !claimed.contains(&desired_key)
            && device_file_size(&desired) == Some(file.size)
        {
            stale.remove(&desired_key);
            claimed.insert(desired_key);
            plan.kept.push(ManifestFile {
                path: desired,
                track_id: source.track.id.clone(),
                size: file.size,
                modified_at: file.modified_at,
                device_size: None,
                transcode: None,
            });
            continue;
        }

        // 管理外のファイルがある場所は避ける。対象から外れて削除する曲の場所は、削除の後に使う
        let dest = unique_path(&desired, |candidate| {
            let key = path_key(candidate);
            !claimed.contains(&key)
                && (device_file_size(candidate).is_none()
                    || (remove_unselected && stale.contains_key(&key)))
        });
        claimed.insert(path_key(&dest));

        match entry_index.map(|i| &manifest.files[i]) {
            // 変換の設定も同じ時だけ、名前を変える（変換の設定が変わると、拡張子も中身も変わる）
            Some(entry) if is_copy_current(entry, source, file, &device_file_size) => {
                plan.to_rename.push(RenameOp {
                    entry: entry.clone(),
                    to: dest,
                });
            }
            // 変換の設定が変わった: 新しい場所にコピー（変換）できてから、古いファイルを消す
            Some(entry) if !is_same_transcode(entry, source) => {
                plan.delete_bytes += device_file_size(&entry.path).unwrap_or(0);
                plan.to_copy.push(CopyOp {
                    replaces: Some(entry.clone()),
                    ..copy_op(source, file, dest)
                });
            }
            Some(entry) => {
                // 元のファイルも変わっている: 古いファイルを消して、新しい場所にコピーする
                plan.delete_bytes += device_file_size(&entry.path).unwrap_or(0);
                plan.to_delete.push(entry.clone());
                plan.to_copy.push(copy_op(source, file, dest));
            }
            None => plan.to_copy.push(copy_op(source, file, dest)),
        }
    }
    plan.to_copy
        .sort_by_key(|op| track_index.get(op.track_id.as_str()).copied());

    // 3. 対象から外れた曲を削除する（削除しない設定なら、記録を残す）
    for (entry_index, entry) in manifest.files.iter().enumerate() {
        if stale.get(&path_key(&entry.path)) != Some(&entry_index) {
            continue;
        }
        if remove_unselected {
            plan.delete_bytes += device_file_size(&entry.path).unwrap_or(0);
            plan.to_delete.push(entry.clone());
        } else {
            plan.kept.push(entry.clone());
        }
    }

    plan_extras(&mut plan, tracks, &track_index, manifest, &device_file_size);
    plan_playlists(
        &mut plan,
        tracks,
        &track_index,
        playlists,
        manifest,
        remove_unselected,
        device_file_size,
    );
    plan
}

/// 曲に添えるファイルの、デバイス上の場所（曲のデバイス上の場所から決める。置けない場合はNone）
///
/// 歌詞ファイルは、曲と同じ名前（拡張子だけを`.lrc`にする。連番の付いた曲は、連番も同じ）。
/// フォルダの画像は、曲のフォルダに元のファイルと同じ名前で置く（デバイスのフォルダの直下の曲には
/// 置かない。直下の画像は、どのアルバムの画像か分からないため）。
fn extra_dest(track_path: &str, extra: &SourceExtra) -> Option<String> {
    match extra.kind {
        ExtraKind::Lyrics => {
            let (stem, _) = split_extension(track_path);
            Some(format!(
                "{}.{}",
                stem,
                crate::file_organize::LYRICS_EXTENSION
            ))
        }
        ExtraKind::FolderImage => {
            let (folder, _) = track_path.rsplit_once('/')?;
            let name = Path::new(&extra.source).file_name()?.to_str()?;
            Some(format!("{}/{}", folder, sanitize_component(name)))
        }
    }
}

/// 曲に添えるファイル（歌詞ファイル・フォルダの画像）のコピー・削除を決める（ADR-112）
///
/// - 同期する曲の、同期した後の場所（コピー・名前の変更の先と、そのまま残す曲）に合わせて置く。
///   同じ場所になるファイル（同じフォルダの曲の画像）は、曲順で最初の曲のものを使う
/// - 記録があり、元のファイルが同じで変わっておらず、デバイス上にもある: そのまま残す。変わった・
///   デバイス上にない: 同じ場所にコピーし直す
/// - 記録がない: デバイス上になければコピーする。同じサイズのファイルがあれば、コピー済みとして記録する。
///   違うファイルがあれば、置かない（利用者が置いたファイルを上書きしない）
/// - 記録があるが、置く場所でなくなった（曲の場所が変わった・元のファイルがなくなった・コピーしない
///   設定にした）: 削除する。ただし、元のファイルが見つからない曲・対象から外れて残す曲の、歌詞ファイルと
///   同じフォルダの画像は残す（曲と一緒に残す）
fn plan_extras(
    plan: &mut SyncPlan,
    tracks: &[SourceTrack],
    track_index: &HashMap<&str, usize>,
    manifest: &Manifest,
    device_file_size: &impl Fn(&str) -> Option<i64>,
) {
    // 同期した後の、曲の場所（トラックID → 場所）
    let mut final_paths: HashMap<&str, &str> = HashMap::new();
    for op in &plan.to_copy {
        final_paths.insert(op.track_id.as_str(), op.dest.as_str());
    }
    for op in &plan.to_rename {
        final_paths.insert(op.entry.track_id.as_str(), op.to.as_str());
    }
    for entry in &plan.kept {
        final_paths
            .entry(entry.track_id.as_str())
            .or_insert(entry.path.as_str());
    }

    // 元のファイルがない・対象から外れたが、そのまま残す曲（添えたファイルも残す）
    let left_paths: Vec<&str> = plan
        .kept
        .iter()
        .filter(|entry| {
            track_index
                .get(entry.track_id.as_str())
                .is_none_or(|&index| tracks[index].file.is_none())
        })
        .map(|entry| entry.path.as_str())
        .collect();

    // 置く場所（パスのキー → 場所と元のファイル）
    let mut desired: Vec<(String, String, &SourceExtra)> = Vec::new();
    let mut claimed: HashSet<String> = HashSet::new();
    for source in tracks {
        let Some(track_path) = final_paths.get(source.track.id.as_str()) else {
            continue;
        };
        for extra in &source.extras {
            let Some(dest) = extra_dest(track_path, extra) else {
                continue;
            };
            let key = path_key(&dest);
            if claimed.insert(key.clone()) {
                desired.push((key, dest, extra));
            }
        }
    }

    let recorded: HashMap<String, &ManifestExtra> = manifest
        .extras
        .iter()
        .rev()
        .map(|entry| (path_key(&entry.path), entry))
        .collect();

    for (key, dest, extra) in desired {
        match recorded.get(&key) {
            Some(entry)
                if entry.source == extra.source
                    && is_extra_unchanged(entry, &extra.file)
                    && device_file_size(&entry.path) == Some(entry.size) =>
            {
                plan.kept_extras.push((*entry).clone());
            }
            Some(_) => plan.extras_to_copy.push(extra_copy_op(extra, dest)),
            None => match device_file_size(&dest) {
                None => plan.extras_to_copy.push(extra_copy_op(extra, dest)),
                Some(size) if size == extra.file.size => plan.kept_extras.push(ManifestExtra {
                    path: dest,
                    source: extra.source.clone(),
                    size,
                    modified_at: extra.file.modified_at,
                }),
                // 利用者が置いたファイル: 上書きしない
                Some(_) => {}
            },
        }
    }

    for (key, entry) in recorded {
        if claimed.contains(&key) {
            continue;
        }
        if belongs_to_left_track(&entry.path, &left_paths) {
            plan.kept_extras.push(entry.clone());
        } else {
            plan.extra_delete_bytes += device_file_size(&entry.path).unwrap_or(0);
            plan.extras_to_delete.push(entry.clone());
        }
    }
    // 管理ファイルの順を一定にする（記録の順）
    plan.extras_to_delete.sort_by(|a, b| a.path.cmp(&b.path));
}

/// 曲に添えたファイルが、そのまま残す曲（元のファイルがない・対象から外れた曲）のものか
///
/// 歌詞ファイルは、拡張子を除いた名前が同じ曲。画像は、同じフォルダの曲。
fn belongs_to_left_track(extra_path: &str, left_paths: &[&str]) -> bool {
    let is_lyrics = split_extension(extra_path)
        .1
        .eq_ignore_ascii_case(&format!(".{}", crate::file_organize::LYRICS_EXTENSION));
    if is_lyrics {
        let stem = path_key(split_extension(extra_path).0);
        left_paths
            .iter()
            .any(|path| path_key(split_extension(path).0) == stem)
    } else {
        let folder = extra_path
            .rsplit_once('/')
            .map(|(folder, _)| path_key(folder));
        left_paths
            .iter()
            .any(|path| path.rsplit_once('/').map(|(f, _)| path_key(f)) == folder)
    }
}

/// 曲に添えたファイルの元のファイルが、コピーした時点から変わっていないか（曲と同じ判定）
fn is_extra_unchanged(entry: &ManifestExtra, file: &SourceFile) -> bool {
    entry.size == file.size
        && match (entry.modified_at, file.modified_at) {
            (Some(recorded), Some(current)) => {
                recorded == current || is_dst_shift(recorded, current)
            }
            _ => true,
        }
}

fn extra_copy_op(extra: &SourceExtra, dest: String) -> ExtraCopyOp {
    ExtraCopyOp {
        source: extra.source.clone(),
        dest,
        size: extra.file.size,
        modified_at: extra.file.modified_at,
    }
}

/// プレイリストのファイルの書き出し・削除を決める
///
/// 書き出すプレイリストは毎回書き直す。名前を変えたプレイリストは、古いファイルを消して
/// 新しい名前で書き出す。
fn plan_playlists(
    plan: &mut SyncPlan,
    tracks: &[SourceTrack],
    track_index: &HashMap<&str, usize>,
    playlists: &[SourcePlaylist],
    manifest: &Manifest,
    remove_unselected: bool,
    device_file_size: impl Fn(&str) -> Option<i64>,
) {
    let selected: HashMap<&str, &SourcePlaylist> =
        playlists.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut claimed: HashSet<String> = HashSet::new();
    // 削除するファイルのパス（削除の後に、別のプレイリストが使える）
    let mut freed: HashSet<String> = HashSet::new();
    // 今のファイル名を使い続けるプレイリスト
    let mut assigned: HashMap<&str, String> = HashMap::new();

    for entry in &manifest.playlists {
        let key = path_key(&entry.path);
        if claimed.contains(&key) || freed.contains(&key) {
            continue;
        }
        match selected.get(entry.playlist_id.as_str()) {
            Some(playlist)
                if !assigned.contains_key(playlist.id.as_str())
                    && is_same_or_numbered(&entry.path, &playlist_file_name(&playlist.name)) =>
            {
                claimed.insert(key);
                assigned.insert(playlist.id.as_str(), entry.path.clone());
            }
            None if !remove_unselected => {
                claimed.insert(key);
                plan.kept_playlists.push(entry.clone());
            }
            // 名前が変わったプレイリストと、対象から外れたプレイリスト
            _ => {
                freed.insert(key);
                plan.playlists_to_delete.push(entry.clone());
            }
        }
    }

    for playlist in playlists {
        let path = assigned.remove(playlist.id.as_str()).unwrap_or_else(|| {
            unique_path(&playlist_file_name(&playlist.name), |candidate| {
                let key = path_key(candidate);
                !claimed.contains(&key)
                    && (device_file_size(candidate).is_none() || freed.contains(&key))
            })
        });
        claimed.insert(path_key(&path));

        let entries = playlist
            .track_ids
            .iter()
            .filter_map(|track_id| track_index.get(track_id.as_str()))
            .map(|&index| {
                let track = &tracks[index].track;
                PlaylistEntry {
                    track_id: track.id.clone(),
                    duration: track.duration,
                    label: track_label(track),
                }
            })
            .collect();
        plan.playlists.push(PlaylistOp {
            playlist_id: playlist.id.clone(),
            path,
            entries,
        });
    }
}

/// プレイリストのファイル（拡張M3U、UTF-8）の内容を作る
///
/// デバイスにある曲だけを、デバイスのフォルダからの相対パスで書く。プレイリストのファイルは
/// デバイスのフォルダの直下に置くため、パスに`..`を含まない（`..`を解釈できない機器がある）。
///
/// - `track_paths`: デバイスにある曲の相対パス（トラックID → パス）
pub fn build_m3u8(entries: &[PlaylistEntry], track_paths: &HashMap<&str, &str>) -> String {
    let mut content = String::from("#EXTM3U\r\n");
    for entry in entries {
        let Some(path) = track_paths.get(entry.track_id.as_str()) else {
            continue;
        };
        content.push_str(&format!(
            "#EXTINF:{},{}\r\n{}\r\n",
            entry.duration.unwrap_or(-1),
            entry.label,
            path
        ));
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_track(
        id: &str,
        artist: &str,
        album: &str,
        number: i32,
        title: &str,
    ) -> TransferTrack {
        TransferTrack {
            id: id.to_string(),
            file_path: format!("/music/{}.mp3", id),
            title: Some(title.to_string()),
            artist: Some(artist.to_string()),
            album: Some(album.to_string()),
            album_artist: None,
            compilation: false,
            genre: None,
            year: None,
            track_number: Some(number),
            disc_number: None,
            duration: Some(180),
            file_size: 100,
            bitrate: Some(320),
        }
    }

    /// 以前の配置の書式（曲のアーティスト/アルバム/曲名）
    fn legacy() -> Template {
        Template::parse(crate::device::LEGACY_DEVICE_TEMPLATE).unwrap()
    }

    /// 元のファイルが見つかるトラック（サイズ100・更新日時10。配置は以前の配置の書式）
    fn source(id: &str, artist: &str, album: &str, number: i32, title: &str) -> SourceTrack {
        let track = transfer_track(id, artist, album, number, title);
        SourceTrack {
            path: track_relative_path(&track, &legacy(), None),
            track,
            file: Some(SourceFile {
                size: 100,
                modified_at: Some(10),
            }),
            transcode: None,
            extras: Vec::new(),
        }
    }

    fn entry(path: &str, track_id: &str) -> ManifestFile {
        ManifestFile {
            path: path.to_string(),
            track_id: track_id.to_string(),
            size: 100,
            modified_at: Some(10),
            device_size: None,
            transcode: None,
        }
    }

    fn manifest(files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            files,
            ..Manifest::new("device-1")
        }
    }

    /// デバイス上のファイル（パス → サイズ）から、サイズを調べる関数を作る
    fn device(files: &[(&str, i64)]) -> impl Fn(&str) -> Option<i64> {
        let files: HashMap<String, i64> = files
            .iter()
            .map(|(path, size)| (path_key(path), *size))
            .collect();
        move |path| files.get(&path_key(path)).copied()
    }

    fn copy_dests(plan: &SyncPlan) -> Vec<&str> {
        plan.to_copy.iter().map(|op| op.dest.as_str()).collect()
    }

    fn paths(files: &[ManifestFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn test_sanitize_component() {
        assert_eq!(sanitize_component("AC/DC"), "AC_DC");
        assert_eq!(
            sanitize_component("What? <Live>: \"A|B\"*"),
            "What_ _Live__ _A_B__"
        );
        assert_eq!(sanitize_component("a\tb\nc"), "a_b_c");
        // 先頭のドットと、前後の空白・末尾のドットを除く
        assert_eq!(
            sanitize_component("...And Justice for All"),
            "And Justice for All"
        );
        assert_eq!(sanitize_component("  R.E.M.  "), "R.E.M");
        assert_eq!(sanitize_component(" . "), "_");
        assert_eq!(sanitize_component(""), "_");
        // Windowsで使えない名前
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("nul.live"), "_nul.live");
        assert_eq!(sanitize_component("Console"), "Console");
    }

    #[test]
    fn test_sanitize_component_normalizes_to_nfc() {
        // 「か」+ 濁点（NFD）を「が」（NFC）にそろえる
        assert_eq!(sanitize_component("か\u{3099}"), "が");
    }

    #[test]
    fn test_sanitize_component_truncates_long_names() {
        let ascii = sanitize_component(&"a".repeat(300));
        assert_eq!(ascii.chars().count(), MAX_COMPONENT_CHARS);

        // マルチバイト文字はバイト数の上限で切る（文字の途中では切らない）
        let japanese = sanitize_component(&"あ".repeat(300));
        assert!(japanese.len() <= MAX_COMPONENT_BYTES);
        assert_eq!(japanese.chars().count(), MAX_COMPONENT_BYTES / 3);

        // 切り詰めた位置の空白は残さない
        let spaced = sanitize_component(&format!("{} tail", "a".repeat(MAX_COMPONENT_CHARS - 1)));
        assert_eq!(spaced, "a".repeat(MAX_COMPONENT_CHARS - 1));
    }

    #[test]
    fn test_track_relative_path() {
        let legacy = legacy();
        let mut track = transfer_track("t1", "Artist", "Album", 3, "Title");
        assert_eq!(
            track_relative_path(&track, &legacy, None),
            "Artist/Album/03 Title.mp3"
        );

        track.disc_number = Some(2);
        assert_eq!(
            track_relative_path(&track, &legacy, None),
            "Artist/Album/2-03 Title.mp3"
        );

        // 拡張子は小文字にする
        track.file_path = "/music/Song.FLAC".to_string();
        track.disc_number = None;
        track.track_number = None;
        assert_eq!(
            track_relative_path(&track, &legacy, None),
            "Artist/Album/Title.flac"
        );
    }

    #[test]
    fn test_track_relative_path_uses_album_artist_by_default() {
        let template = Template::parse(crate::device::DEFAULT_DEVICE_TEMPLATE).unwrap();
        // アルバムアーティストのある曲は、アルバムアーティストのフォルダ（フィーチャリングの曲も同じ）
        let mut track = transfer_track("t1", "Band feat. Guest", "Album", 3, "Title");
        track.album_artist = Some("Band".to_string());
        assert_eq!(
            track_relative_path(&track, &template, None),
            "Band/Album/03 Title.mp3"
        );

        // アルバムアーティストのないコンピレーションは、Various Artists（一覧のまとめ方と同じ）
        let mut track = transfer_track("t2", "Someone", "Hits", 1, "Song");
        track.compilation = true;
        assert_eq!(
            track_relative_path(&track, &template, None),
            "Various Artists/Hits/01 Song.mp3"
        );
        // 以前の配置の書式では、曲のアーティストのフォルダ
        assert_eq!(
            track_relative_path(&track, &legacy(), None),
            "Someone/Hits/01 Song.mp3"
        );

        // `/`のない書式は、デバイスのフォルダの直下
        let flat = Template::parse("%artist% - %title%").unwrap();
        assert_eq!(
            track_relative_path(&track, &flat, None),
            "Someone - Song.mp3"
        );
    }

    /// 曲に添えるファイルのある、元のファイルが見つかるトラック
    fn source_with_extras(
        id: &str,
        album: &str,
        number: i32,
        extras: Vec<SourceExtra>,
    ) -> SourceTrack {
        SourceTrack {
            extras,
            ..source(id, "Artist", album, number, id)
        }
    }

    fn extra(kind: ExtraKind, source: &str, size: i64) -> SourceExtra {
        SourceExtra {
            kind,
            source: source.to_string(),
            file: SourceFile {
                size,
                modified_at: Some(10),
            },
        }
    }

    fn extra_entry(path: &str, source: &str, size: i64) -> ManifestExtra {
        ManifestExtra {
            path: path.to_string(),
            source: source.to_string(),
            size,
            modified_at: Some(10),
        }
    }

    #[test]
    fn test_plan_copies_lyrics_and_one_folder_image_per_folder() {
        let tracks = vec![
            source_with_extras(
                "t1",
                "Album",
                1,
                vec![
                    extra(ExtraKind::Lyrics, "/music/t1.lrc", 5),
                    extra(ExtraKind::FolderImage, "/music/Album/Cover.JPG", 50),
                ],
            ),
            // 同じフォルダの2曲目の画像は、1曲目のものと同じ場所になるため、コピーしない
            source_with_extras(
                "t2",
                "Album",
                2,
                vec![extra(ExtraKind::FolderImage, "/music/Other/folder.jpg", 60)],
            ),
        ];
        let plan = plan_sync(&tracks, &[], &manifest(vec![]), true, device(&[]));

        let mut dests: Vec<&str> = plan
            .extras_to_copy
            .iter()
            .map(|op| op.dest.as_str())
            .collect();
        dests.sort_unstable();
        assert_eq!(
            dests,
            [
                "Artist/Album/01 t1.lrc",
                "Artist/Album/Cover.JPG",
                "Artist/Album/folder.jpg"
            ]
        );
        assert_eq!(plan.extra_copy_bytes(), 5 + 50 + 60);
    }

    #[test]
    fn test_plan_keeps_unchanged_extras_and_recopies_changed_ones() {
        let tracks = vec![source_with_extras(
            "t1",
            "Album",
            1,
            vec![
                extra(ExtraKind::Lyrics, "/music/t1.lrc", 5),
                extra(ExtraKind::FolderImage, "/music/cover.jpg", 70),
            ],
        )];
        let files = vec![entry("Artist/Album/01 t1.mp3", "t1")];
        let on_device = [
            ("Artist/Album/01 t1.mp3", 100),
            ("Artist/Album/01 t1.lrc", 5),
            ("Artist/Album/cover.jpg", 50),
        ];
        let manifest = Manifest {
            extras: vec![
                extra_entry("Artist/Album/01 t1.lrc", "/music/t1.lrc", 5),
                // 元の画像が差し替えられた（サイズが変わった）
                extra_entry("Artist/Album/cover.jpg", "/music/cover.jpg", 50),
            ],
            ..manifest(files)
        };

        let plan = plan_sync(&tracks, &[], &manifest, true, device(&on_device));

        assert_eq!(
            plan.kept_extras
                .iter()
                .map(|e| e.path.as_str())
                .collect::<Vec<_>>(),
            ["Artist/Album/01 t1.lrc"]
        );
        assert_eq!(
            plan.extras_to_copy
                .iter()
                .map(|op| op.dest.as_str())
                .collect::<Vec<_>>(),
            ["Artist/Album/cover.jpg"]
        );
        assert!(plan.extras_to_delete.is_empty());
    }

    #[test]
    fn test_plan_does_not_overwrite_unmanaged_extras_and_adopts_same_size() {
        let tracks = vec![source_with_extras(
            "t1",
            "Album",
            1,
            vec![
                extra(ExtraKind::Lyrics, "/music/t1.lrc", 5),
                extra(ExtraKind::FolderImage, "/music/cover.jpg", 70),
            ],
        )];
        // 記録のないファイル: 同じサイズの歌詞はコピー済みとして記録し、違う画像は上書きしない
        let on_device = [
            ("Artist/Album/01 t1.lrc", 5),
            ("Artist/Album/cover.jpg", 999),
        ];
        let plan = plan_sync(&tracks, &[], &manifest(vec![]), true, device(&on_device));

        assert_eq!(
            plan.kept_extras
                .iter()
                .map(|e| e.path.as_str())
                .collect::<Vec<_>>(),
            ["Artist/Album/01 t1.lrc"]
        );
        assert!(plan.extras_to_copy.is_empty());
    }

    #[test]
    fn test_plan_deletes_extras_of_removed_tracks_but_keeps_those_of_left_tracks() {
        // t1は同期する曲で、歌詞ファイルがなくなった。t2は対象から外れた（残す設定）
        let tracks = vec![source("t1", "Artist", "Album", 1, "t1")];
        let files = vec![
            entry("Artist/Album/01 t1.mp3", "t1"),
            entry("Artist/Other/01 t2.mp3", "t2"),
        ];
        let on_device = [
            ("Artist/Album/01 t1.mp3", 100),
            ("Artist/Album/01 t1.lrc", 5),
            ("Artist/Other/01 t2.mp3", 100),
            ("Artist/Other/01 t2.lrc", 5),
            ("Artist/Other/cover.jpg", 50),
        ];
        let manifest = Manifest {
            extras: vec![
                extra_entry("Artist/Album/01 t1.lrc", "/music/t1.lrc", 5),
                extra_entry("Artist/Other/01 t2.lrc", "/music/t2.lrc", 5),
                extra_entry("Artist/Other/cover.jpg", "/music/cover.jpg", 50),
            ],
            ..manifest(files)
        };

        let plan = plan_sync(&tracks, &[], &manifest, false, device(&on_device));

        assert_eq!(
            plan.extras_to_delete
                .iter()
                .map(|e| e.path.as_str())
                .collect::<Vec<_>>(),
            ["Artist/Album/01 t1.lrc"]
        );
        let mut kept: Vec<&str> = plan.kept_extras.iter().map(|e| e.path.as_str()).collect();
        kept.sort_unstable();
        assert_eq!(kept, ["Artist/Other/01 t2.lrc", "Artist/Other/cover.jpg"]);
        assert_eq!(plan.extra_delete_bytes, 5);
    }

    #[test]
    fn test_plan_moves_lyrics_with_renamed_tracks() {
        // 配置が変わって名前を変える曲の歌詞ファイルは、古い場所を消し、新しい場所へコピーする
        let mut tracks = vec![source_with_extras(
            "t1",
            "New Album",
            1,
            vec![extra(ExtraKind::Lyrics, "/music/t1.lrc", 5)],
        )];
        tracks[0].track.album = Some("New Album".to_string());
        let files = vec![entry("Artist/Album/01 t1.mp3", "t1")];
        let on_device = [
            ("Artist/Album/01 t1.mp3", 100),
            ("Artist/Album/01 t1.lrc", 5),
        ];
        let manifest = Manifest {
            extras: vec![extra_entry("Artist/Album/01 t1.lrc", "/music/t1.lrc", 5)],
            ..manifest(files)
        };

        let plan = plan_sync(&tracks, &[], &manifest, true, device(&on_device));

        assert_eq!(plan.to_rename[0].to, "Artist/New Album/01 t1.mp3");
        assert_eq!(plan.extras_to_copy[0].dest, "Artist/New Album/01 t1.lrc");
        assert_eq!(plan.extras_to_delete[0].path, "Artist/Album/01 t1.lrc");
    }

    #[test]
    fn test_plan_renames_tracks_when_the_template_changes() {
        // 以前の配置でコピー済みの曲（元のファイルは変わっていない）
        let mut track = transfer_track("t1", "Someone", "Hits", 1, "Song");
        track.compilation = true;
        let files = vec![entry("Someone/Hits/01 Song.mp3", "t1")];
        let on_device = [("Someone/Hits/01 Song.mp3", 100)];
        let template = Template::parse(crate::device::DEFAULT_DEVICE_TEMPLATE).unwrap();
        let tracks = vec![SourceTrack {
            path: track_relative_path(&track, &template, None),
            track,
            file: Some(SourceFile {
                size: 100,
                modified_at: Some(10),
            }),
            transcode: None,
            extras: Vec::new(),
        }];

        let plan = plan_sync(&tracks, &[], &manifest(files), true, device(&on_device));
        assert!(plan.to_copy.is_empty());
        assert_eq!(plan.to_rename.len(), 1);
        assert_eq!(plan.to_rename[0].to, "Various Artists/Hits/01 Song.mp3");
    }

    #[test]
    fn test_track_relative_path_falls_back_for_missing_tags() {
        let track = TransferTrack {
            id: "t1".to_string(),
            file_path: "/music/untagged song.m4a".to_string(),
            title: None,
            artist: Some("  ".to_string()),
            album: None,
            album_artist: None,
            compilation: false,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration: None,
            file_size: 0,
            bitrate: None,
        };
        assert_eq!(
            track_relative_path(&track, &legacy(), None),
            "Unknown Artist/Unknown Album/untagged song.m4a"
        );
    }

    #[test]
    fn test_is_same_or_numbered() {
        assert!(is_same_or_numbered("A/B/01 T.mp3", "A/B/01 T.mp3"));
        assert!(is_same_or_numbered("A/B/01 T (2).mp3", "A/B/01 T.mp3"));
        assert!(is_same_or_numbered("A/B/01 T (13).mp3", "A/B/01 T.mp3"));
        // 大文字・小文字だけの違いは同じとみなす
        assert!(is_same_or_numbered("a/b/01 t.mp3", "A/B/01 T.mp3"));

        assert!(!is_same_or_numbered("A/B/01 T (x).mp3", "A/B/01 T.mp3"));
        assert!(!is_same_or_numbered("A/B/01 T ().mp3", "A/B/01 T.mp3"));
        assert!(!is_same_or_numbered("A/B/01 T.flac", "A/B/01 T.mp3"));
        assert!(!is_same_or_numbered("A/B/02 T.mp3", "A/B/01 T.mp3"));
    }

    #[test]
    fn test_plan_copies_new_tracks_in_order() {
        let tracks = vec![
            source("t1", "Artist", "Album", 1, "One"),
            source("t2", "Artist", "Album", 2, "Two"),
        ];
        let plan = plan_sync(&tracks, &[], &manifest(vec![]), true, device(&[]));

        assert_eq!(
            copy_dests(&plan),
            ["Artist/Album/01 One.mp3", "Artist/Album/02 Two.mp3"]
        );
        assert_eq!(plan.to_copy[0].source, "/music/t1.mp3");
        assert_eq!(plan.copy_bytes(), 200);
        assert!(plan.to_delete.is_empty() && plan.to_rename.is_empty() && plan.kept.is_empty());
    }

    #[test]
    fn test_plan_keeps_unchanged_tracks() {
        let tracks = vec![source("t1", "Artist", "Album", 1, "One")];
        let path = "Artist/Album/01 One.mp3";
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![entry(path, "t1")]),
            true,
            device(&[(path, 100)]),
        );

        assert_eq!(paths(&plan.kept), [path]);
        assert!(plan.to_copy.is_empty() && plan.to_delete.is_empty());
    }

    #[test]
    fn test_plan_recopies_changed_source_and_broken_device_file() {
        let mut changed = source("t1", "Artist", "Album", 1, "One");
        changed.file = Some(SourceFile {
            size: 120,
            modified_at: Some(99),
        });
        let tracks = vec![
            changed,
            source("t2", "Artist", "Album", 2, "Two"),
            source("t3", "Artist", "Album", 3, "Three"),
        ];
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![
                entry("Artist/Album/01 One.mp3", "t1"),
                entry("Artist/Album/02 Two.mp3", "t2"),
                entry("Artist/Album/03 Three.mp3", "t3"),
            ]),
            true,
            // t2はデバイスから消えていて、t3は途中までしか書かれていない
            device(&[
                ("Artist/Album/01 One.mp3", 100),
                ("Artist/Album/03 Three.mp3", 40),
            ]),
        );

        // 同じ場所にコピーし直す（削除はしない）
        assert_eq!(
            copy_dests(&plan),
            [
                "Artist/Album/01 One.mp3",
                "Artist/Album/02 Two.mp3",
                "Artist/Album/03 Three.mp3"
            ]
        );
        assert_eq!(plan.to_copy[0].size, 120);
        assert!(plan.to_delete.is_empty() && plan.kept.is_empty());
    }

    #[test]
    fn test_plan_treats_dst_shift_of_source_as_unchanged() {
        let mut shifted = source("t1", "Artist", "Album", 1, "One");
        shifted.file = Some(SourceFile {
            size: 100,
            modified_at: Some(10 + 3600),
        });
        let path = "Artist/Album/01 One.mp3";
        let plan = plan_sync(
            &[shifted],
            &[],
            &manifest(vec![entry(path, "t1")]),
            true,
            device(&[(path, 100)]),
        );
        assert!(plan.to_copy.is_empty());
        assert_eq!(plan.kept.len(), 1);
    }

    #[test]
    fn test_plan_removes_unselected_tracks_only_when_enabled() {
        let tracks = vec![source("t1", "Artist", "Album", 1, "One")];
        let files = vec![
            entry("Artist/Album/01 One.mp3", "t1"),
            entry("Artist/Album/02 Two.mp3", "t2"),
        ];
        let on_device = [
            ("Artist/Album/01 One.mp3", 100),
            ("Artist/Album/02 Two.mp3", 100),
        ];

        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(files.clone()),
            true,
            device(&on_device),
        );
        assert_eq!(paths(&plan.to_delete), ["Artist/Album/02 Two.mp3"]);
        assert_eq!(plan.delete_bytes, 100);
        assert_eq!(paths(&plan.kept), ["Artist/Album/01 One.mp3"]);

        // 削除しない設定では、記録を残す
        let plan = plan_sync(&tracks, &[], &manifest(files), false, device(&on_device));
        assert!(plan.to_delete.is_empty());
        assert_eq!(
            paths(&plan.kept),
            ["Artist/Album/01 One.mp3", "Artist/Album/02 Two.mp3"]
        );
    }

    /// AACに変換してコピーする曲（配置は以前の配置の書式。拡張子は`.m4a`）
    fn transcoded(id: &str, title: &str, bitrate_kbps: u32) -> SourceTrack {
        let mut track = source(id, "Artist", "Album", 1, title);
        track.transcode = Some(Transcode { bitrate_kbps });
        track.path = track_relative_path(&track.track, &legacy(), track.transcode);
        track
    }

    /// 変換してコピーした記録（デバイス上のサイズは`device_size`）
    fn transcoded_entry(
        path: &str,
        track_id: &str,
        bitrate_kbps: u32,
        device_size: i64,
    ) -> ManifestFile {
        ManifestFile {
            device_size: Some(device_size),
            transcode: Some(Transcode { bitrate_kbps }.signature()),
            ..entry(path, track_id)
        }
    }

    #[test]
    fn test_plan_transcodes_with_m4a_paths_and_estimated_sizes() {
        let tracks = vec![transcoded("t1", "One", 256)];
        let plan = plan_sync(&tracks, &[], &manifest(vec![]), true, device(&[]));

        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One.m4a"]);
        let op = &plan.to_copy[0];
        assert_eq!(op.transcode, Some(Transcode { bitrate_kbps: 256 }));
        // 元のファイルのサイズと、ビットレート×長さ（180秒）からの見積もり
        assert_eq!(op.size, 100);
        assert_eq!(op.device_size, 256 * 1000 / 8 * 180 + 256 * 1024);
        assert_eq!(plan.copy_bytes(), op.device_size);
        assert_eq!(plan.transcode_count(), 1);
    }

    #[test]
    fn test_plan_keeps_transcoded_copies_until_the_setting_changes() {
        let path = "Artist/Album/01 One.m4a";
        let recorded = manifest(vec![transcoded_entry(path, "t1", 256, 5000)]);
        let on_device = [(path, 5000)];

        // 元のファイル・設定が変わらず、デバイス上のサイズが記録と同じなら、そのまま
        let plan = plan_sync(
            &[transcoded("t1", "One", 256)],
            &[],
            &recorded,
            true,
            device(&on_device),
        );
        assert_eq!(paths(&plan.kept), [path]);
        assert!(plan.to_copy.is_empty());

        // ビットレートを変えた: 同じ場所に変換し直す
        let plan = plan_sync(
            &[transcoded("t1", "One", 128)],
            &[],
            &recorded,
            true,
            device(&on_device),
        );
        assert_eq!(copy_dests(&plan), [path]);
        assert!(plan.to_delete.is_empty() && plan.kept.is_empty());

        // 変換しない設定にした: 名前を変えずに、元のファイルをコピーしてから変換したファイルを消す
        let plan = plan_sync(
            &[source("t1", "Artist", "Album", 1, "One")],
            &[],
            &recorded,
            true,
            device(&on_device),
        );
        assert!(plan.to_rename.is_empty() && plan.to_delete.is_empty());
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One.mp3"]);
        assert_eq!(
            plan.to_copy[0].replaces.as_ref().map(|e| e.path.as_str()),
            Some(path)
        );
        assert_eq!(
            paths(&plan.deleted_files().cloned().collect::<Vec<_>>()),
            [path]
        );
    }

    #[test]
    fn test_plan_does_not_rename_original_copies_into_transcoded_ones() {
        // 変換しないでコピーした曲を、変換する設定にした
        let old = "Artist/Album/01 One.mp3";
        let plan = plan_sync(
            &[transcoded("t1", "One", 256)],
            &[],
            &manifest(vec![entry(old, "t1")]),
            true,
            device(&[(old, 100)]),
        );
        assert!(plan.to_rename.is_empty() && plan.to_delete.is_empty());
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One.m4a"]);
        assert_eq!(
            plan.to_copy[0].replaces.as_ref().map(|e| e.path.as_str()),
            Some(old)
        );
        assert_eq!(plan.delete_bytes, 100);

        // タグだけが変わった変換済みの曲は、名前を変える
        let recorded = "Artist/Album/01 Old.m4a";
        let plan = plan_sync(
            &[transcoded("t1", "New", 256)],
            &[],
            &manifest(vec![transcoded_entry(recorded, "t1", 256, 5000)]),
            true,
            device(&[(recorded, 5000)]),
        );
        assert_eq!(plan.to_rename.len(), 1);
        assert_eq!(plan.to_rename[0].to, "Artist/Album/01 New.m4a");
    }

    #[test]
    fn test_plan_does_not_adopt_unmanaged_files_for_transcoded_tracks() {
        // 記録のない同じ名前・元と同じサイズのファイルは、変換したものか分からないため、使わない
        let desired = "Artist/Album/01 One.m4a";
        let plan = plan_sync(
            &[transcoded("t1", "One", 256)],
            &[],
            &manifest(vec![]),
            true,
            device(&[(desired, 100)]),
        );
        assert!(plan.kept.is_empty());
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One (2).m4a"]);
    }

    #[test]
    fn test_plan_renames_when_tags_change() {
        // タイトルを変えた（元のファイルは変わっていない）
        let tracks = vec![source("t1", "Artist", "Album", 1, "New Title")];
        let old = "Artist/Album/01 Old Title.mp3";
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![entry(old, "t1")]),
            true,
            device(&[(old, 100)]),
        );

        assert_eq!(plan.to_rename.len(), 1);
        assert_eq!(plan.to_rename[0].entry.path, old);
        assert_eq!(plan.to_rename[0].to, "Artist/Album/01 New Title.mp3");
        assert!(plan.to_copy.is_empty() && plan.to_delete.is_empty());
    }

    #[test]
    fn test_plan_recopies_to_new_path_when_tags_and_file_change() {
        // タグをファイルに書き込んだ（配置も元のファイルも変わった）
        let mut edited = source("t1", "Artist", "Album", 1, "New Title");
        edited.file = Some(SourceFile {
            size: 130,
            modified_at: Some(50),
        });
        let old = "Artist/Album/01 Old Title.mp3";
        let plan = plan_sync(
            &[edited],
            &[],
            &manifest(vec![entry(old, "t1")]),
            true,
            device(&[(old, 100)]),
        );

        assert_eq!(paths(&plan.to_delete), [old]);
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 New Title.mp3"]);
        assert!(plan.to_rename.is_empty());
    }

    #[test]
    fn test_plan_ignores_case_only_tag_change() {
        let tracks = vec![source("t1", "The Artist", "Album", 1, "One")];
        let old = "the artist/Album/01 One.mp3";
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![entry(old, "t1")]),
            true,
            device(&[(old, 100)]),
        );
        assert_eq!(paths(&plan.kept), [old]);
        assert!(plan.to_rename.is_empty() && plan.to_copy.is_empty());
    }

    #[test]
    fn test_plan_numbers_colliding_paths_and_keeps_them_stable() {
        // 同じタグの別のトラック（大文字・小文字だけが違うものを含む）
        let tracks = vec![
            source("t1", "Artist", "Album", 1, "Same"),
            source("t2", "Artist", "Album", 1, "Same"),
            source("t3", "ARTIST", "Album", 1, "same"),
        ];
        let plan = plan_sync(&tracks, &[], &manifest(vec![]), true, device(&[]));
        assert_eq!(
            copy_dests(&plan),
            [
                "Artist/Album/01 Same.mp3",
                "Artist/Album/01 Same (2).mp3",
                "ARTIST/Album/01 same (3).mp3"
            ]
        );

        // 次の同期では、連番の付いたパスをそのまま使う（t1が対象から外れても詰めない）
        let files = vec![
            entry("Artist/Album/01 Same.mp3", "t1"),
            entry("Artist/Album/01 Same (2).mp3", "t2"),
        ];
        let on_device = [
            ("Artist/Album/01 Same.mp3", 100),
            ("Artist/Album/01 Same (2).mp3", 100),
        ];
        let plan = plan_sync(
            &tracks[1..2],
            &[],
            &manifest(files),
            true,
            device(&on_device),
        );
        assert_eq!(paths(&plan.kept), ["Artist/Album/01 Same (2).mp3"]);
        assert_eq!(paths(&plan.to_delete), ["Artist/Album/01 Same.mp3"]);
        assert!(plan.to_copy.is_empty() && plan.to_rename.is_empty());
    }

    #[test]
    fn test_plan_avoids_unmanaged_files() {
        // ユーザーが自分で置いたファイル（サイズが違う）は上書きしない
        let tracks = vec![source("t1", "Artist", "Album", 1, "One")];
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![]),
            true,
            device(&[("Artist/Album/01 One.mp3", 999)]),
        );
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One (2).mp3"]);
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn test_plan_adopts_same_size_file_at_same_path() {
        let tracks = vec![
            source("t1", "Artist", "Album", 1, "One"),
            source("new-id", "Artist", "Album", 2, "Two"),
        ];
        let plan = plan_sync(
            &tracks,
            &[],
            // t1は記録がなく（管理ファイルをなくした）、2曲目はトラックが登録し直された
            &manifest(vec![entry("Artist/Album/02 Two.mp3", "old-id")]),
            true,
            device(&[
                ("Artist/Album/01 One.mp3", 100),
                ("Artist/Album/02 Two.mp3", 100),
            ]),
        );

        assert!(plan.to_copy.is_empty() && plan.to_delete.is_empty());
        let mut kept: Vec<(&str, &str)> = plan
            .kept
            .iter()
            .map(|f| (f.path.as_str(), f.track_id.as_str()))
            .collect();
        kept.sort();
        assert_eq!(
            kept,
            [
                ("Artist/Album/01 One.mp3", "t1"),
                ("Artist/Album/02 Two.mp3", "new-id")
            ]
        );
    }

    #[test]
    fn test_plan_reuses_path_of_removed_track() {
        // 対象から外れた曲と同じ場所に、サイズの違う別の曲を置く（削除の後にコピーする）
        let mut replacement = source("t2", "Artist", "Album", 1, "One");
        replacement.file = Some(SourceFile {
            size: 150,
            modified_at: Some(20),
        });
        let path = "Artist/Album/01 One.mp3";
        let files = vec![entry(path, "t1")];

        let plan = plan_sync(
            std::slice::from_ref(&replacement),
            &[],
            &manifest(files.clone()),
            true,
            device(&[(path, 100)]),
        );
        assert_eq!(paths(&plan.to_delete), [path]);
        assert_eq!(copy_dests(&plan), [path]);

        // 削除しない設定では、残す曲の場所を避ける
        let plan = plan_sync(
            &[replacement],
            &[],
            &manifest(files),
            false,
            device(&[(path, 100)]),
        );
        assert_eq!(copy_dests(&plan), ["Artist/Album/01 One (2).mp3"]);
        assert_eq!(paths(&plan.kept), [path]);
    }

    #[test]
    fn test_plan_keeps_device_copy_when_source_is_missing() {
        let mut unplugged = source("t1", "Artist", "Album", 1, "One");
        unplugged.file = None;
        let mut never_copied = source("t2", "Artist", "Album", 2, "Two");
        never_copied.file = None;
        let path = "Artist/Album/01 One.mp3";

        let plan = plan_sync(
            &[unplugged, never_copied],
            &[],
            &manifest(vec![entry(path, "t1")]),
            true,
            device(&[(path, 100)]),
        );

        assert_eq!(plan.missing_source_count, 2);
        assert_eq!(paths(&plan.kept), [path]);
        assert!(plan.to_copy.is_empty() && plan.to_delete.is_empty());
    }

    #[test]
    fn test_plan_ignores_duplicate_manifest_entries() {
        let tracks = vec![source("t1", "Artist", "Album", 1, "One")];
        let path = "Artist/Album/01 One.mp3";
        let plan = plan_sync(
            &tracks,
            &[],
            &manifest(vec![
                entry(path, "t1"),
                // 同じパスの記録と、同じトラックの別のパスの記録
                entry(path, "t9"),
                entry("Artist/Album/01 One (2).mp3", "t1"),
            ]),
            true,
            device(&[(path, 100), ("Artist/Album/01 One (2).mp3", 100)]),
        );

        assert_eq!(paths(&plan.kept), [path]);
        assert_eq!(paths(&plan.to_delete), ["Artist/Album/01 One (2).mp3"]);
    }

    fn playlist(id: &str, name: &str, track_ids: &[&str]) -> SourcePlaylist {
        SourcePlaylist {
            id: id.to_string(),
            name: name.to_string(),
            track_ids: track_ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    fn playlist_entry(path: &str, playlist_id: &str) -> ManifestPlaylist {
        ManifestPlaylist {
            path: path.to_string(),
            playlist_id: playlist_id.to_string(),
        }
    }

    #[test]
    fn test_plan_playlists_names_files() {
        let tracks = vec![
            source("t1", "Artist", "Album", 1, "One"),
            source("t2", "Artist", "Album", 2, "Two"),
        ];
        let playlists = vec![
            playlist("p1", "通勤", &["t2", "t1", "not-selected"]),
            playlist("p2", "通勤", &[]),
            playlist("p3", "Road/Trip", &["t1"]),
        ];
        let plan = plan_sync(&tracks, &playlists, &manifest(vec![]), true, device(&[]));

        let files: Vec<&str> = plan.playlists.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(files, ["通勤.m3u8", "通勤 (2).m3u8", "Road_Trip.m3u8"]);
        // 曲順どおりで、同期する曲にないトラックは含めない
        let ids: Vec<&str> = plan.playlists[0]
            .entries
            .iter()
            .map(|e| e.track_id.as_str())
            .collect();
        assert_eq!(ids, ["t2", "t1"]);
        assert_eq!(plan.playlists[0].entries[0].label, "Artist - Two");
    }

    #[test]
    fn test_plan_playlists_handles_rename_and_removal() {
        let playlists = vec![
            playlist("p1", "通勤", &[]),
            playlist("p2", "新しい名前", &[]),
        ];
        let mut recorded = manifest(vec![]);
        recorded.playlists = vec![
            playlist_entry("通勤.m3u8", "p1"),
            playlist_entry("古い名前.m3u8", "p2"),
            playlist_entry("外した.m3u8", "p3"),
        ];
        let on_device = [
            ("通勤.m3u8", 10),
            ("古い名前.m3u8", 10),
            ("外した.m3u8", 10),
        ];

        let plan = plan_sync(&[], &playlists, &recorded, true, device(&on_device));
        let files: Vec<&str> = plan.playlists.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(files, ["通勤.m3u8", "新しい名前.m3u8"]);
        let deleted: Vec<&str> = plan
            .playlists_to_delete
            .iter()
            .map(|p| p.path.as_str())
            .collect();
        assert_eq!(deleted, ["古い名前.m3u8", "外した.m3u8"]);

        // 削除しない設定では、対象から外れたプレイリストは残す（名前を変えたものは消す）
        let plan = plan_sync(&[], &playlists, &recorded, false, device(&on_device));
        let deleted: Vec<&str> = plan
            .playlists_to_delete
            .iter()
            .map(|p| p.path.as_str())
            .collect();
        assert_eq!(deleted, ["古い名前.m3u8"]);
        assert_eq!(plan.kept_playlists, [playlist_entry("外した.m3u8", "p3")]);
    }

    #[test]
    fn test_plan_playlists_avoids_unmanaged_files() {
        let playlists = vec![playlist("p1", "通勤", &[])];
        let plan = plan_sync(
            &[],
            &playlists,
            &manifest(vec![]),
            true,
            device(&[("通勤.m3u8", 10)]),
        );
        assert_eq!(plan.playlists[0].path, "通勤 (2).m3u8");
    }

    #[test]
    fn test_build_m3u8() {
        let entries = vec![
            PlaylistEntry {
                track_id: "t1".to_string(),
                duration: Some(215),
                label: "Artist - One".to_string(),
            },
            PlaylistEntry {
                track_id: "not-on-device".to_string(),
                duration: Some(100),
                label: "Artist - Missing".to_string(),
            },
            PlaylistEntry {
                track_id: "t2".to_string(),
                duration: None,
                label: "Two".to_string(),
            },
        ];
        let track_paths = HashMap::from([
            ("t1", "Artist/Album/01 One.mp3"),
            ("t2", "Unknown Artist/Unknown Album/Two.flac"),
        ]);

        assert_eq!(
            build_m3u8(&entries, &track_paths),
            "#EXTM3U\r\n\
             #EXTINF:215,Artist - One\r\nArtist/Album/01 One.mp3\r\n\
             #EXTINF:-1,Two\r\nUnknown Artist/Unknown Album/Two.flac\r\n"
        );
    }

    #[test]
    fn test_track_label_replaces_control_characters() {
        let mut track = transfer_track("t1", "Artist", "Album", 1, "Line1\nLine2");
        assert_eq!(track_label(&track), "Artist - Line1 Line2");
        track.artist = None;
        assert_eq!(track_label(&track), "Line1 Line2");
    }
}
