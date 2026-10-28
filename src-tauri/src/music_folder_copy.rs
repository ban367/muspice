//! 取り込む曲の、音楽フォルダへのコピー（ADR-069）
//!
//! 取り込む曲を、音楽フォルダの書式（`Settings::music_folder_template`）どおりの場所へコピーする。
//! 計画（`plan`）と実行（`copy_files`・`copy_folder_images`）を分ける。置き方の決まり（値のない項目・
//! ファイル名に使えない文字）と、上書きしない決まりは、ファイルの整理（`file_organize`。ADR-052）と同じ。
//!
//! - 元のファイルは残す（移動はしない）
//! - コピー先に同じ名前のファイルがある曲・取り込む曲の中で同じコピー先になる曲は、コピーしない
//!   （上書きしない・連番も付けない）。コピー先のファイルが元と同じ内容なら、コピー済みとして扱う
//!   （同じファイルを2回取り込んだ場合）
//! - 曲と同じ名前の歌詞ファイル（`.lrc`）は、曲と一緒にコピーする
//! - フォルダの画像（`cover.jpg`など）は、フォルダの曲がすべて同じフォルダへコピーされた場合に、
//!   一緒にコピーする
//!
//! 曲の登録（データベース）は、呼び出し側（`commands::import`）が行う。

use crate::album_art::is_folder_image;
use crate::error::{AppError, AppResult};
use crate::file_organize::{
    DirCache, LYRICS_EXTENSION, Template, copy_no_replace, create_target_folder, list_dir,
    location_key, remove_created_folders, remove_empty_folders,
};
use crate::library::is_supported_audio_file;
use crate::models::Track;
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// コピーの読み書きの単位
pub const COPY_BUFFER_SIZE: usize = 1024 * 1024;

/// コピーしない理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NotCopiedReason {
    /// コピー先に、同じ名前のファイルがある
    TargetExists,
    /// コピー先の場所を、ライブラリの曲（ファイルが見つからない曲）が使っている（ファイルはない）
    TargetRegistered,
    /// 取り込む曲の中に、同じコピー先になる曲がある
    SameTarget,
}

/// コピーしなかった曲（インポートの結果に出す。曲は取り込んでいない）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotCopied {
    /// 元のファイルのパス
    pub source: String,
    /// コピー先（音楽フォルダからの場所）
    pub target: String,
    pub reason: NotCopiedReason,
}

/// コピーする候補（元のファイルから読んだ曲）
#[derive(Debug, Clone)]
pub struct CopySource {
    pub path: PathBuf,
    /// ファイルから読んだ曲（`file_path`は、元のファイル）
    pub track: Track,
}

/// 1曲のコピー（実行用）
#[derive(Debug, Clone)]
pub struct PlannedCopy {
    pub source: PathBuf,
    pub target: PathBuf,
    /// 一緒にコピーする歌詞ファイル（元, コピー先）
    pub lyrics: Option<(PathBuf, PathBuf)>,
    /// ファイルから読んだ曲（`file_path`は、元のファイル）
    pub track: Track,
}

/// コピーの計画
#[derive(Debug, Default)]
pub struct CopyPlan {
    /// コピーする曲（取り込む順）
    pub copies: Vec<PlannedCopy>,
    /// コピー先に、元と同じ内容のファイルがある曲（元, コピー先）。コピーせず、コピー先を取り込む
    pub already_copied: Vec<(PathBuf, PathBuf)>,
    pub not_copied: Vec<NotCopied>,
}

impl CopyPlan {
    /// コピーに要る容量（曲と、歌詞ファイルの大きさの合計。バイト）
    pub fn required_bytes(&self) -> u64 {
        let size = |path: &Path| fs::metadata(path).map_or(0, |metadata| metadata.len());
        self.copies
            .iter()
            .map(|planned| {
                let lyrics = planned.lyrics.as_ref().map_or(0, |(from, _)| size(from));
                u64::try_from(planned.track.file_size).unwrap_or(0) + lyrics
            })
            .sum()
    }
}

/// 取り込む曲の、コピーの計画を立てる
///
/// @param sources - コピーする候補（この順にコピーする）
/// @param music_folder - 音楽フォルダ
/// @param registered_paths - ライブラリに登録されている、すべての曲のパス（見つからない曲を含む。
///   ファイルがなくても、その場所はコピー先にしない）
pub fn plan(
    sources: Vec<CopySource>,
    music_folder: &Path,
    template: &Template,
    registered_paths: &HashSet<String>,
) -> CopyPlan {
    let mut dirs = DirCache::default();
    let candidates: Vec<PlannedCopy> = sources
        .into_iter()
        .map(|source| {
            // 書式に`/`がなくても、音楽フォルダの直下に置く（元のフォルダのままにはしない）
            let target = template
                .components(&source.track, &source.path)
                .iter()
                .fold(music_folder.to_path_buf(), |path, component| {
                    path.join(component)
                });
            let lyrics = dirs.lyrics_file(&source.path).map(|lyrics| {
                let copied = match lyrics.extension() {
                    Some(extension) => target.with_extension(extension),
                    None => target.with_extension(LYRICS_EXTENSION),
                };
                (lyrics, copied)
            });
            PlannedCopy {
                source: source.path,
                target,
                lyrics,
                track: source.track,
            }
        })
        .collect();

    // 取り込む曲の中で、同じコピー先になる曲（どれもコピーしない）
    let targets_of = |planned: &PlannedCopy| {
        let lyrics = planned.lyrics.as_ref().map(|(_, to)| to);
        std::iter::once(&planned.target)
            .chain(lyrics)
            .map(|to| location_key(to))
            .collect::<Vec<_>>()
    };
    let mut users: HashMap<String, usize> = HashMap::new();
    for planned in &candidates {
        for key in targets_of(planned) {
            *users.entry(key).or_default() += 1;
        }
    }

    let mut plan = CopyPlan::default();
    for planned in candidates {
        let not_copied = |reason| NotCopied {
            source: planned.source.to_string_lossy().into_owned(),
            target: planned
                .target
                .strip_prefix(music_folder)
                .unwrap_or(&planned.target)
                .to_string_lossy()
                .into_owned(),
            reason,
        };
        if targets_of(&planned)
            .iter()
            .any(|key| users.get(key).is_some_and(|count| *count > 1))
        {
            plan.not_copied
                .push(not_copied(NotCopiedReason::SameTarget));
            continue;
        }
        if fs::symlink_metadata(&planned.target).is_ok() {
            if is_same_content(&planned.source, &planned.target) {
                plan.already_copied.push((planned.source, planned.target));
            } else {
                plan.not_copied
                    .push(not_copied(NotCopiedReason::TargetExists));
            }
            continue;
        }
        // ファイルはないが、ライブラリの曲（見つからない曲）の場所。その曲が見つかった時に重なるため、
        // コピー先にしない（理由は、ファイルがある場合と分ける）
        if planned
            .target
            .to_str()
            .is_some_and(|to| registered_paths.contains(to))
        {
            plan.not_copied
                .push(not_copied(NotCopiedReason::TargetRegistered));
            continue;
        }
        if planned
            .lyrics
            .as_ref()
            .is_some_and(|(_, to)| fs::symlink_metadata(to).is_ok())
        {
            plan.not_copied
                .push(not_copied(NotCopiedReason::TargetExists));
            continue;
        }
        plan.copies.push(planned);
    }
    plan
}

/// 2つのファイルの中身が同じか（どちらかを読めない場合は、違うとみなす）
fn is_same_content(a: &Path, b: &Path) -> bool {
    let (Ok(a_meta), Ok(b_meta)) = (fs::metadata(a), fs::symlink_metadata(b)) else {
        return false;
    };
    if !b_meta.is_file() || a_meta.len() != b_meta.len() {
        return false;
    }
    let compare = || -> io::Result<bool> {
        let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
        let mut a_buffer = vec![0u8; 64 * 1024];
        let mut b_buffer = vec![0u8; 64 * 1024];
        loop {
            let read = a.read(&mut a_buffer)?;
            if read == 0 {
                // 大きさは同じため、もう一方も終わっている
                return Ok(true);
            }
            b.read_exact(&mut b_buffer[..read])?;
            if a_buffer[..read] != b_buffer[..read] {
                return Ok(false);
            }
        }
    };
    compare().unwrap_or(false)
}

/// 1曲をコピーした結果
#[derive(Debug, PartialEq)]
pub struct CopiedFiles {
    /// コピーした曲の大きさ（バイト）
    pub size: u64,
    /// 歌詞ファイルをコピーしたか（ない場合はfalse）
    pub lyrics_copied: bool,
    /// 歌詞ファイルをコピーできなかった場合の理由（曲のコピーは、成功として扱う）
    pub lyrics_error: Option<String>,
}

/// 1曲のファイル（と、同じ名前の歌詞ファイル）をコピーする
///
/// コピー先のフォルダは、なければ作る（コピーできなかった場合は、作ったフォルダを片付ける）。
pub fn copy_files(planned: &PlannedCopy, buffer: &mut [u8]) -> AppResult<CopiedFiles> {
    let created = match planned.target.parent() {
        Some(parent) => Some((
            parent,
            create_target_folder(parent).map_err(|e| {
                AppError::Io(format!("コピー先のフォルダを作れませんでした: {}", e))
            })?,
        )),
        None => None,
    };
    let size = copy_no_replace(&planned.source, &planned.target, buffer).map_err(|e| {
        if let Some((parent, existing)) = &created {
            remove_created_folders(parent, existing);
        }
        AppError::Io(if e.kind() == io::ErrorKind::AlreadyExists {
            "コピー先に同じ名前のファイルがあります".to_string()
        } else {
            format!("ファイルをコピーできませんでした: {}", e)
        })
    })?;
    // コピーした曲は、この後でライブラリへ登録する。音楽フォルダの監視が、このコピーで再スキャン
    // しないように（`own_writes`）
    crate::own_writes::record(&planned.target);

    let lyrics = planned
        .lyrics
        .as_ref()
        .map(|(from, to)| copy_no_replace(from, to, buffer));
    Ok(CopiedFiles {
        size,
        lyrics_copied: matches!(lyrics, Some(Ok(_))),
        lyrics_error: match lyrics {
            Some(Err(e)) => Some(format!("歌詞ファイルをコピーできませんでした: {}", e)),
            _ => None,
        },
    })
}

/// コピーしたファイルを消す（データベースに記録できなかった時に使う）
///
/// コピーで作ったフォルダも、空になれば消す（音楽フォルダ自体は消さない）。
/// @param lyrics_copied - 歌詞ファイルもコピーしたか（コピーしていなければ、コピー先の歌詞ファイルは
///   ほかのファイルのため、消さない）
pub fn remove_copy(planned: &PlannedCopy, lyrics_copied: bool, music_folder: &Path) {
    let remove = |path: &Path| {
        if let Err(e) = fs::remove_file(path) {
            log::error!("Failed to delete copied file: {}: {}", path.display(), e);
        }
    };
    remove(&planned.target);
    if let Some((_, to)) = planned.lyrics.as_ref().filter(|_| lyrics_copied) {
        remove(to);
    }
    if let Some(parent) = planned.target.parent() {
        remove_empty_folders(parent, music_folder);
    }
}

/// コピーした曲の、フォルダの画像をコピーする
///
/// フォルダの画像（`cover.jpg`など。埋め込みの画像がない曲のアルバムアートに使う）は、元のフォルダの
/// 曲がすべてコピーされ、コピー先が1つのフォルダで、そのフォルダへコピーされた曲がすべて同じフォルダから
/// 来た場合に、一緒にコピーする（アルバムのフォルダを、そのまま取り込んだ場合。ファイルの整理と同じ
/// 決まり）。コピー先に同じ名前のファイルがあれば、コピーしない（失敗はログに残す）。
/// @param copied - コピーした曲
pub fn copy_folder_images(copied: &[&PlannedCopy], buffer: &mut [u8]) {
    // 元のフォルダ → (コピー先のフォルダ, コピーした曲)
    let mut destinations: HashMap<&Path, (HashSet<&Path>, HashSet<String>)> = HashMap::new();
    // コピー先のフォルダ → 元のフォルダ
    let mut sources: HashMap<&Path, HashSet<&Path>> = HashMap::new();
    for planned in copied {
        let (Some(from_dir), Some(to_dir)) = (planned.source.parent(), planned.target.parent())
        else {
            continue;
        };
        let (to_dirs, tracks) = destinations.entry(from_dir).or_default();
        to_dirs.insert(to_dir);
        tracks.insert(location_key(&planned.source));
        sources.entry(to_dir).or_default().insert(from_dir);
    }

    for (from_dir, (to_dirs, tracks)) in destinations {
        let mut to_dirs = to_dirs.into_iter();
        let (Some(to_dir), None) = (to_dirs.next(), to_dirs.next()) else {
            continue;
        };
        if !sources.get(to_dir).is_some_and(|from| from.len() == 1) {
            continue;
        }
        let Ok(entries) = list_dir(from_dir) else {
            continue;
        };
        // 元のフォルダに、コピーしなかった曲があれば、画像はほかの曲のものかもしれない
        if entries
            .iter()
            .any(|path| is_supported_audio_file(path) && !tracks.contains(&location_key(path)))
        {
            continue;
        }
        for image in entries
            .iter()
            .filter(|path| is_folder_image(path) && path.is_file())
        {
            let Some(name) = image.file_name() else {
                continue;
            };
            if let Err(e) = copy_no_replace(image, &to_dir.join(name), buffer) {
                log::info!("Did not copy folder image: {}: {}", image.display(), e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_manifest::test_support::TempDir;
    use crate::models::ReplayGain;

    const TEMPLATE: &str = "%albumartist%/%album%/[%disc%-]%track% %title%";

    fn track(path: &Path, title: &str, number: i32) -> Track {
        Track {
            id: format!("t{}", number),
            file_path: path.to_str().unwrap().to_string(),
            file_name: path.file_name().unwrap().to_str().unwrap().to_string(),
            title: Some(title.to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: None,
            genre: None,
            year: None,
            track_number: Some(number),
            disc_number: None,
            duration: Some(200),
            file_size: fs::metadata(path).map_or(0, |m| m.len() as i64),
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
    fn write(path: &Path, content: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn source(path: &Path, title: &str, number: i32) -> CopySource {
        CopySource {
            path: path.to_path_buf(),
            track: track(path, title, number),
        }
    }

    fn plan_into(music: &Path, sources: Vec<CopySource>) -> CopyPlan {
        plan(
            sources,
            music,
            &Template::parse(TEMPLATE).unwrap(),
            &HashSet::new(),
        )
    }

    /// 計画どおりにコピーし、フォルダの画像もコピーする
    fn copy_all(plan: &CopyPlan) {
        let mut buffer = vec![0u8; 16];
        for planned in &plan.copies {
            copy_files(planned, &mut buffer).unwrap();
        }
        let copied: Vec<&PlannedCopy> = plan.copies.iter().collect();
        copy_folder_images(&copied, &mut buffer);
    }

    #[test]
    fn test_plan_places_tracks_by_the_template() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let downloads = dir.path().join("Downloads");
        let song = downloads.join("song.mp3");
        write(&song, b"song data");
        write(&downloads.join("song.lrc"), b"[00:01]la");

        let plan = plan_into(&music, vec![source(&song, "Song", 1)]);

        assert_eq!(plan.copies.len(), 1);
        assert_eq!(
            plan.copies[0].target,
            music.join("Artist/Album/01 Song.mp3")
        );
        assert_eq!(
            plan.copies[0].lyrics,
            Some((
                downloads.join("song.lrc"),
                music.join("Artist/Album/01 Song.lrc")
            ))
        );
        // 曲と歌詞ファイルの大きさの合計
        assert_eq!(plan.required_bytes(), 9 + 9);
        assert!(plan.not_copied.is_empty() && plan.already_copied.is_empty());
    }

    /// 書式に`/`がない場合も、音楽フォルダの直下に置く
    #[test]
    fn test_plan_without_folders_places_tracks_in_the_music_folder() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        write(&song, b"x");

        let plan = plan(
            vec![source(&song, "Song", 1)],
            &music,
            &Template::parse("%artist% - %title%").unwrap(),
            &HashSet::new(),
        );

        assert_eq!(plan.copies[0].target, music.join("Artist - Song.mp3"));
    }

    #[test]
    fn test_copy_keeps_the_source_and_its_modified_time() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        write(&song, b"0123456789abcdefghij");
        let modified =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(&song)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let plan = plan_into(&music, vec![source(&song, "Song", 1)]);

        // 読み書きの単位より大きなファイルも、全体をコピーする
        let copied = copy_files(&plan.copies[0], &mut [0u8; 8]).unwrap();

        let target = music.join("Artist/Album/01 Song.mp3");
        assert_eq!(
            copied,
            CopiedFiles {
                size: 20,
                lyrics_copied: false,
                lyrics_error: None
            }
        );
        assert_eq!(fs::read(&target).unwrap(), b"0123456789abcdefghij");
        assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), modified);
        assert!(song.is_file());
        // 一時ファイルは残らない
        assert_eq!(list_dir(target.parent().unwrap()).unwrap(), [target]);
    }

    #[test]
    fn test_plan_does_not_overwrite_existing_files() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        let other = dir.path().join("Downloads/other.mp3");
        let missing = dir.path().join("Downloads/missing.mp3");
        write(&song, b"new");
        write(&other, b"other");
        write(&missing, b"missing");
        write(&music.join("Artist/Album/01 Song.mp3"), b"old");
        // ファイルはないが、見つからない曲として登録されている場所
        let registered = music.join("Artist/Album/03 Missing.mp3");

        let plan = plan(
            vec![
                source(&song, "Song", 1),
                source(&other, "Other", 2),
                source(&missing, "Missing", 3),
            ],
            &music,
            &Template::parse(TEMPLATE).unwrap(),
            &HashSet::from([registered.to_str().unwrap().to_string()]),
        );

        let relative = |path: &str| Path::new(path).to_string_lossy().into_owned();
        assert_eq!(
            plan.not_copied,
            [
                NotCopied {
                    source: song.to_str().unwrap().to_string(),
                    target: relative("Artist/Album/01 Song.mp3"),
                    reason: NotCopiedReason::TargetExists,
                },
                NotCopied {
                    source: missing.to_str().unwrap().to_string(),
                    target: relative("Artist/Album/03 Missing.mp3"),
                    reason: NotCopiedReason::TargetRegistered,
                }
            ]
        );
        assert_eq!(plan.copies.len(), 1);
        assert_eq!(plan.copies[0].source, other);
    }

    #[test]
    fn test_plan_does_not_copy_tracks_sharing_a_target() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let first = dir.path().join("a/one.mp3");
        let second = dir.path().join("b/two.mp3");
        let third = dir.path().join("b/three.mp3");
        for path in [&first, &second, &third] {
            write(path, b"x");
        }
        // 大文字と小文字だけが違うコピー先も、同じコピー先とみなす
        let plan = plan_into(
            &music,
            vec![
                source(&first, "Song", 1),
                source(&second, "song", 1),
                source(&third, "Other", 2),
            ],
        );

        let reasons: Vec<NotCopiedReason> = plan.not_copied.iter().map(|n| n.reason).collect();
        assert_eq!(
            reasons,
            [NotCopiedReason::SameTarget, NotCopiedReason::SameTarget]
        );
        assert_eq!(plan.copies.len(), 1);
        assert_eq!(plan.copies[0].source, third);
    }

    /// コピー先に、元と同じ内容のファイルがあれば、コピー済みとして扱う
    #[test]
    fn test_plan_treats_identical_targets_as_already_copied() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        let changed = dir.path().join("Downloads/changed.mp3");
        write(&song, b"same data");
        write(&changed, b"new data!");
        write(&music.join("Artist/Album/01 Song.mp3"), b"same data");
        // 大きさが同じで、中身が違う
        write(&music.join("Artist/Album/02 Changed.mp3"), b"old data!");

        let plan = plan_into(
            &music,
            vec![source(&song, "Song", 1), source(&changed, "Changed", 2)],
        );

        assert_eq!(
            plan.already_copied,
            [(song, music.join("Artist/Album/01 Song.mp3"))]
        );
        assert_eq!(plan.not_copied.len(), 1);
        assert_eq!(plan.not_copied[0].reason, NotCopiedReason::TargetExists);
        assert!(plan.copies.is_empty());
    }

    #[test]
    fn test_plan_does_not_copy_when_lyrics_target_exists() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        write(&song, b"x");
        write(&dir.path().join("Downloads/song.lrc"), b"new");
        write(&music.join("Artist/Album/01 Song.lrc"), b"old");

        let plan = plan_into(&music, vec![source(&song, "Song", 1)]);

        assert_eq!(plan.not_copied[0].reason, NotCopiedReason::TargetExists);
        assert!(plan.copies.is_empty());
    }

    #[test]
    fn test_copy_never_overwrites_files_placed_after_planning() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        write(&song, b"new");
        let plan = plan_into(&music, vec![source(&song, "Song", 1)]);
        // 計画を立てた後に、コピー先へ別のファイルが置かれた
        let target = music.join("Artist/Album/01 Song.mp3");
        write(&target, b"other");

        let error = copy_files(&plan.copies[0], &mut [0u8; 16]).unwrap_err();

        assert!(error.to_string().contains("同じ名前のファイル"));
        assert_eq!(fs::read(&target).unwrap(), b"other");
        // 一時ファイルは残らない
        assert_eq!(list_dir(target.parent().unwrap()).unwrap(), [target]);
    }

    #[test]
    fn test_remove_copy_removes_only_the_copied_files() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("Downloads/song.mp3");
        let lyrics = dir.path().join("Downloads/song.lrc");
        write(&song, b"x");
        write(&lyrics, b"la");
        write(&music.join("Keep/notes.txt"), b"keep");
        let plan = plan_into(&music, vec![source(&song, "Song", 1)]);
        let copied = copy_files(&plan.copies[0], &mut [0u8; 16]).unwrap();
        assert!(copied.lyrics_copied);

        remove_copy(&plan.copies[0], copied.lyrics_copied, &music);

        // コピーで作ったフォルダも消す（音楽フォルダ・ほかのフォルダは残す）
        assert!(!music.join("Artist").exists());
        assert!(music.join("Keep/notes.txt").is_file());
        assert!(song.is_file() && lyrics.is_file());
    }

    #[test]
    fn test_folder_images_are_copied_with_the_whole_album() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let rip = dir.path().join("rip");
        let first = rip.join("a.mp3");
        let second = rip.join("b.mp3");
        write(&first, b"a");
        write(&second, b"b");
        write(&rip.join("cover.jpg"), b"image");
        write(&rip.join("notes.txt"), b"text");

        copy_all(&plan_into(
            &music,
            vec![source(&first, "One", 1), source(&second, "Two", 2)],
        ));

        let album = music.join("Artist/Album");
        assert!(album.join("01 One.mp3").is_file());
        assert!(album.join("02 Two.mp3").is_file());
        assert_eq!(fs::read(album.join("cover.jpg")).unwrap(), b"image");
        assert!(!album.join("notes.txt").exists());
        // 元のファイルは残す
        assert!(rip.join("cover.jpg").is_file());
    }

    #[test]
    fn test_folder_images_stay_when_album_is_partly_copied_or_split() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");

        // フォルダの一部の曲だけをコピーした: 画像はコピーしない
        let picked = dir.path().join("mixed/a.mp3");
        write(&picked, b"a");
        write(&dir.path().join("mixed/not picked.mp3"), b"b");
        write(&dir.path().join("mixed/cover.jpg"), b"image");
        copy_all(&plan_into(&music, vec![source(&picked, "Song", 1)]));
        assert!(music.join("Artist/Album/01 Song.mp3").is_file());
        assert!(!music.join("Artist/Album/cover.jpg").exists());

        // 曲が別々のフォルダへコピーされた: 画像はコピーしない
        let one = dir.path().join("split/a.mp3");
        let two = dir.path().join("split/b.mp3");
        write(&one, b"a");
        write(&two, b"b");
        write(&dir.path().join("split/folder.png"), b"image");
        let mut other_album = source(&two, "Other", 2);
        other_album.track.album = Some("Other Album".to_string());
        copy_all(&plan_into(
            &music,
            vec![source(&one, "First", 3), other_album],
        ));
        assert!(music.join("Artist/Other Album/02 Other.mp3").is_file());
        assert!(!music.join("Artist/Other Album/folder.png").exists());
    }

    #[test]
    fn test_folder_images_never_overwrite() {
        let dir = TempDir::new();
        let music = dir.path().join("Music");
        let song = dir.path().join("rip/a.mp3");
        write(&song, b"a");
        write(&dir.path().join("rip/cover.jpg"), b"new");
        write(&music.join("Artist/Album/cover.jpg"), b"old");

        copy_all(&plan_into(&music, vec![source(&song, "Song", 1)]));

        assert_eq!(
            fs::read(music.join("Artist/Album/cover.jpg")).unwrap(),
            b"old"
        );
    }
}
